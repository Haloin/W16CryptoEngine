use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use common::{AppError, AppResult, UserId};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const JWT_HEADER: &str = r#"{"alg":"HS256","typ":"JWT"}"#;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub:      String,
    pub iat:      i64,
    pub exp:      i64,
    pub is_admin: bool,
}

impl Claims {
    pub fn user_id(&self) -> AppResult<UserId> {
        uuid::Uuid::parse_str(&self.sub)
            .map(UserId)
            .map_err(|_| AppError::Unauthorized)
    }

    pub fn is_expired(&self) -> bool {
        self.exp < Utc::now().timestamp()
    }
}

#[derive(Clone)]
pub struct JwtService {
    secret:   Vec<u8>,
    ttl_secs: i64,
}

impl JwtService {
    pub fn new(secret: &[u8], ttl_secs: i64) -> Self {
        Self {
            secret:   secret.to_vec(),
            ttl_secs,
        }
    }

    pub fn issue(&self, user_id: UserId, is_admin: bool) -> AppResult<String> {
        let now = Utc::now();
        let claims = Claims {
            sub:      user_id.0.to_string(),
            iat:      now.timestamp(),
            exp:      (now + Duration::seconds(self.ttl_secs)).timestamp(),
            is_admin,
        };

        let header = URL_SAFE_NO_PAD.encode(JWT_HEADER.as_bytes());
        let payload = URL_SAFE_NO_PAD.encode(
            serde_json::to_vec(&claims)
                .map_err(|e| AppError::Internal(e.to_string()))?
                .as_slice(),
        );

        let signing_input = format!("{header}.{payload}");
        let sig = self.sign(signing_input.as_bytes())?;

        Ok(format!("{signing_input}.{sig}"))
    }

    pub fn verify(&self, token: &str) -> AppResult<Claims> {
        let parts: Vec<&str> = token.splitn(3, '.').collect();
        if parts.len() != 3 {
            return Err(AppError::Unauthorized);
        }

        let signing_input = format!("{}.{}", parts[0], parts[1]);
        let expected = self.sign(signing_input.as_bytes())?;

        if !constant_eq(parts[2].as_bytes(), expected.as_bytes()) {
            return Err(AppError::Unauthorized);
        }

        let payload = URL_SAFE_NO_PAD
            .decode(parts[1])
            .map_err(|_| AppError::Unauthorized)?;

        let claims: Claims =
            serde_json::from_slice(&payload).map_err(|_| AppError::Unauthorized)?;

        if claims.is_expired() {
            return Err(AppError::Unauthorized);
        }

        Ok(claims)
    }

    fn sign(&self, input: &[u8]) -> AppResult<String> {
        let mut mac = HmacSha256::new_from_slice(&self.secret)
            .map_err(|_| AppError::Internal("hmac init failed".into()))?;
        mac.update(input);
        let result = mac.finalize();
        Ok(URL_SAFE_NO_PAD.encode(result.into_bytes()))
    }
}

fn constant_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b.iter()).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

pub struct PasswordService;

impl PasswordService {
    pub fn hash(password: &str) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(format!("password hash failed: {e}")))
    }

    pub fn verify(password: &str, hash: &str) -> AppResult<()> {
        let parsed = PasswordHash::new(hash).map_err(|_| AppError::Internal("invalid hash".into()))?;
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .map_err(|_| AppError::Unauthorized)
    }
}
