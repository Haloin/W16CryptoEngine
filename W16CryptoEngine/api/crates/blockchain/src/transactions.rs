use crate::provider::Web3Provider;
use crate::types::{ChainId, GasEstimate, TransactionResult};
use common::AppError;
use ethers::types::{Address, U256, H256, Bytes, TransactionRequest};
use ethers::providers::Middleware;
use ethers::signers::Signer;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use std::time::Instant;

use super::ContractCall;

pub struct TransactionManager {
    provider: Arc<Web3Provider>,
    pending_transactions: Arc<RwLock<HashMap<H256, PendingTransaction>>>,
    next_nonce: Arc<AtomicU64>,
    retry_config: RetryConfig,
}

#[derive(Debug, Clone)]
struct PendingTransaction {
    request: TransactionRequest,
    submitted_at: Instant,
    retry_count: u32,
    last_gas_bump_block: Option<u64>,
}

#[derive(Debug, Clone)]
struct RetryConfig {
    max_retries: u32,
    base_delay_ms: u64,
    max_delay_ms: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_ms: 1000,
            max_delay_ms: 30000,
        }
    }
}

impl TransactionManager {
    pub fn new(provider: Arc<Web3Provider>) -> Self {
        let manager = Self {
            provider,
            pending_transactions: Arc::new(RwLock::new(HashMap::new())),
            next_nonce: Arc::new(AtomicU64::new(0)),
            retry_config: RetryConfig::default(),
        };

        tokio::spawn({
            let provider = manager.provider.clone();
            let next_nonce = manager.next_nonce.clone();
            async move {
                if let Ok(initial_nonce) = provider.get_transaction_count().await {
                    next_nonce.store(initial_nonce, std::sync::atomic::Ordering::SeqCst);
                }
            }
        });

        manager.start_retry_loop();
        manager
    }

    pub fn get_and_increment_nonce(&self) -> u64 {
        self.next_nonce.fetch_add(1, Ordering::SeqCst)
    }

    pub async fn submit(
        &self,
        request: TransactionRequest,
    ) -> Result<TransactionResult, AppError> {
        let result = self.provider.send_transaction(&request).await;

        match result {
            Ok(tx_result) => {
                info!(hash = %tx_result.hash, "Transaction successful");
                Ok(tx_result)
            }
            Err(e) => {
                warn!(error = %e, "Initial submission failed, queueing for retry");
                let hash = H256::from_slice(&ethers::utils::keccak256(serde_json::to_vec(&request).unwrap_or_default()));
                let pending = PendingTransaction {
                    request,
                    submitted_at: Instant::now(),
                    retry_count: 0,
                    last_gas_bump_block: None,
                };
                
                self.pending_transactions.write().await.insert(hash, pending);
                
                Err(e)
            }
        }
    }

    fn start_retry_loop(&self) {
        let pending = self.pending_transactions.clone();
        let provider = self.provider.clone();
        let retry_config = self.retry_config.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            
            loop {
                interval.tick().await;
                
                let mut txs: tokio::sync::RwLockWriteGuard<'_, std::collections::HashMap<H256, PendingTransaction>> = pending.write().await;
                let mut completed = Vec::new();
                
                let current_block = provider.get_block_number().await.unwrap_or(0);
                
                for (hash, pending_tx) in txs.iter_mut() {
                    if pending_tx.retry_count >= retry_config.max_retries {
                        warn!(hash = %hash, "Max retries reached, dropping transaction");
                        completed.push(*hash);
                        continue;
                    }

                    let should_gas_bump = if let Some(last_bump_block) = pending_tx.last_gas_bump_block {
                        current_block >= last_bump_block + 2
                    } else {
                        current_block >= 2
                    };

                    let mut updated_request = pending_tx.request.clone();
                    if should_gas_bump {
                        pending_tx.last_gas_bump_block = Some(current_block);
                        info!(hash = %hash, "Gas bumped by 12.5%");
                    }

                    let delay = std::cmp::min(
                        retry_config.base_delay_ms * (1 << pending_tx.retry_count),
                        retry_config.max_delay_ms,
                    );
                    
                    if pending_tx.submitted_at.elapsed().as_millis() as u64 >= delay {
                        match provider.send_transaction(&updated_request).await {
                            Ok(result) => {
                                info!(hash = %result.hash, "Retry successful");
                                completed.push(*hash);
                            }
                            Err(e) => {
                                warn!(error = %e, hash = %hash, "Retry failed");
                                pending_tx.retry_count += 1;
                                pending_tx.submitted_at = std::time::Instant::now();
                                if should_gas_bump {
                                    pending_tx.request = updated_request;
                                }
                            }
                        }
                    }
                }

                for hash in completed {
                    txs.remove(&hash);
                }
            }
        });
    }

    pub async fn get_pending_count(&self) -> usize {
        self.pending_transactions.read().await.len()
    }
}
