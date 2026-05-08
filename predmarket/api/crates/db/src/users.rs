use crate::Db;
use common::{AppError, AppResult, UserId};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct UserRow {
    pub id:            Uuid,
    pub email:         String,
    pub password_hash: String,
    pub is_admin:      bool,
}

impl Db {
    pub async fn insert_user(&self, email: &str, password_hash: &str) -> AppResult<UserId> {
        let id: Uuid = sqlx::query_scalar(
            "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id",
        )
        .bind(email)
        .bind(password_hash)
        .fetch_one(&self.pool)
        .await?;

        Ok(UserId(id))
    }

    pub async fn get_user_by_email(&self, email: &str) -> AppResult<UserRow> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, is_admin FROM users WHERE email = $1",
        )
        .bind(email)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {email}")))
    }

    pub async fn get_user_by_id(&self, id: UserId) -> AppResult<UserRow> {
        sqlx::query_as::<_, UserRow>(
            "SELECT id, email, password_hash, is_admin FROM users WHERE id = $1",
        )
        .bind(id.0)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("user {id}")))
    }

    pub async fn audit(
        &self,
        user_id:   Option<UserId>,
        action:    &str,
        entity:    &str,
        entity_id: Option<&str>,
        meta:      Option<serde_json::Value>,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO audit_log (user_id, action, entity, entity_id, meta)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(user_id.map(|u| u.0))
        .bind(action)
        .bind(entity)
        .bind(entity_id)
        .bind(meta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
