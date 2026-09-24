//! Cliente PF/PJ unificado (`customer`). LGPD: minimizar e anonimizar sem
//! quebrar PKs nem o histórico fiscal (retenção SEFAZ 5 anos).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct Customer {
    pub id: Uuid,
    pub store_id: Uuid,
    pub name: String,
    pub cpf_cnpj: Option<String>,
    pub corporate_name: Option<String>,
    pub state_registration: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
