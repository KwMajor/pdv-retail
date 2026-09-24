//! Repositório de `sale`. Status mutável via `set_status`; sem `DELETE`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Sale;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSale {
    pub store_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub anonymous_cpf_cnpj: Option<String>,
    /// `open` | `closed` | `cancelled` | `pending`.
    pub status: String,
    pub subtotal: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
    pub change_amount: Decimal,
    pub created_by: Option<Uuid>,
}

pub trait SaleRepository {
    async fn create(&self, input: NewSale) -> Result<Sale, sqlx::Error>;
    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sale>, sqlx::Error>;
    async fn list_by_store(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Sale>, sqlx::Error>;
    /// Transição de status (ex: `cancelled`). Venda nunca é apagada.
    async fn set_status(
        &self,
        store_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<Sale, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgSaleRepository {
    pool: sqlx::PgPool,
}

impl PgSaleRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl SaleRepository for PgSaleRepository {
    async fn create(&self, input: NewSale) -> Result<Sale, sqlx::Error> {
        sqlx::query_as!(
            Sale,
            "INSERT INTO sale(store_id, customer_id, anonymous_cpf_cnpj, status, subtotal, discount, total, change_amount, created_by)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             RETURNING id, store_id, customer_id, anonymous_cpf_cnpj, status, subtotal, discount, total, change_amount, created_by, created_at",
            input.store_id,
            input.customer_id,
            input.anonymous_cpf_cnpj,
            input.status,
            input.subtotal,
            input.discount,
            input.total,
            input.change_amount,
            input.created_by,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Sale>, sqlx::Error> {
        sqlx::query_as!(
            Sale,
            "SELECT id, store_id, customer_id, anonymous_cpf_cnpj, status, subtotal, discount, total, change_amount, created_by, created_at
             FROM sale WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_store(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Sale>, sqlx::Error> {
        sqlx::query_as!(
            Sale,
            "SELECT id, store_id, customer_id, anonymous_cpf_cnpj, status, subtotal, discount, total, change_amount, created_by, created_at
             FROM sale WHERE store_id = $1
             ORDER BY created_at DESC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn set_status(
        &self,
        store_id: Uuid,
        id: Uuid,
        status: &str,
    ) -> Result<Sale, sqlx::Error> {
        sqlx::query_as!(
            Sale,
            "UPDATE sale SET status = $3 WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, customer_id, anonymous_cpf_cnpj, status, subtotal, discount, total, change_amount, created_by, created_at",
            store_id,
            id,
            status,
        )
        .fetch_one(&self.pool)
        .await
    }
}
