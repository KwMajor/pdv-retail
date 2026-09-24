//! Venda (`sale`). `customer_id` nulo + `anonymous_cpf_cnpj` = "CPF na nota".

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Sale {
    pub id: Uuid,
    pub store_id: Uuid,
    pub customer_id: Option<Uuid>,
    pub anonymous_cpf_cnpj: Option<String>,
    /// `open` | `closed` | `cancelled` | `pending` (CHECK no banco).
    pub status: String,
    pub subtotal: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
    pub change_amount: Decimal,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
