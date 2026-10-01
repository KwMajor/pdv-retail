//! Repositório de `stock` (saldo consolidado). Leitura + `upsert`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Stock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpsertStock {
    pub store_id: Uuid,
    pub product_id: Uuid,
    pub quantity: Decimal,
}

pub trait StockRepository {
    async fn get(&self, store_id: Uuid, product_id: Uuid) -> Result<Option<Stock>, sqlx::Error>;
    async fn list_by_store(&self, store_id: Uuid, limit: i64) -> Result<Vec<Stock>, sqlx::Error>;
    /// Cria ou substitui o saldo (`UNIQUE(store_id, product_id)`).
    async fn upsert(&self, input: UpsertStock) -> Result<Stock, sqlx::Error>;
    /// Soma atômica ao saldo (cria a linha se inexistente). Uma única
    /// instrução: sem read-modify-write, sem venda perdida em concorrência.
    async fn add_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        store_id: Uuid,
        product_id: Uuid,
        delta: Decimal,
    ) -> Result<Stock, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgStockRepository {
    pool: sqlx::PgPool,
}

impl PgStockRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl StockRepository for PgStockRepository {
    async fn get(&self, store_id: Uuid, product_id: Uuid) -> Result<Option<Stock>, sqlx::Error> {
        sqlx::query_as!(
            Stock,
            "SELECT id, store_id, product_id, quantity, created_at, updated_at
             FROM stock WHERE store_id = $1 AND product_id = $2",
            store_id,
            product_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_store(&self, store_id: Uuid, limit: i64) -> Result<Vec<Stock>, sqlx::Error> {
        sqlx::query_as!(
            Stock,
            "SELECT id, store_id, product_id, quantity, created_at, updated_at
             FROM stock WHERE store_id = $1
             ORDER BY updated_at DESC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn upsert(&self, input: UpsertStock) -> Result<Stock, sqlx::Error> {
        sqlx::query_as!(
            Stock,
            "INSERT INTO stock(store_id, product_id, quantity) VALUES ($1, $2, $3)
             ON CONFLICT (store_id, product_id)
             DO UPDATE SET quantity = EXCLUDED.quantity
             RETURNING id, store_id, product_id, quantity, created_at, updated_at",
            input.store_id,
            input.product_id,
            input.quantity,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn add_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        store_id: Uuid,
        product_id: Uuid,
        delta: Decimal,
    ) -> Result<Stock, sqlx::Error> {
        sqlx::query_as!(
            Stock,
            "INSERT INTO stock(store_id, product_id, quantity) VALUES ($1, $2, $3)
             ON CONFLICT (store_id, product_id)
             DO UPDATE SET quantity = stock.quantity + EXCLUDED.quantity
             RETURNING id, store_id, product_id, quantity, created_at, updated_at",
            store_id,
            product_id,
            delta,
        )
        .fetch_one(&mut **tx)
        .await
    }
}
