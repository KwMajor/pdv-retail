//! `POST /api/v1/auth/login` — valida credenciais e emite JWT (US02 Task 2.2).
//!
//! O tenant vem do `StoreHint` (header `X-Store-ID` NÃO autenticado, usado só
//! para localizar a loja — o "crachá" propriamente dito é o JWT devolvido).
//! Falha → `401` genérico, sem revelar se o email existe.

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;
use crate::middleware::StoreHint;
use crate::repositories::PgUserRepository;
use crate::services::auth_service::{LoginError, LoginInput, login};

use super::users::UserResponse;

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub token_type: String,
    pub expires_in: i64,
    pub user: UserResponse,
}

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
        LoginInput { email: body.email, password: body.password },
    )
    .await
    .map_err(|e| match e {
        LoginError::Invalid => AppError::Unauthorized,
        LoginError::Db(e) => AppError::Db(e),
    })?;
    Ok(Json(LoginResponse {
        token: out.token,
        token_type: "Bearer".to_string(),
        expires_in: crate::auth::TOKEN_TTL_SECS,
        user: UserResponse::from(out.user),
    }))
}

fn pool_of(state: &crate::AppState) -> Result<sqlx::PgPool, AppError> {
    state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))
}
