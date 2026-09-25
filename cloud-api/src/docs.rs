//! OpenAPI/Swagger (US02 Task 2.5).
//!
//! - Schemas derivam do código (`ToSchema` nos DTOs): a doc nunca diverge.
//! - `NUNCA` derivar `ToSchema` em structs com segredo (`User` tem
//!   `password_hash` — só `UserResponse` é documentado).
//! - `routes::DOCUMENTED` é a fonte única cruzada pelo teste anti-drift:
//!   o `app_router` REGISTRA por estas consts e o spec PRECISA contê-las.

use utoipa::{Modify, OpenApi};
use utoipa::openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme};

use crate::controllers::{auth, health, ping, users};
use crate::errors::ErrorBody;
use crate::models::UserRole;

/// Caminhos da API.
pub mod routes {
    pub const HEALTH: &str = "/health";
    pub const READY: &str = "/ready";
    pub const PING: &str = "/api/v1/ping";
    pub const USERS: &str = "/api/v1/users";
    pub const ME: &str = "/api/v1/me";
    pub const LOGIN: &str = "/api/v1/auth/login";
    pub const OPENAPI_JSON: &str = "/api-docs/openapi.json";
    pub const DOCS_UI: &str = "/docs";

    /// Toda rota de negócio precisa estar no spec (anti-drift).
    pub const DOCUMENTED: &[&str] = &[HEALTH, READY, PING, USERS, ME, LOGIN];
}

/// `Authorization: Bearer <JWT>` como esquema nomeado `bearer`.
struct SecurityAddon;

impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        // Mescla no components JÁ gerado (schemas): nunca substituir por
        // completo, senão os schemas somem do spec.
        let components = openapi.components.get_or_insert_with(Default::default);
        components.add_security_scheme(
            "bearer",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .build(),
            ),
        );
    }
}

#[derive(OpenApi)]
#[openapi(
    paths(
        health::health,
        health::ready,
        ping::ping,
        users::create_user_handler,
        users::me_handler,
        auth::login_handler,
    ),
    components(
        schemas(
            health::StatusResponse,
            health::DegradedResponse,
            ping::PingResponse,
            users::CreateUserRequest,
            users::UserResponse,
            auth::LoginRequest,
            auth::LoginResponse,
            UserRole,
            ErrorBody,
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "sistema", description = "Liveness, readiness e identidade"),
        (name = "isolamento", description = "Prova de tenant autenticado"),
        (name = "usuarios", description = "Gestão de funcionários (gestão)"),
        (name = "auth", description = "Emissão de JWT"),
    ),
)]
pub struct ApiDoc;
