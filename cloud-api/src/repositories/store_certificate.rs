//! Repositório de `store_certificate` (1:1 com o tenant).
//!
//! Segredos chegam aqui já cifrados pela aplicação (`certificate_cipher`).
//! Nenhuma leitura devolve a senha em claro: quem precisa decifrar usa a
//! chave mestra do processo, nunca o banco.

use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::models::StoreCertificate;

#[derive(Debug, Clone)]
pub struct UpsertStoreCertificate {
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
}

pub trait StoreCertificateRepository {
    async fn find_by_store(&self, store_id: Uuid) -> Result<Option<StoreCertificate>, sqlx::Error>;
    /// Substitui o certificado da loja (upload é upsert: reenviar troca o par).
    async fn upsert(&self, input: UpsertStoreCertificate) -> Result<StoreCertificate, sqlx::Error>;
    /// Upsert dentro da transação do upload (junto do `audit_log`).
    async fn upsert_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        input: UpsertStoreCertificate,
    ) -> Result<StoreCertificate, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgStoreCertificateRepository {
    pool: sqlx::PgPool,
}

impl PgStoreCertificateRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl StoreCertificateRepository for PgStoreCertificateRepository {
    async fn find_by_store(&self, store_id: Uuid) -> Result<Option<StoreCertificate>, sqlx::Error> {
        sqlx::query_as!(
            StoreCertificate,
            "SELECT store_id, cnpj, pfx_nonce, pfx_ciphertext, password_nonce,
             password_ciphertext, subject_cn, serial_hex, not_before, not_after,
             uploaded_by, created_at, updated_at
             FROM store_certificate WHERE store_id = $1",
            store_id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn upsert(&self, input: UpsertStoreCertificate) -> Result<StoreCertificate, sqlx::Error> {
        sqlx::query_as!(
            StoreCertificate,
            "INSERT INTO store_certificate(
                store_id, cnpj, pfx_nonce, pfx_ciphertext, password_nonce,
                password_ciphertext, subject_cn, serial_hex, not_before, not_after,
                uploaded_by
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (store_id) DO UPDATE SET
                cnpj = EXCLUDED.cnpj,
                pfx_nonce = EXCLUDED.pfx_nonce,
                pfx_ciphertext = EXCLUDED.pfx_ciphertext,
                password_nonce = EXCLUDED.password_nonce,
                password_ciphertext = EXCLUDED.password_ciphertext,
                subject_cn = EXCLUDED.subject_cn,
                serial_hex = EXCLUDED.serial_hex,
                not_before = EXCLUDED.not_before,
                not_after = EXCLUDED.not_after,
                uploaded_by = EXCLUDED.uploaded_by
             RETURNING store_id, cnpj, pfx_nonce, pfx_ciphertext, password_nonce,
             password_ciphertext, subject_cn, serial_hex, not_before, not_after,
             uploaded_by, created_at, updated_at",
            input.store_id,
            input.cnpj,
            input.pfx_nonce,
            input.pfx_ciphertext,
            input.password_nonce,
            input.password_ciphertext,
            input.subject_cn,
            input.serial_hex,
            input.not_before,
            input.not_after,
            input.uploaded_by,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn upsert_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        input: UpsertStoreCertificate,
    ) -> Result<StoreCertificate, sqlx::Error> {
        sqlx::query_as!(
            StoreCertificate,
            "INSERT INTO store_certificate(
                store_id, cnpj, pfx_nonce, pfx_ciphertext, password_nonce,
                password_ciphertext, subject_cn, serial_hex, not_before, not_after,
                uploaded_by
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             ON CONFLICT (store_id) DO UPDATE SET
                cnpj = EXCLUDED.cnpj,
                pfx_nonce = EXCLUDED.pfx_nonce,
                pfx_ciphertext = EXCLUDED.pfx_ciphertext,
                password_nonce = EXCLUDED.password_nonce,
                password_ciphertext = EXCLUDED.password_ciphertext,
                subject_cn = EXCLUDED.subject_cn,
                serial_hex = EXCLUDED.serial_hex,
                not_before = EXCLUDED.not_before,
                not_after = EXCLUDED.not_after,
                uploaded_by = EXCLUDED.uploaded_by
             RETURNING store_id, cnpj, pfx_nonce, pfx_ciphertext, password_nonce,
             password_ciphertext, subject_cn, serial_hex, not_before, not_after,
             uploaded_by, created_at, updated_at",
            input.store_id,
            input.cnpj,
            input.pfx_nonce,
            input.pfx_ciphertext,
            input.password_nonce,
            input.password_ciphertext,
            input.subject_cn,
            input.serial_hex,
            input.not_before,
            input.not_after,
            input.uploaded_by,
        )
        .fetch_one(&mut **tx)
        .await
    }
}
