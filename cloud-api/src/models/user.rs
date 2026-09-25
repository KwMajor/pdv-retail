//! Funcionário (`"user"`). Soft delete via `is_active` — nunca DELETE físico.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Papel do funcionário (US02 Task 2.1). Lista fechada e tipada: qualquer
/// outro valor é rejeitado na desserialização do payload (camada de rota).
/// Serializa em minúsculo para casar com o `CHECK` do banco (`001_initial_schema.sql`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Manager,
    Cashier,
}

impl UserRole {
    pub fn as_str(self) -> &'static str {
        match self {
            UserRole::Admin => "admin",
            UserRole::Manager => "manager",
            UserRole::Cashier => "cashier",
        }
    }
}

impl std::fmt::Display for UserRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

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
