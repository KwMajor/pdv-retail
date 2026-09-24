//! Funcionário (`"user"`). Soft delete via `is_active` — nunca DELETE físico.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct User {
    pub id: Uuid,
    pub store_id: Uuid,
    pub name: String,
    pub email: String,
    /// Hash Argon2 (OWASP A02/A07). Nunca trafega em resposta pública.
    pub password_hash: String,
    /// `admin` | `manager` | `cashier` (CHECK no banco).
    pub role: String,
    /// PIN do gerente (hash Argon2) para overrides (US21).
    pub pin_hash: Option<String>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
