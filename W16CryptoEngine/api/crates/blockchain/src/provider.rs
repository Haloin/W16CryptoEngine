use crate::types::{ChainId, GasEstimate, TransactionResult};
use common::AppError;
use ethers::types::{Address, U256, Bytes, TransactionReceipt, TransactionRequest};
use ethers::providers::{Provider, Ws, Middleware};
use ethers::signers::{LocalWallet, Signer};
use ethers::types::transaction::eip2718::TypedTransaction;
use ethers::types::Eip1559TransactionRequest;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, error};
use chrono::Utc;

use super::ContractCall;

pub struct Web3Provider {
    provider: Arc<Provider<Ws>>,
    wallet: LocalWallet,
    chain_id: ChainId,
    nonce: Arc<RwLock<u64>>,
    gas_oracle: Arc<dyn GasOracle>,
}

impl Web3Provider {
    pub async fn new(rpc_url: &str, private_key: &str, chain_id: ChainId) -> Result<Self, AppError> {
        let provider = Provider::<Ws>::connect(rpc_url).await.map_err(|e| {
            AppError::Internal(format!("WebSocket connect failed: {}", e))
        })?;

        let chain_id_clone = chain_id.clone();
        let wallet = private_key.parse::<LocalWallet>().map_err(|e| {
            AppError::Internal(format!("Key parse failed: {}", e))
        })?.with_chain_id(chain_id as u64);

        let gas_oracle = Arc::new(PolygonGasOracle::new(Arc::new(provider.clone())).await?);

        let provider = Self {
            provider: Arc::new(provider),
            wallet,
            chain_id: chain_id_clone,
            nonce: Arc::new(RwLock::new(0)),
            gas_oracle,
        };

        provider.initialize_nonce().await?;
        
        info!(address = %provider.wallet.address(), "Web3 provider initialized");
        Ok(provider)
    }

    async fn initialize_nonce(&self) -> Result<(), AppError> {
        let address = self.wallet.address();
        let nonce = self.provider
            .get_transaction_count(address, None)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get nonce: {}", e)))?;
        
        *self.nonce.write().await = nonce.as_u64();
        debug!(nonce = %nonce, "Nonce initialized");
        Ok(())
    }

    pub async fn get_balance(&self, address: Address) -> Result<U256, AppError> {
        self.provider
            .get_balance(address, None)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get balance: {}", e)))
    }

    pub async fn get_token_balance(
        &self,
        token_address: Address,
        wallet_address: Address,
    ) -> Result<U256, AppError> {
        let contract = ethers::contract::Contract::new(
            token_address,
            match serde_json::from_str::<ethers::abi::Abi>(ERC20_ABI) {
                Ok(abi) => abi,
                Err(e) => return Err(AppError::Internal(format!("Failed to parse ERC20 ABI: {}", e))),
            },
            self.provider.clone(),
        );

        let balance: U256 = contract
            .method::<_, U256>("balanceOf", wallet_address)
            .map_err(|e| AppError::Internal(format!("Contract error: {}", e)))?
            .call()
            .await
            .map_err(|e| AppError::Internal(format!("Call failed: {}", e)))?;

        Ok(balance)
    }

