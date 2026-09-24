//! Outbox fiscal (`fiscal_queue`, US13). Venda + enqueue na mesma transação.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct FiscalQueue {
    pub id: Uuid,
    pub store_id: Uuid,
    pub sale_id: Uuid,
    /// `pending` | `processing` | `done` | `failed` (CHECK no banco).
    pub status: String,
    pub attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
