//! Certificado digital A1 da loja (US09 Task 9.1) — exclusivo `admin` (DONO).
//!
//! A API nunca devolve a senha nem os bytes do PFX: o `GET` expõe só os
//! metadados X.509 públicos, e o `POST` aceita o `.pfx` em base64 para
//! validar, cifrar e persistir (upsert) com auditoria.

use axum::{Json, extract::State};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::services::certificate_service::{
    CertificateError, CertificateInfo, UploadCertificateInput, get_certificate, upload_certificate,
};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({
    "cnpj": "11222333000181",
    "password": "senha-do-certificado",
    "file_name": "certificado.pfx",
    "file_base64": "MIIK..."
}))]
pub struct UploadCertificateRequest {
    /// Só dígitos (14), com DV válido; precisa bater com o cadastro da loja.
    pub cnpj: String,
    /// 1–128 caracteres. Nunca é devolvida nem gravada em log.
    pub password: String,
    /// Nome do arquivo; a extensão precisa ser `.pfx` ou `.p12`.
    pub file_name: String,
    /// Conteúdo binário do PKCS#12 em base64 (máx. 5 MB decodificado).
    pub file_base64: String,
}

/// Metadados públicos do certificado — jamais senha/PFX.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({
    "store_id": "11111111-1111-1111-1111-111111111111",
    "cnpj": "11222333000181",
    "subject_cn": "MINHA LOJA LTDA:11222333000181",
    "serial_hex": "01A2B3C4D5E6F7",
    "not_before": "2026-01-01T00:00:00Z",
    "not_after": "2027-01-01T00:00:00Z",
    "uploaded_at": "2026-10-28T12:00:00Z"
}))]
pub struct CertificateResponse {
    #[schema(value_type = String)]
    pub store_id: Uuid,
    pub cnpj: String,
    pub subject_cn: String,
    pub serial_hex: String,
    pub not_before: DateTime<Utc>,
    pub not_after: DateTime<Utc>,
    pub uploaded_at: DateTime<Utc>,
}

impl From<CertificateInfo> for CertificateResponse {
    fn from(info: CertificateInfo) -> Self {
        Self {
            store_id: info.store_id,
            cnpj: info.cnpj,
            subject_cn: info.subject_cn,
            serial_hex: info.serial_hex,
            not_before: info.not_before,
            not_after: info.not_after,
            uploaded_at: info.uploaded_at,
        }
    }
}

fn service_error(error: CertificateError) -> AppError {
    match error {
        CertificateError::Invalid(message) => AppError::Unprocessable(message),
        CertificateError::NotFound => AppError::NotFound,
        CertificateError::Db(error) => AppError::Db(error),
        // Detalhe criptográfico fica fora do JSON público (e sem segredo no log).
        CertificateError::Crypto(_) => {
            AppError::Internal("falha ao cifrar o certificado".to_string())
        }
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/store/certificate",
    tag = "fiscal",
    security(("bearer" = [])),
    request_body(content = UploadCertificateRequest),
    responses(
        (status = 200, description = "PKCS#12 validated (password/MAC), PFX and password encrypted with AES-256-GCM, stored and audit-logged", body = CertificateResponse),
        (status = 401, description = "Invalid or missing Bearer token", body = ErrorBody),
        (status = 403, description = "Caller is not the store admin (DONO)", body = ErrorBody),
        (status = 413, description = "Request body above the 8 MiB transport limit (plain text)"),
        (status = 422, description = "Invalid extension/base64/size, wrong password, corrupt PKCS#12, invalid CNPJ or CNPJ mismatch", body = ErrorBody),
    ),
)]
pub async fn upload_certificate_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Json(body): Json<UploadCertificateRequest>,
) -> Result<Json<CertificateResponse>, AppError> {
    ctx.require_admin()?;
    let pool = state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let info = upload_certificate(
        &pool,
        ctx.store_id,
        ctx.user_id,
        state.cert_master_key,
        UploadCertificateInput {
            cnpj: body.cnpj,
            password: body.password,
            file_name: body.file_name,
            file_base64: body.file_base64,
        },
    )
    .await
    .map_err(service_error)?;
    Ok(Json(info.into()))
}

#[utoipa::path(
    get,
    path = "/api/v1/store/certificate",
    tag = "fiscal",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Certificate metadata only (the password and PFX bytes are never returned)", body = CertificateResponse),
        (status = 401, description = "Invalid or missing Bearer token", body = ErrorBody),
        (status = 403, description = "Caller is not the store admin (DONO)", body = ErrorBody),
        (status = 404, description = "Store has no certificate uploaded", body = ErrorBody),
    ),
)]
pub async fn get_certificate_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
) -> Result<Json<CertificateResponse>, AppError> {
    ctx.require_admin()?;
    let pool = state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let info = get_certificate(&pool, ctx.store_id)
        .await
        .map_err(service_error)?;
    Ok(Json(info.into()))
}