    pub async fn send_transaction(
        &self,
        tx: &TransactionRequest,
    ) -> Result<TransactionResult, AppError> {
        let gas_estimate = self.gas_oracle.estimate_gas().await?;
        
        let mut nonce = self.nonce.write().await;
        
        let tx_request: Eip1559TransactionRequest = Eip1559TransactionRequest::new()
            .to(tx.to.clone().unwrap_or(ethers::types::NameOrAddress::Address(ethers::types::H160::zero())))
            .value(tx.value.unwrap_or_default())
            .data(tx.data.clone().unwrap_or_default())
            .gas(gas_estimate.gas_limit as u64)
            .max_fee_per_gas(gas_estimate.max_fee_per_gas)
            .max_priority_fee_per_gas(gas_estimate.max_priority_fee_per_gas)
            .nonce(*nonce);

        let typed_tx: TypedTransaction = tx_request.into();
        let signed_tx = self.wallet.sign_transaction(&typed_tx).await.map_err(|e| {
            AppError::Internal("Failed to sign transaction".to_string())
        })?;
        let raw_tx = typed_tx.rlp_signed(&signed_tx);

        let pending_tx = self.provider.send_raw_transaction(raw_tx).await.map_err(|e| {
            AppError::Internal("Failed to send transaction".to_string())
        })?;

        info!(hash = %pending_tx.tx_hash(), "Transaction sent, waiting for confirmation...");

        let receipt = pending_tx.confirmations(1).await.map_err(|e| {
            AppError::Internal("Failed to confirm transaction".to_string())
        })?.ok_or_else(|| AppError::Internal("Transaction receipt not found".to_string()))?;

        *nonce += 1;
        drop(nonce);

        let result = TransactionResult {
            hash: receipt.transaction_hash,
            nonce: receipt.transaction_index.as_u64(),
            gas_price: receipt.effective_gas_price.unwrap_or_else(U256::zero),
            gas_used: receipt.gas_used.unwrap_or_else(U256::zero),
            status: receipt.status.map(|s| s.as_u64() == 1).unwrap_or(false),
            block_number: receipt.block_number.map(|n| n.as_u64()).unwrap_or(0),
            timestamp: Utc::now(),
        };

        info!(hash = %result.hash, status = result.status, "Transaction confirmed");
        Ok(result)
    }

    pub async fn call_contract(&self, contract_call: &ContractCall) -> Result<Bytes, AppError> {
        let tx = Eip1559TransactionRequest::new()
            .to(contract_call.contract_address)
            .data(Bytes::from(contract_call.parameters.join("").into_bytes()))
            .value(contract_call.value);

        self.provider
            .call(&tx.into(), None)
            .await
            .map_err(|e| AppError::Internal(format!("Contract call failed: {}", e)))
    }

    pub async fn get_gas_price(&self) -> Result<GasEstimate, AppError> {
        self.gas_oracle.estimate_gas().await
    }

    pub async fn get_transaction_count(&self) -> Result<u64, AppError> {
        let address = self.wallet.address();
        let nonce = self.provider
            .get_transaction_count(address, None)
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get nonce: {}", e)))?;
        
        Ok(nonce.low_u64())
    }

    pub async fn get_block_number(&self) -> Result<u64, AppError> {
        let block = self.provider
            .get_block_number()
            .await
            .map_err(|e| AppError::Internal(format!("Failed to get block number: {}", e)))?;
        Ok(block.low_u64())
    }

    pub fn address(&self) -> Address {
        self.wallet.address()
    }

    pub fn chain_id(&self) -> ChainId {
        self.chain_id.clone()
    }
}

#[async_trait::async_trait]
trait GasOracle: Send + Sync {
    async fn estimate_gas(&self) -> Result<GasEstimate, AppError>;
}

struct PolygonGasOracle {
    provider: Arc<Provider<Ws>>,
}

impl PolygonGasOracle {
    async fn new(provider: Arc<Provider<Ws>>) -> Result<Self, AppError> {
        Ok(Self { provider })
    }
}

#[async_trait::async_trait]
impl GasOracle for PolygonGasOracle {
    async fn estimate_gas(&self) -> Result<GasEstimate, AppError> {
        let base_fee = self.provider.get_gas_price().await.map_err(|e| {
            AppError::Internal(format!("Failed to get gas price: {}", e))
        })?;

        let max_priority_fee = U256::from(30_000_000_000u64);
        let max_fee = base_fee * 2 + max_priority_fee;
        let gas_limit = 500_000u64;

        let matic_price_usd = 0.5;
        let estimated_cost_wei: U256 = max_fee * gas_limit;
        let estimated_cost_matic = estimated_cost_wei.as_u64() as f64 / 1e18;
        let estimated_cost_usd = estimated_cost_matic * matic_price_usd;

        Ok(GasEstimate {
            gas_limit,
            max_fee_per_gas: max_fee,
            max_priority_fee_per_gas: max_priority_fee,
            estimated_cost_usd,
        })
    }
}

const ERC20_ABI: &str = r#"[
    {
        "constant": true,
        "inputs": [{"name": "_owner", "type": "address"}],
        "name": "balanceOf",
        "outputs": [{"name": "balance", "type": "uint256"}],
        "type": "function"
    }
]"#;
