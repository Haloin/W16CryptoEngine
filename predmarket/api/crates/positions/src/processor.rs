use chrono::Utc;
use common::{AppResult, Fill, OrderId, OrderStatus, Price, Quantity};
use db::{fills::InsertFillOutcome, Db};
use messaging::{decode_fill_event, EngineFillEvent, Nats};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

const DLQ_RETRY_COUNT: usize = 3;

pub struct FillProcessor {
    db: Arc<Db>,
    nats: Arc<Nats>,
    dlq_tx: mpsc::Sender<FailedFill>,
}

#[derive(Debug, Clone)]
struct FailedFill {
    event: EngineFillEvent,
    retries: usize,
}

impl FillProcessor {
    pub fn new(db: Arc<Db>, nats: Arc<Nats>) -> (Self, mpsc::Receiver<FailedFill>) {
        let (dlq_tx, dlq_rx) = mpsc::channel(1000);
        let processor = Self { db, nats, dlq_tx };
        (processor, dlq_rx)
    }

    pub fn spawn_workers(
        self: Arc<Self>,
        worker_count: usize,
        mut dlq_rx: mpsc::Receiver<FailedFill>,
    ) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::with_capacity(worker_count + 1);

        for worker_id in 0..worker_count {
            let processor = Arc::clone(&self);
            handles.push(tokio::spawn(async move {
                processor.run_worker(worker_id).await;
            }));
        }

        let processor = Arc::clone(&self);
        handles.push(tokio::spawn(async move {
            processor.run_dlq_processor(&mut dlq_rx).await;
        }));

        handles
    }

    async fn run_worker(&self, worker_id: usize) {
        let partition_suffix = format!(".{}", worker_id);
        let subject = format!("{}{}", self.nats.fill_subject_base(), partition_suffix);

        loop {
            match self.process_partition(&subject).await {
                Ok(()) => {
                    warn!(worker_id, "partition processor exited, restarting");
                }
                Err(e) => {
                    error!(worker_id, error = %e, "partition processor crashed, restarting");
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }

    async fn process_partition(&self, subject: &str) -> AppResult<()> {
        let mut sub = self.nats.subscribe_to_subject(subject).await?;

        while let Some(msg) = sub.next().await {
            let Some(event) = decode_fill_event(&msg.payload) else {
                warn!("unparseable fill payload, skipping");
                continue;
            };

            if let Err(e) = self.process(&event).await {
                error!(fill_id = %event.fill_id, error = %e, "fill processing failed, sending to DLQ");
                let _ = self.dlq_tx.send(FailedFill { event, retries: 0 }).await;
            }
        }

        Ok(())
    }

    async fn run_dlq_processor(&self, rx: &mut mpsc::Receiver<FailedFill>) {
        while let Some(failed) = rx.recv().await {
            if failed.retries >= DLQ_RETRY_COUNT {
                error!(
                    fill_id = %failed.event.fill_id,
                    retries = failed.retries,
                    "fill exceeded max retries, dropping"
                );
                continue;
            }

            tokio::time::sleep(std::time::Duration::from_millis(100 * (failed.retries as u64 + 1))).await;

            if let Err(e) = self.process(&failed.event).await {
                error!(
                    fill_id = %failed.event.fill_id,
                    retries = failed.retries,
                    error = %e,
                    "DLQ retry failed"
                );
                let _ = self.dlq_tx.send(FailedFill {
                    event: failed.event,
                    retries: failed.retries + 1,
                }).await;
            } else {
                info!(fill_id = %failed.event.fill_id, "DLQ retry succeeded");
            }
        }
    }

    async fn process(&self, event: &EngineFillEvent) -> AppResult<()> {
        let fill = Fill {
            id:             event.fill_id(),
            market_id:      event.market_id(),
            maker_order_id: event.maker_order_id(),
            taker_order_id: event.taker_order_id(),
            maker_user_id:  event.maker_user_id(),
            taker_user_id:  event.taker_user_id(),
            price:          event.price(),
            quantity:       event.quantity(),
            aggressor:      event.aggressor(),
            sequence:       event.sequence,
            filled_at:      Utc::now(),
        };

        match self.db.insert_fill_dedup(&fill).await? {
            InsertFillOutcome::Duplicate => {
                warn!(
                    fill_id  = %fill.id,
                    sequence = fill.sequence,
                    "duplicate fill received, skipping"
                );
                return Ok(());
            }
            InsertFillOutcome::Inserted => {}
        }

        let fill_cost = (fill.price.0 as i64)
            .checked_mul(fill.quantity.0 as i64)
            .and_then(|v| v.checked_div(10_000))
            .unwrap_or(0);

        self.db
            .apply_fill_balances(fill.taker_user_id, fill.maker_user_id, fill_cost)
            .await?;

        let buyer_delta  =  fill.quantity.0 as i64;
        let seller_delta = -(fill.quantity.0 as i64);

        self.db
            .record_position_change(fill.id, fill.taker_user_id, fill.market_id, buyer_delta)
            .await?;

        self.db
            .record_position_change(fill.id, fill.maker_user_id, fill.market_id, seller_delta)
            .await?;

        self.update_orders(&fill).await;

        info!(
            fill_id   = %fill.id,
            market_id = %fill.market_id,
            price     = fill.price.0,
            quantity  = fill.quantity.0,
            "fill processed"
        );

        Ok(())
    }

    async fn update_orders(&self, fill: &Fill) {
        for &order_id in &[fill.maker_order_id, fill.taker_order_id] {
            let order = match self.db.get_order(order_id).await {
                Ok(o) => o,
                Err(e) => {
                    warn!(order_id = %order_id, error = %e, "could not load order for fill update");
                    continue;
                }
            };

            let new_filled = Quantity(order.filled_quantity.0.saturating_add(fill.quantity.0));
            let status = if new_filled.0 >= order.quantity.0 {
                OrderStatus::Filled
            } else {
                OrderStatus::Partial
            };

            if let Err(e) = self.db.update_order_fill(order_id, status, new_filled).await {
                warn!(order_id = %order_id, error = %e, "failed updating order after fill");
            }
        }
    }
}
