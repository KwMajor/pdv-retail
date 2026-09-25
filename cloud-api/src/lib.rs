//! Biblioteca da Cloud API (PDV SaaS).
//!
//! Os módulos vivem aqui (e não só no binário) para que os testes de
//! integração em `tests/` possam exercitar modelos e repositórios.

// Os futures dos repositórios capturam apenas `&Self` (+ tipos `Send` como
// `Uuid`/`Decimal`); exigir bound `Send` explícito via RPITIT poluiria as
// 11 traits sem ganho. Supressão consciente e documentada.
#![allow(async_fn_in_trait)]

pub mod auth;
pub mod config;
pub mod controllers;
pub mod docs;
pub mod errors;
pub mod middleware;
pub mod models;
pub mod repositories;
pub mod services;

use axum::{Router, routing::{get, post}};
use sqlx::PgPool;
use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::auth::JwtKeys;
use crate::docs::routes;

/// Estado compartilhado (pool opcional: `/health` responde sem banco).
#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub jwt: JwtKeys,
    /// Docs OpenAPI expostas? Só dev/teste — NUNCA produção (Task 2.5).
    pub expose_docs: bool,
}

/// Router único do binário e dos testes (sem subir socket real).
/// Rotas `/api/*` exigem `TenantContext` (JWT Bearer); sem credencial válida
/// o handler nem é alcançado (`401`). Exceção: `/auth/login`, que usa a dica
/// não autenticada `X-Store-ID` só para localizar a loja.
pub fn app_router(state: AppState) -> Router {
    let app = Router::new()
        .route(routes::HEALTH, get(controllers::health::health))
        .route(routes::READY, get(controllers::health::ready))
        .route(routes::PING, get(controllers::ping::ping))
        .route(routes::USERS, post(controllers::users::create_user_handler))
        .route(routes::ME, get(controllers::users::me_handler))
        .route(routes::LOGIN, post(controllers::auth::login_handler));
    // O próprio SwaggerUi serve o JSON em OPENAPI_JSON + a UI em DOCS_UI.
    let app = if state.expose_docs {
        app.merge(
            SwaggerUi::new(routes::DOCS_UI).url(routes::OPENAPI_JSON, crate::docs::ApiDoc::openapi()),
        )
    } else {
        app
    };
    app.with_state(state)
}
