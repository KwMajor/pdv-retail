//! Repositório da tabela raiz `store_settings` (única sem `store_id`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::StoreSettings;

/// Payload de criação de loja.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStoreSettings {
    pub name: String,
    pub cnpj: String,
}

pub trait StoreSettingsRepository {
    async fn create(&self, input: NewStoreSettings) -> Result<StoreSettings, sqlx::Error>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<StoreSettings>, sqlx::Error>;
    async fn list(&self, limit: i64) -> Result<Vec<StoreSettings>, sqlx::Error>;
    async fn rename(&self, id: Uuid, name: &str) -> Result<StoreSettings, sqlx::Error>;
    /// Retorna linhas afetadas. Com dados vinculados o banco barra
    /// (`ON DELETE RESTRICT`) — ver teste da Task 1.1.
    async fn delete(&self, id: Uuid) -> Result<u64, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgStoreSettingsRepository {
    pool: sqlx::PgPool,
}

impl PgStoreSettingsRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl StoreSettingsRepository for PgStoreSettingsRepository {
    async fn create(&self, input: NewStoreSettings) -> Result<StoreSettings, sqlx::Error> {
        sqlx::query_as!(
            StoreSettings,
            "INSERT INTO store_settings(name, cnpj) VALUES ($1, $2)
             RETURNING id, name, cnpj, created_at, updated_at",
            input.name,
            input.cnpj,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(&self, id: Uuid) -> Result<Option<StoreSettings>, sqlx::Error> {
        sqlx::query_as!(
            StoreSettings,
            "SELECT id, name, cnpj, created_at, updated_at
             FROM store_settings WHERE id = $1",
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list(&self, limit: i64) -> Result<Vec<StoreSettings>, sqlx::Error> {
        sqlx::query_as!(
            StoreSettings,
            "SELECT id, name, cnpj, created_at, updated_at
             FROM store_settings ORDER BY created_at DESC LIMIT $1",
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn rename(&self, id: Uuid, name: &str) -> Result<StoreSettings, sqlx::Error> {
        sqlx::query_as!(
            StoreSettings,
            "UPDATE store_settings SET name = $2 WHERE id = $1
             RETURNING id, name, cnpj, created_at, updated_at",
            id,
            name,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn delete(&self, id: Uuid) -> Result<u64, sqlx::Error> {
        sqlx::query!("DELETE FROM store_settings WHERE id = $1", id)
            .execute(&self.pool)
            .await
            .map(|r| r.rows_affected())
    }
}
