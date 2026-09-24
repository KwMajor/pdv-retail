//! Repositório de `"user"`. Sem `DELETE` físico: só `deactivate` (soft delete).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::User;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewUser {
    pub store_id: Uuid,
    pub name: String,
    pub email: String,
    pub password_hash: String,
    /// `admin` | `manager` | `cashier`.
    pub role: String,
}

pub trait UserRepository {
    async fn create(&self, input: NewUser) -> Result<User, sqlx::Error>;
    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<User>, sqlx::Error>;
    async fn find_by_email(
        &self,
        store_id: Uuid,
        email: &str,
    ) -> Result<Option<User>, sqlx::Error>;
    async fn list_active(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<User>, sqlx::Error>;
    async fn set_pin_hash(
        &self,
        store_id: Uuid,
        id: Uuid,
        pin_hash: &str,
    ) -> Result<User, sqlx::Error>;
    /// Soft delete (`is_active = FALSE`). Preserva FKs do histórico.
    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<User, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgUserRepository {
    pool: sqlx::PgPool,
}

impl PgUserRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for PgUserRepository {
    async fn create(&self, input: NewUser) -> Result<User, sqlx::Error> {
        sqlx::query_as!(
            User,
            "INSERT INTO \"user\"(store_id, name, email, password_hash, role)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at",
            input.store_id,
            input.name,
            input.email,
            input.password_hash,
            input.role,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            "SELECT id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at
             FROM \"user\" WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_email(
        &self,
        store_id: Uuid,
        email: &str,
    ) -> Result<Option<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            "SELECT id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at
             FROM \"user\" WHERE store_id = $1 AND email = $2",
            store_id,
            email,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_active(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<User>, sqlx::Error> {
        sqlx::query_as!(
            User,
            "SELECT id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at
             FROM \"user\" WHERE store_id = $1 AND is_active = TRUE
             ORDER BY created_at DESC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn set_pin_hash(
        &self,
        store_id: Uuid,
        id: Uuid,
        pin_hash: &str,
    ) -> Result<User, sqlx::Error> {
        sqlx::query_as!(
            User,
            "UPDATE \"user\" SET pin_hash = $3 WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at",
            store_id,
            id,
            pin_hash,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<User, sqlx::Error> {
        sqlx::query_as!(
            User,
            "UPDATE \"user\" SET is_active = FALSE WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, name, email, password_hash, role, pin_hash, is_active, created_at, updated_at",
            store_id,
            id,
        )
        .fetch_one(&self.pool)
        .await
    }
}
