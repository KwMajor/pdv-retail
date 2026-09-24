//! Repositório do ledger `stock_movement` (US04). Append-only: o banco barra
//! `UPDATE`/`DELETE` via trigger — aqui só `record` e leituras.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::StockMovement;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStockMovement {
    pub store_id: Uuid,
    pub product_id: Uuid,
    pub qty_delta: Decimal,
    pub reason: String,
    pub ref_sale_id: Option<Uuid>,
}

pub trait StockMovementRepository {
    async fn record(&self, input: NewStockMovement) -> Result<StockMovement, sqlx::Error>;
    async fn list_by_product(
        &self,
        store_id: Uuid,
        product_id: Uuid,
        limit: i64,
    ) -> Result<Vec<StockMovement>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgStockMovementRepository {
    pool: sqlx::PgPool,
}

impl PgStockMovementRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl StockMovementRepository for PgStockMovementRepository {
    async fn record(&self, input: NewStockMovement) -> Result<StockMovement, sqlx::Error> {
        sqlx::query_as!(
            StockMovement,
            "INSERT INTO stock_movement(store_id, product_id, qty_delta, reason, ref_sale_id)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, store_id, product_id, qty_delta, reason, ref_sale_id, created_at",
            input.store_id,
            input.product_id,
            input.qty_delta,
            input.reason,
            input.ref_sale_id,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn list_by_product(
        &self,
        store_id: Uuid,
        product_id: Uuid,
        limit: i64,
    ) -> Result<Vec<StockMovement>, sqlx::Error> {
        sqlx::query_as!(
            StockMovement,
            "SELECT id, store_id, product_id, qty_delta, reason, ref_sale_id, created_at
             FROM stock_movement WHERE store_id = $1 AND product_id = $2
             ORDER BY created_at DESC LIMIT $3",
            store_id,
            product_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }
}
