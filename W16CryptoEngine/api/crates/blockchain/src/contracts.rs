use crate::types::{ChainId, GasEstimate, TransactionResult};
use crate::provider::Web3Provider;
use std::sync::Arc;
use common::AppError;
use ethers::types::{Address, U256, Bytes, TransactionReceipt, TransactionRequest};
use ethers::providers::Middleware;
use ethers::signers::Signer;
use crate::types::ContractCall;
use tracing::info;

pub struct ConditionalTokensContract {
    provider: Arc<Web3Provider>,
    contract_address: Address,
}

impl ConditionalTokensContract {
    pub fn new(provider: Arc<Web3Provider>, contract_address: Address) -> Self {
        Self {
            provider,
            contract_address,
        }
    }

    pub async fn get_balance(&self, user: Address, token_id: U256) -> Result<U256, AppError> {
        let call_data = self.encode_balance_of(user, token_id);
        
        let result = self.provider.call_contract(&ContractCall {
            contract_address: self.contract_address,
            function_signature: "balanceOf(address,uint256)".to_string(),
            parameters: vec![format!("{:?}", user), token_id.to_string()],
            value: U256::zero(),
        }).await?;

        if result.len() >= 32 {
            Ok(U256::from_big_endian(&result[0..32]))
        } else {
            Err(AppError::Internal("Invalid balance response".to_string()))
        }
    }

    pub async fn transfer(
        &self,
        from: Address,
        to: Address,
        token_id: U256,
        amount: U256,
    ) -> Result<TransactionResult, AppError> {
        let data = self.encode_transfer(to, token_id, amount);
        
        let tx = TransactionRequest {
            from: None,
            to: Some(ethers::types::NameOrAddress::Address(self.contract_address)),
            value: Some(U256::zero()),
            data: Some(data.into()),
            gas: Some(U256::zero()),
            gas_price: None,
            chain_id: None,
            nonce: None,
        };

        self.provider.send_transaction(&tx).await
    }

    pub async fn split_position(
        &self,
        market_id: U256,
        outcome_index: u8,
        amount: U256,
    ) -> Result<TransactionResult, AppError> {
        let data = self.encode_split_position(market_id, outcome_index, amount);
        
        let tx = TransactionRequest {
            from: None,
            to: Some(ethers::types::NameOrAddress::Address(self.contract_address)),
            value: Some(U256::zero()),
            data: Some(data.into()),
            gas: Some(U256::zero()),
            gas_price: None,
            chain_id: None,
            nonce: None,
        };

        self.provider.send_transaction(&tx).await
    }

    pub async fn merge_positions(
        &self,
        market_id: U256,
        outcome_indices: Vec<u8>,
        amount: U256,
    ) -> Result<TransactionResult, AppError> {
        let data = self.encode_merge_positions(market_id, outcome_indices, amount);
        
        let tx = TransactionRequest {
            from: None,
            to: Some(ethers::types::NameOrAddress::Address(self.contract_address)),
            value: Some(U256::zero()),
            data: Some(data.into()),
            gas: Some(U256::zero()),
            gas_price: None,
            chain_id: None,
            nonce: None,
        };

        self.provider.send_transaction(&tx).await
    }

    pub async fn redeem_positions(
        &self,
        market_id: U256,
        outcome_indices: Vec<u8>,
        amount: U256,
    ) -> Result<TransactionResult, AppError> {
        let data = self.encode_redeem_positions(market_id, outcome_indices, amount);
        
        let tx = TransactionRequest {
            from: None,
            to: Some(ethers::types::NameOrAddress::Address(self.contract_address)),
            value: Some(U256::zero()),
            data: Some(data.into()),
            gas: Some(U256::zero()),
            gas_price: None,
            chain_id: None,
            nonce: None,
        };

        self.provider.send_transaction(&tx).await
    }

    fn encode_balance_of(&self, user: Address, token_id: U256) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&hex::decode("00fdd58e").unwrap_or_default());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(user.as_bytes());
        let mut token_buf = [0u8; 32];
        token_id.to_big_endian(&mut token_buf);
        data.extend_from_slice(&token_buf);
        data
    }

    fn encode_transfer(&self, to: Address, token_id: U256, amount: U256) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&hex::decode("f242432a").unwrap_or_default());
        data.extend_from_slice(&[0u8; 12]);
        data.extend_from_slice(to.as_bytes());
        let mut token_buf = [0u8; 32];
        token_id.to_big_endian(&mut token_buf);
        data.extend_from_slice(&token_buf);
        let mut amount_buf = [0u8; 32];
        amount.to_big_endian(&mut amount_buf);
        data.extend_from_slice(&amount_buf);
        data.extend_from_slice(&[0u8; 32]);
        data
    }

    fn encode_split_position(&self, market_id: U256, outcome_index: u8, amount: U256) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&hex::decode("4c830cbd").unwrap_or_default());
        let mut market_buf = [0u8; 32];
        market_id.to_big_endian(&mut market_buf);
        data.extend_from_slice(&market_buf);
        data.push(outcome_index);
        let mut amount_buf = [0u8; 32];
        amount.to_big_endian(&mut amount_buf);
        data.extend_from_slice(&amount_buf);
        data
    }

    fn encode_merge_positions(&self, market_id: U256, outcome_indices: Vec<u8>, amount: U256) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&hex::decode("8f3ea0d1").unwrap_or_default());
        let mut market_buf = [0u8; 32];
        market_id.to_big_endian(&mut market_buf);
        data.extend_from_slice(&market_buf);
        
        let indices = U256::from(0);
        for (i, &idx) in outcome_indices.iter().enumerate() {
            if idx > 0 {
                let bit = U256::from(1) << (idx as usize);
            }
        }
        let mut indices_buf = [0u8; 32];
        indices.to_big_endian(&mut indices_buf);
        data.extend_from_slice(&indices_buf);
        let mut amount_buf = [0u8; 32];
        amount.to_big_endian(&mut amount_buf);
        data.extend_from_slice(&amount_buf);
        data
    }

    fn encode_redeem_positions(&self, market_id: U256, outcome_indices: Vec<u8>, amount: U256) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&hex::decode("8f3ea0d1").unwrap_or_default());
        let mut market_buf = [0u8; 32];
        market_id.to_big_endian(&mut market_buf);
        data.extend_from_slice(&market_buf);
        
        let indices = U256::from(0);
        let mut indices_buf = [0u8; 32];
        indices.to_big_endian(&mut indices_buf);
        data.extend_from_slice(&indices_buf);
        let mut amount_buf = [0u8; 32];
        amount.to_big_endian(&mut amount_buf);
        data.extend_from_slice(&amount_buf);
        data
    }
}
