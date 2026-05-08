use std::env;
use ethers::core::types::{Signature, H256, U256};
use ethers::signers::{Signer, Wallet};
use ethers::utils::keccak256;
use serde::{Deserialize, Serialize};

const ORDER_TYPE_HASH: &str = "0x40c3893b0e68c223937b5f6d094d9e923019880e32802c993b7002a2a563fae2";
const DOMAIN_SEPARATOR: &str = "0x66db50918f76348ef6263890cec205fd5e93df07f1f2a0967e9ac0e6d3e52c4c";

#[derive(Debug, Clone)]
pub struct Eip712Signer {
    wallet: Wallet<ethers::core::k256::ecdsa::SigningKey>,
    chain_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Eip712Domain {
    pub name: String,
    pub version: String,
    pub chain_id: u64,
    pub verifying_contract: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderMessage {
    pub maker: String,
    pub taker: String,
    pub token_id: u64,
    pub maker_amount: u64,
    pub taker_amount: u64,
    pub side: u8,
    pub expiration: u64,
    pub nonce: u64,
    pub fee_rate_bps: u64,
    pub signature_type: u8,
}

impl Eip712Signer {
    pub fn new(private_key: &str, chain_id: u64) -> Result<Self, common::AppError> {
        let wallet = private_key
            .parse::<Wallet<ethers::core::k256::ecdsa::SigningKey>>()
            .map_err(|e| common::AppError::Internal(format!("Invalid private key: {}", e)))?;

        Ok(Self { wallet, chain_id })
    }

    pub fn address(&self) -> String {
        format!("{:?}", self.wallet.address())
    }

    pub fn sign_order(&self, order: &OrderMessage) -> Result<String, common::AppError> {
        let struct_hash = self.hash_order_struct(order)?;
        let domain_separator = self.hash_domain_separator()?;
        let digest = self.hash_typed_data_v4(domain_separator, struct_hash)?;

        let signature = self
            .wallet
            .sign_hash(digest)
            .map_err(|e| common::AppError::Internal(format!("Sign failed: {}", e)))?;

        Ok(format!("0x{}", hex::encode(signature.to_vec())))
    }

    fn hash_order_struct(&self, order: &OrderMessage) -> Result<H256, common::AppError> {
        let type_hash = hex::decode(&ORDER_TYPE_HASH[2..])
            .map_err(|e| common::AppError::Internal(format!("Type hash invalid: {}", e)))?;

        let maker = hex::decode(&order.maker[2..])
            .map_err(|e| common::AppError::Internal(format!("Maker address invalid: {}", e)))?;
        let taker = hex::decode(&order.taker[2..])
            .map_err(|e| common::AppError::Internal(format!("Taker address invalid: {}", e)))?;

        let mut encoded = Vec::new();
        encoded.extend_from_slice(&type_hash);
        encoded.extend_from_slice(&Self::pad_32(&maker)?);
        encoded.extend_from_slice(&Self::pad_32(&taker)?);
        encoded.extend_from_slice(&Self::u256_to_bytes(order.token_id));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.maker_amount));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.taker_amount));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.side as u64));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.expiration));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.nonce));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.fee_rate_bps));
        encoded.extend_from_slice(&Self::u256_to_bytes(order.signature_type as u64));

        let hash = keccak256(&encoded);
        Ok(H256::from(hash))
    }

    fn hash_domain_separator(&self) -> Result<H256, common::AppError> {
        let domain = Eip712Domain {
            name: "Clob".to_string(),
            version: "1".to_string(),
            chain_id: self.chain_id,
            verifying_contract: env::var("CONTRACT_ADDRESS")
                .unwrap_or_else(|_| "0x4bFb41d5B3570DeFd03C39a9A4D8dEEDBdC96028".to_string()),
        };

        let type_hash = keccak256(b"EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)");

        let name_hash = keccak256(domain.name.as_bytes());
        let version_hash = keccak256(domain.version.as_bytes());

        let mut encoded = Vec::new();
        encoded.extend_from_slice(&type_hash);
        encoded.extend_from_slice(&name_hash);
        encoded.extend_from_slice(&version_hash);
        encoded.extend_from_slice(&Self::u256_to_bytes(domain.chain_id));
        encoded.extend_from_slice(&Self::pad_32(
            &hex::decode(&domain.verifying_contract[2..]).map_err(|e| {
                common::AppError::Internal(format!("Contract address invalid: {}", e))
            })?,
        )?);

        let hash = keccak256(&encoded);
        Ok(H256::from(hash))
    }

    fn hash_typed_data_v4(&self, domain: H256, struct_hash: H256) -> Result<H256, common::AppError> {
        let prefix = b"\x19\x01";
        let mut encoded = Vec::new();
        encoded.extend_from_slice(prefix);
        encoded.extend_from_slice(domain.as_bytes());
        encoded.extend_from_slice(struct_hash.as_bytes());

        let hash = keccak256(&encoded);
        Ok(H256::from(hash))
    }

    fn pad_32(bytes: &[u8]) -> Result<[u8; 32], common::AppError> {
        if bytes.len() > 32 {
            return Err(common::AppError::Internal(
                "Byte array too long".to_string(),
            ));
        }
        let mut padded = [0u8; 32];
        padded[32 - bytes.len()..].copy_from_slice(bytes);
        Ok(padded)
    }

    fn u256_to_bytes(value: u64) -> [u8; 32] {
        let mut bytes = [0u8; 32];
        bytes[24..].copy_from_slice(&value.to_be_bytes());
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_DUMMY_KEY: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";

    #[test]
    fn test_eip712_signing() {
        let signer = match Eip712Signer::new(TEST_DUMMY_KEY, 137) {
            Ok(s) => s,
            Err(e) => return println!("Failed to create signer: {:?}", e),
        };

        let order = OrderMessage {
            maker: signer.address(),
            taker: "0x0000000000000000000000000000000000000000".to_string(),
            token_id: 1,
            maker_amount: 1000000,
            taker_amount: 1000000,
            side: 0,
            expiration: 1893456000,
            nonce: 1,
            fee_rate_bps: 100,
            signature_type: 0,
        };

        let signature = match signer.sign_order(&order) {
            Ok(sig) => sig,
            Err(e) => return println!("Failed to sign order: {:?}", e),
        };
        assert!(signature.starts_with("0x"));
        assert_eq!(signature.len(), 132);
    }
}
