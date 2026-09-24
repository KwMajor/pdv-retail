//! Tenant raiz (`store_settings`). Única tabela SEM `store_id` — ela É o tenant.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct StoreSettings {
    pub id: Uuid,
    pub name: String,
    pub cnpj: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
