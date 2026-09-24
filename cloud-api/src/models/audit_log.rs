//! Trilha de auditoria (`audit_log`, US03/US21). Append-only + `JSONB`.
//! Nunca logar PII em `stdout`/erros públicos (LGPD).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub store_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub old_data: Option<serde_json::Value>,
    pub new_data: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}
