use analytics::{RiskManager, RiskLimits, RiskCheckResult};
use common::{MarketId, OrderId, OrderKind, OrderRequest, Price, Quantity, Side, UserId};
use orders::reconciliation::SlippageConfig;
use orders::{OrderService, OrderReconciler};
use std::sync::Arc;

#[tokio::test]
async fn test_risk_enforcement_blocks_oversized_order() {
    let risk_limits = RiskLimits {
        max_position_size_usd: 1000.0,
        max_drawdown_pct: 5.0,
        max_total_exposure_usd: 2000.0,
        max_daily_loss_usd: 500.0,
        max_positions_per_market: 10,
    };

    let risk_manager = RiskManager::new(risk_limits.clone());
    let user_id = UserId::default();
    let market_id = MarketId::default();

    risk_manager.update_position(user_id, market_id, 900.0).await;

    let order = common::Order {
    let order = common::Order {
        id: OrderId::default(),
        user_id,
        market_id,
        side: Side::Buy,
        price: Price(500_000), // $50.00
        quantity: Quantity(200),       // Would exceed $1K limit
        filled: Quantity(0),
        status: common::OrderStatus::Open,
        created_at: chrono::Utc::now(),
        kind: OrderKind::Limit,
    };

    let check_result = risk_manager.check_order_risk(user_id, &order, 900.0).await;
    
    assert!(
        !check_result.allowed,
        "Order exceeding position limit should be rejected, got: {:?}",
        check_result
    );
}

#[tokio::test]
async fn test_risk_enforcement_accepts_valid_order() {
    let risk_limits = RiskLimits {
        max_position_size_usd: 1000.0,
        max_drawdown_pct: 5.0,
        max_total_exposure_usd: 2000.0,
        max_daily_loss_usd: 500.0,
        max_positions_per_market: 10,
    };

    let risk_manager = RiskManager::new(risk_limits);
    let user_id = UserId::default();
    let market_id = MarketId::default();

    let order = common::Order {
        id: OrderId::default(),
        user_id,
        market_id,
        side: Side::Buy,
        price: Price(100_000), // $10.00
        quantity: Quantity(10),       // $100 total
        filled: Quantity(0),
        status: common::OrderStatus::Open,
        created_at: chrono::Utc::now(),
        kind: OrderKind::Limit,
    };

    let check_result = risk_manager.check_order_risk(user_id, &order, 0.0).await;
    
    assert!(
        check_result.allowed,
        "Small order should be approved, got: {:?}",
        check_result
    );
}

#[tokio::test]
async fn test_slippage_protection_rejects_excessive_slippage() {
    let slippage_config = SlippageConfig {
        max_market_order_slippage_bps: 100,
    };

    let order_price = 100.0;
    let fill_price = 100.60;

    let result = OrderReconciler::new(slippage_config)
        .check_slippage(order_price, fill_price, false);

    assert!(
        result.is_err(),
        "Fill with excessive slippage should be rejected"
    );
}

#[tokio::test]
async fn test_slippage_protection_accepts_acceptable_slippage() {
    let slippage_config = SlippageConfig {
        max_market_order_slippage_bps: 100,
    };

    let order_price = 100.0;
    let fill_price = 100.30;

    let result = OrderReconciler::new(slippage_config)
        .check_slippage(order_price, fill_price, false);

    assert!(
        result.is_ok(),
        "Fill with acceptable slippage should be accepted"
    );
}
