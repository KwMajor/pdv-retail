//! `POST /api/v1/auth/login` — valida credenciais e emite JWT (US02 Task 2.2).
//!
//! O tenant vem do `StoreHint` (header `X-Store-ID` NÃO autenticado, usado só
//! para localizar a loja — o "crachá" propriamente dito é o JWT devolvido).
//! Falha → `401` genérico, sem revelar se o email existe.

use axum::{Json, extract::State};

use crate::errors::AppError;
use crate::middleware::StoreHint;
use crate::repositories::PgUserRepository;
use crate::services::session_service::{LoginError, LoginInput, login};

use super::users::UserResponse;

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"email": "maria@loja.exemplo", "password": "senha-exemplo-123"}))]
pub struct LoginRequest {
    pub email: String,
    /// Texto plano só no request — nunca persistido nem devolvido.
    pub password: String,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({"token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.EXEMPLO NAO REAL.falsa", "token_type": "Bearer", "expires_in": 43200, "user": {"id": "11111111-1111-1111-1111-111111111111", "store_id": "22222222-2222-2222-2222-222222222222", "name": "Maria Caixa", "email": "maria@loja.exemplo", "role": "cashier", "is_active": true}}))]
pub struct LoginResponse {
    /// JWT assinado (exemplo fictício — nunca um token real).
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body(content = LoginRequest, description = "Tenant via header X-Store-ID (dica não autenticada)"),
    responses(
        (status = 200, description = "JWT emitido (TTL 12h)", body = LoginResponse),
        (status = 400, description = "Sem dica de loja (X-Store-ID)", body = ErrorBody),
        (status = 401, description = "Credencial inválida (genérico, anti-enumeração)", body = ErrorBody),
    ),
)]
pub async fn login_handler(
    hint: StoreHint,
    State(state): State<crate::AppState>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, AppError> {
    let pool = pool_of(&state)?;
    let repo = PgUserRepository::new(pool);
    let out = login(
        &repo,
        &state.jwt,
        hint.store_id,
        LoginInput {
            email: body.email,
            password: body.password,
        },
    )
    .await
    .map_err(|e| match e {
        LoginError::Invalid => AppError::Unauthorized,
        LoginError::Db(e) => AppError::Db(e),
    })?;
    Ok(Json(LoginResponse {
        token: out.token,
        token_type: "Bearer".to_string(),
        expires_in: crate::jwt::TOKEN_TTL_SECS,
        user: UserResponse::from(out.user),
    }))
}

fn pool_of(state: &crate::AppState) -> Result<sqlx::PgPool, AppError> {
    state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))
}
