use ethers::core::types::{Address, U256, H256, TransactionRequest as EthTransactionRequest};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionRequest {
    pub to: Address,
    pub value: U256,
    pub data: Vec<u8>,
    pub gas_limit: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub nonce: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionResult {
    pub hash: H256,
    pub nonce: u64,
    pub gas_price: U256,
    pub gas_used: U256,
    pub status: bool,
    pub block_number: u64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletBalance {
    pub address: Address,
    pub native_balance: U256,
    pub token_balances: Vec<TokenBalance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBalance {
    pub token_address: Address,
    pub symbol: String,
    pub decimals: u8,
    pub balance: U256,
}

#[derive(Debug, Clone)]
pub enum ChainId {
    Mainnet = 1,
    Polygon = 137,
    Mumbai = 80001,
}

impl From<ChainId> for u64 {
    fn from(chain: ChainId) -> Self {
        match chain {
            ChainId::Mainnet => 1,
            ChainId::Polygon => 137,
            ChainId::Mumbai => 80001,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasEstimate {
    pub gas_limit: u64,
    pub max_fee_per_gas: U256,
    pub max_priority_fee_per_gas: U256,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContractCall {
    pub contract_address: Address,
    pub function_signature: String,
    pub parameters: Vec<String>,
    pub value: U256,
}
