//! `POST /api/v1/users` — cadastro de funcionários (US02 Task 2.1).
//!
//! - Tenant SEMPRE do `TenantContext` (na US02, do JWT): o DTO sequer possui
//!   campo `store_id`, então é impossível forçar outra loja pelo payload.
//! - Papel tipado (`UserRole`): valor fora da lista é rejeitado na rota (422).
//! - Resposta `201` sem nenhum hash (credencial nunca volta ao cliente).

use axum::{Json, http::StatusCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::models::{User, UserRole};
use crate::repositories::PgUserRepository;
use crate::services::{CreateUserError, CreateUserInput, create_user};

/// Payload de criação. Deliberadamente SEM `store_id` (DoD: implícito do criador).
#[derive(Debug, Deserialize)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub role: UserRole,
}

/// Resposta pública: espelho do `User` sem `password_hash`/`pin_hash`.
#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: Uuid,
    pub store_id: Uuid,
    pub name: String,
    pub email: String,
    pub role: String,
    pub is_active: bool,
}

impl From<User> for UserResponse {
    fn from(user: User) -> Self {
        Self {
            id: user.id,
            store_id: user.store_id,
            name: user.name,
            email: user.email,
            role: user.role,
            is_active: user.is_active,
        }
    }
}

fn service_error(e: CreateUserError) -> AppError {
    match e {
        CreateUserError::Invalid(m) => AppError::BadRequest(m),
        CreateUserError::EmailTaken => {
            AppError::Conflict("email já cadastrado nesta loja".to_string())
        }
        // Detalhe do Argon2 fica só no log; cliente recebe mensagem genérica.
        CreateUserError::Hash(detail) => {
            tracing::error!(detail = %detail, "falha do Argon2 ao criar usuário");
            AppError::Internal("falha ao proteger credencial".to_string())
        }
        CreateUserError::Db(e) => AppError::Db(e),
    }
}

pub async fn create_user_handler(
    ctx: TenantContext,
    axum::extract::State(pool): axum::extract::State<crate::AppState>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    let pool = pool.pool.ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let repo = PgUserRepository::new(pool);
    let user = create_user(
        &repo,
        ctx.store_id,
        CreateUserInput {
            name: body.name,
            email: body.email,
            password: body.password,
            role: body.role,
        },
    )
    .await
    .map_err(service_error)?;
    Ok((StatusCode::CREATED, Json(UserResponse::from(user))))
}
