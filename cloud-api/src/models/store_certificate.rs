//! US09 Task 9.1: certificado digital A1 (e-CNPJ) da loja.
//!
//! A tabela guarda o `.pfx` e a senha JÁ CIFRADOS pela aplicação
//! (AES-256-GCM). Metadados X.509 públicos ficam em claro para a UI.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct StoreCertificate {
    pub store_id: Uuid,
    pub cnpj: String,
    pub pfx_nonce: Vec<u8>,
    pub pfx_ciphertext: Vec<u8>,
    pub password_nonce: Vec<u8>,
    pub password_ciphertext: Vec<u8>,
    pub subject_cn: String,
    pub serial_hex: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub uploaded_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
