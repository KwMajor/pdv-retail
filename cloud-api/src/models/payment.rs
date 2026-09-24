//! Pagamento fracionado (`payment`). Dinheiro: `tendered_amount >= amount`.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub store_id: Uuid,
    pub sale_id: Uuid,
    /// `cash` | `pix` | `credit` | `debit` | `crediario` (CHECK no banco).
    pub method: String,
    pub amount: Decimal,
    pub tendered_amount: Option<Decimal>,
    pub created_at: DateTime<Utc>,
}
