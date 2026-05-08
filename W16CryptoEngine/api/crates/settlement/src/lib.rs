use chrono::Utc;
use common::{AppError, AppResult, MarketId, MarketStatus, Outcome, UserId};
use db::Db;
use std::sync::Arc;
use tracing::info;

pub struct SettlementService {
    db: Arc<Db>,
}

impl SettlementService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn settle(
        &self,
        market_id:   MarketId,
        outcome:     Outcome,
        resolved_by: UserId,
    ) -> AppResult<SettlementSummary> {
        let market = self.db.get_market(market_id).await?;

        if market.status != MarketStatus::Open && market.status != MarketStatus::Paused {
            return Err(AppError::BadRequest(format!(
                "market cannot be settled from status {:?}",
                market.status
            )));
        }

        if let Some(resolves_at) = market.resolves_at {
            if Utc::now() < resolves_at {
                return Err(AppError::BadRequest(
                    "resolution window has not opened yet".into(),
                ));
            }
        }

        let position_rows = self.db.get_user_positions(market_id).await?;

        let payouts: Vec<(UserId, i64)> = position_rows
            .iter()
            .filter_map(|row| {
                let net = row.net_quantity;
                if net <= 0 {
                    return None;
                }
                let user_id = UserId(row.user_id);
                let amount  = match outcome {
                    Outcome::Yes => net,
                    Outcome::No  => 0,
                };
                if amount == 0 {
                    return None;
                }
                Some((user_id, amount))
            })
            .collect();

        let total_payout: i64 = payouts.iter().map(|(_, p)| p).sum();

        self.db
            .apply_settlement_payouts(market_id, outcome, &payouts)
            .await?;

        info!(
            market_id    = %market_id,
            outcome      = ?outcome,
            resolved_by  = %resolved_by,
            payout_count = payouts.len(),
            total_payout,
            "market settled"
        );

        Ok(SettlementSummary { market_id, outcome, payouts, total_payout })
    }
}

#[derive(Debug)]
pub struct SettlementSummary {
    pub market_id:    MarketId,
    pub outcome:      Outcome,
    pub payouts:      Vec<(UserId, i64)>,
    pub total_payout: i64,
}
