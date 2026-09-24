//! Repositório do outbox `fiscal_queue` (US13). Enfileirar + transições do worker.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::FiscalQueue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewFiscalQueue {
    pub store_id: Uuid,
    pub sale_id: Uuid,
}

pub trait FiscalQueueRepository {
    async fn enqueue(&self, input: NewFiscalQueue) -> Result<FiscalQueue, sqlx::Error>;
    async fn list_pending(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<FiscalQueue>, sqlx::Error>;
    /// Transição de status do worker (`pending`→`processing`→`done`/`failed`).
    async fn set_status(
        &self,
        store_id: Uuid,
        id: Uuid,
        status: &str,
        last_error: Option<&str>,
    ) -> Result<FiscalQueue, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgFiscalQueueRepository {
    pool: sqlx::PgPool,
}

impl PgFiscalQueueRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl FiscalQueueRepository for PgFiscalQueueRepository {
    async fn enqueue(&self, input: NewFiscalQueue) -> Result<FiscalQueue, sqlx::Error> {
        sqlx::query_as!(
            FiscalQueue,
            "INSERT INTO fiscal_queue(store_id, sale_id) VALUES ($1, $2)
             RETURNING id, store_id, sale_id, status, attempts, next_retry_at, last_error, created_at, updated_at",
            input.store_id,
            input.sale_id,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn list_pending(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<FiscalQueue>, sqlx::Error> {
        sqlx::query_as!(
            FiscalQueue,
            "SELECT id, store_id, sale_id, status, attempts, next_retry_at, last_error, created_at, updated_at
             FROM fiscal_queue WHERE store_id = $1 AND status = 'pending'
             ORDER BY created_at ASC LIMIT $2",
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
        last_error: Option<&str>,
    ) -> Result<FiscalQueue, sqlx::Error> {
        sqlx::query_as!(
            FiscalQueue,
            "UPDATE fiscal_queue SET status = $3, last_error = $4 WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, sale_id, status, attempts, next_retry_at, last_error, created_at, updated_at",
            store_id,
            id,
            status,
            last_error,
        )
        .fetch_one(&self.pool)
        .await
    }
}
