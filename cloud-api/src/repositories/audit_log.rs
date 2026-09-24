//! Repositório de `audit_log` (US03/US21). Append-only + `JSONB`.
//! Nunca registrar PII em `old_data`/`new_data` além do estritamente fiscal.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::AuditLog;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewAuditLog {
    pub store_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub action: String,
    pub entity: String,
    pub entity_id: String,
    pub old_data: Option<serde_json::Value>,
    pub new_data: Option<serde_json::Value>,
}

pub trait AuditLogRepository {
    async fn record(&self, input: NewAuditLog) -> Result<AuditLog, sqlx::Error>;
    async fn list_by_store(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditLog>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgAuditLogRepository {
    pool: sqlx::PgPool,
}

impl PgAuditLogRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl AuditLogRepository for PgAuditLogRepository {
    async fn record(&self, input: NewAuditLog) -> Result<AuditLog, sqlx::Error> {
        sqlx::query_as!(
            AuditLog,
            "INSERT INTO audit_log(store_id, actor_user_id, action, entity, entity_id, old_data, new_data)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             RETURNING id, store_id, actor_user_id, action, entity, entity_id, old_data, new_data, created_at",
            input.store_id,
            input.actor_user_id,
            input.action,
            input.entity,
            input.entity_id,
            input.old_data,
            input.new_data,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn list_by_store(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<AuditLog>, sqlx::Error> {
        sqlx::query_as!(
            AuditLog,
            "SELECT id, store_id, actor_user_id, action, entity, entity_id, old_data, new_data, created_at
             FROM audit_log WHERE store_id = $1
             ORDER BY created_at DESC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }
}
