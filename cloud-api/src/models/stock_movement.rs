//! Ledger de estoque (`stock_movement`, US04). Append-only: o banco bloqueia
//! UPDATE/DELETE via trigger — aqui só existem `record` e leituras.

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub store_id: Uuid,
    pub product_id: Uuid,
    pub qty_delta: Decimal,
    pub reason: String,
    pub ref_sale_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
