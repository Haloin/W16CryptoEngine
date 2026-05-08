use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;
use utoipa::ToSchema;
use uuid::Uuid;
use rust_decimal::{Decimal, prelude::*};

macro_rules! uuid_newtype {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}

uuid_newtype!(UserId);
uuid_newtype!(MarketId);
uuid_newtype!(OrderId);
uuid_newtype!(FillId);


#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Price(pub u32);

impl Price {
    pub const MIN: Self = Self(1_000);
    pub const MAX: Self = Self(9_999_999);

    pub fn from_decimal(d: Decimal) -> Option<Self> {
        if d <= Decimal::ZERO {
            return None;
        }
        let raw = (d * Decimal::from(1_000_000)).round().to_u32()?;
        if raw < 1_000 || raw > 9_999_999 {
            return None;
        }
        Some(Self(raw))
    }

    pub fn to_decimal(self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(1_000_000)
    }

    pub fn complement(self) -> Self {
        Self(10_000_000 - self.0)
    }

    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

impl fmt::Display for Price {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.6}", self.to_decimal())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct Quantity(pub u64);

impl Quantity {
    pub const MIN: Self = Self(1_000);

    pub fn from_decimal(d: Decimal) -> Self {
        Self((d * Decimal::from(1_000_000)).round().to_u64().unwrap_or(0))
    }

    pub fn to_decimal(self) -> Decimal {
        Decimal::from(self.0) / Decimal::from(1_000_000)
    }

    pub fn is_zero(self) -> bool {
        self.0 == 0
    }

    pub fn saturating_sub(self, rhs: Self) -> Self {
        Self(self.0.saturating_sub(rhs.0))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "order_side", rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

impl Side {
    pub fn opposite(self) -> Self {
        match self {
            Side::Buy  => Side::Sell,
            Side::Sell => Side::Buy,
        }
    }
}

impl fmt::Display for Side {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Side::Buy  => write!(f, "buy"),
            Side::Sell => write!(f, "sell"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "order_type", rename_all = "lowercase")]
pub enum OrderKind {
    Limit,
    Market,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "order_status", rename_all = "lowercase")]
pub enum OrderStatus {
    Open,
    Partial,
    Filled,
    Cancelled,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "market_status", rename_all = "lowercase")]
pub enum MarketStatus {
    Open,
    Paused,
    Settled,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "outcome", rename_all = "lowercase")]
pub enum Outcome {
    Yes,
    No,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Market {
    pub id:          MarketId,
    pub title:       String,
    pub description: String,
    pub status:      MarketStatus,
    pub created_at:  DateTime<Utc>,
    pub resolves_at: Option<DateTime<Utc>>,
    pub settled_at:  Option<DateTime<Utc>>,
    pub outcome:     Option<Outcome>,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Order {
    pub id:              OrderId,
    pub market_id:       MarketId,
    pub user_id:         UserId,
    pub side:            Side,
    pub kind:            OrderKind,
    pub price:           Option<Price>,
    pub quantity:        Quantity,
    pub filled_quantity: Quantity,
    pub status:          OrderStatus,
    pub sequence:        u64,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl Order {
    pub fn remaining(&self) -> Quantity {
        self.quantity.saturating_sub(self.filled_quantity)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Fill {
    pub id:             FillId,
    pub market_id:      MarketId,
    pub maker_order_id: OrderId,
    pub taker_order_id: OrderId,
    pub maker_user_id:  UserId,
    pub taker_user_id:  UserId,
    pub price:          Price,
    pub quantity:       Quantity,
    pub aggressor:      Side,
    pub sequence:       u64,
    pub filled_at:      DateTime<Utc>,
    pub transaction_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub market_id: MarketId,
    pub side:      Side,
    pub kind:      OrderKind,
    pub price:     Option<Price>,
    pub quantity:  Quantity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceLevel {
    pub price:    Price,
    pub quantity: Quantity,
    pub count:    u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub market_id: MarketId,
    pub bids:      Vec<PriceLevel>,
    pub asks:      Vec<PriceLevel>,
    pub sequence:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Position {
    pub id:            PositionId,
    pub user_id:       UserId,
    pub market_id:     MarketId,
    pub side:          Side,
    pub entry_price:   Price,
    pub current_price: Price,
    pub quantity:      Quantity,
    pub unrealized_pnl: i64,
    pub realized_pnl:   i64,
    pub opened_at:     DateTime<Utc>,
    pub updated_at:    DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct PositionId(pub Uuid);

impl PositionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for PositionId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for PositionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct Signal {
    pub id:              SignalId,
    pub market_id:       MarketId,
    pub signal_type:     SignalType,
    pub strength:        f64,
    pub confidence:      f64,
    pub expected_return: i64,
    pub time_horizon:    u64,
    pub created_at:      DateTime<Utc>,
    pub expires_at:      DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, ToSchema)]
#[serde(transparent)]
pub struct SignalId(pub Uuid);

impl SignalId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for SignalId {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for SignalId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
    Close,
}
