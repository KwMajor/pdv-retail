//! `POST /api/v1/users` — cadastro de funcionários (US02 Task 2.1).
//!
//! - Tenant SEMPRE do `TenantContext` (na US02, do JWT): o DTO sequer possui
//!   campo `store_id`, então é impossível forçar outra loja pelo payload.
//! - Papel tipado (`UserRole`): valor fora da lista é rejeitado na rota (422).
//! - Resposta `201` sem nenhum hash (credencial nunca volta ao cliente).

use axum::{Json, http::StatusCode};
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::models::{User, UserRole};
use crate::repositories::{PgUserRepository, UserRepository};
use crate::services::{CreateUserError, CreateUserInput, create_user};

/// Payload de criação. Deliberadamente SEM `store_id` (DoD: implícito do criador).
#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"name": "Maria Caixa", "email": "maria@loja.exemplo", "password": "senha-exemplo-123", "role": "cashier"}))]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    /// Texto plano só no request — nunca volta em resposta (ver `UserResponse`).
    pub password: String,
    pub role: UserRole,
}

/// Resposta pública: espelho do `User` sem nenhum hash de credencial.
/// NUNCA derivar `ToSchema` em `User` (vazaria credencial nos exemplos).
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({"id": "11111111-1111-1111-1111-111111111111", "store_id": "22222222-2222-2222-2222-222222222222", "name": "Maria Caixa", "email": "maria@loja.exemplo", "role": "cashier", "is_active": true}))]
pub struct UserResponse {
    #[schema(value_type = String, example = "11111111-1111-1111-1111-111111111111")]
    pub id: Uuid,
    #[schema(value_type = String, example = "22222222-2222-2222-2222-222222222222")]
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

/// `GET /api/v1/me` — identidade do token (o desktop revalida a sessão
/// restaurada do cofre aqui; token expirado nem chega: 401 do extractor).
#[utoipa::path(
    get,
    path = "/api/v1/me",
    tag = "sistema",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Token identity (no hashes)", body = UserResponse),
        (status = 401, description = "Invalid or missing Bearer token", body = ErrorBody),
        (status = 404, description = "User deactivated/removed after issuance", body = ErrorBody),
    ),
)]
pub async fn me_handler(
    ctx: TenantContext,
    axum::extract::State(state): axum::extract::State<crate::AppState>,
) -> Result<axum::Json<UserResponse>, AppError> {
    let pool = state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let user = crate::repositories::PgUserRepository::new(pool)
        .find_by_id(ctx.store_id, ctx.user_id)
        .await?
        .filter(|u| u.is_active)
        .ok_or(AppError::NotFound)?;
    Ok(axum::Json(UserResponse::from(user)))
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

#[utoipa::path(
    post,
    path = "/api/v1/users",
    tag = "usuarios",
    security(("bearer" = [])),
    request_body(
        content = CreateUserRequest,
        description = "No store_id field: the token tenant applies"
    ),
    responses(
        (status = 201, description = "Employee created (no hashes)", body = UserResponse),
        (status = 400, description = "Validation (name/email/password)", body = ErrorBody),
        (status = 401, description = "Invalid or missing Bearer token", body = ErrorBody),
        (status = 403, description = "Cashiers cannot create (managers only)", body = ErrorBody),
        (status = 409, description = "Email already registered in the store", body = ErrorBody),
        (status = 422, description = "Role outside the list (Axum plain text)", body = String),
    ),
)]
pub async fn create_user_handler(
    ctx: TenantContext,
    axum::extract::State(pool): axum::extract::State<crate::AppState>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    // RBAC (OWASP A01): caixa não cadastra funcionário — só gestão.
    ctx.require_manager()?;
    let pool = pool
        .pool
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
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
