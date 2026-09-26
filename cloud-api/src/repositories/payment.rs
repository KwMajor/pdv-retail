//! Repositório de `payment`. Append-only: `create` + leituras.
//! A FK composta `(sale_id, store_id)` barra pagamento cross-tenant no banco.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Payment;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewPayment {
    pub store_id: Uuid,
    pub sale_id: Uuid,
    /// `cash` | `pix` | `credit` | `debit` | `crediario`.
    pub method: String,
    pub amount: Decimal,
    pub tendered_amount: Option<Decimal>,
}

pub trait PaymentRepository {
    async fn create(&self, input: NewPayment) -> Result<Payment, sqlx::Error>;
    async fn find_by_id(&self, store_id: Uuid, id: Uuid) -> Result<Option<Payment>, sqlx::Error>;
    async fn list_by_sale(
        &self,
        store_id: Uuid,
        sale_id: Uuid,
    ) -> Result<Vec<Payment>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgPaymentRepository {
    pool: sqlx::PgPool,
}

impl PgPaymentRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl PaymentRepository for PgPaymentRepository {
    async fn create(&self, input: NewPayment) -> Result<Payment, sqlx::Error> {
        sqlx::query_as!(
            Payment,
            "INSERT INTO payment(store_id, sale_id, method, amount, tendered_amount)
             VALUES ($1, $2, $3, $4, $5)
             RETURNING id, store_id, sale_id, method, amount, tendered_amount, created_at",
            input.store_id,
            input.sale_id,
            input.method,
            input.amount,
            input.tendered_amount,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(&self, store_id: Uuid, id: Uuid) -> Result<Option<Payment>, sqlx::Error> {
        sqlx::query_as!(
            Payment,
            "SELECT id, store_id, sale_id, method, amount, tendered_amount, created_at
             FROM payment WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_sale(
        &self,
        store_id: Uuid,
        sale_id: Uuid,
    ) -> Result<Vec<Payment>, sqlx::Error> {
        sqlx::query_as!(
            Payment,
            "SELECT id, store_id, sale_id, method, amount, tendered_amount, created_at
             FROM payment WHERE store_id = $1 AND sale_id = $2 ORDER BY created_at ASC",
            store_id,
            sale_id,
        )
        .fetch_all(&self.pool)
        .await
    }
}
