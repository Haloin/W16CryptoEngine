use chrono::Utc;
use common::{AppResult, Fill, FillId, MarketId, OrderId, OrderStatus, Price, Quantity, Side, UserId};
use db::{fills::InsertFillOutcome, Db};
use futures::StreamExt;
use messaging::{decode_fill_event, Nats};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

pub struct FillProcessor {
    db:   Arc<Db>,
    nats: Arc<Nats>,
}

impl FillProcessor {
    pub fn new(db: Arc<Db>, nats: Arc<Nats>) -> Self {
        Self { db, nats }
    }

    pub fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                if let Err(e) = self.run_loop().await {
                    error!(error = %e, "fill processor crashed, restarting");
                    tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                }
            }
        })
    }

    async fn run_loop(&self) -> AppResult<()> {
        let mut sub = self.nats.subscribe_fills().await?;

        while let Some(msg) = sub.next().await {
            let Some(event) = decode_fill_event(&msg.payload) else {
                warn!("unparseable fill payload, skipping");
                continue;
            };

            if let Err(e) = self.process(&event).await {
                error!(fill_id = %event.fill_id, error = %e, "fill processing failed");
            }
        }

        Ok(())
    }

    async fn process(&self, event: &messaging::EngineFillEvent) -> AppResult<()> {
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
            transaction_id: None,
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
