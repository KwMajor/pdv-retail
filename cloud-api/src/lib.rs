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
pub mod errors;
pub mod middleware;
pub mod models;
pub mod repositories;
pub mod services;

use axum::{Router, routing::{get, post}};
use sqlx::PgPool;

use crate::auth::JwtKeys;

/// Estado compartilhado (pool opcional: `/health` responde sem banco).
#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
    pub jwt: JwtKeys,
}

/// Router único do binário e dos testes (sem subir socket real).
/// Rotas `/api/*` exigem `TenantContext` (JWT Bearer); sem credencial válida
/// o handler nem é alcançado (`401`). Exceção: `/auth/login`, que usa a dica
/// não autenticada `X-Store-ID` só para localizar a loja.
pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(controllers::health::health))
        .route("/ready", get(controllers::health::ready))
        .route("/api/v1/ping", get(controllers::ping::ping))
        .route("/api/v1/users", post(controllers::users::create_user_handler))
        .route("/api/v1/auth/login", post(controllers::auth::login_handler))
        .with_state(state)
}
