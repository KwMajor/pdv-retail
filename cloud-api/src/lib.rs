//! Biblioteca da Cloud API (PDV SaaS).
//!
//! Os módulos vivem aqui (e não só no binário) para que os testes de
//! integração em `tests/` possam exercitar modelos e repositórios.

// Os futures dos repositórios capturam apenas `&Self` (+ tipos `Send` como
// `Uuid`/`Decimal`); exigir bound `Send` explícito via RPITIT poluiria as
// 11 traits sem ganho. Supressão consciente e documentada.
#![allow(async_fn_in_trait)]

pub mod config;
pub mod controllers;
pub mod errors;
pub mod middleware;
pub mod models;
pub mod repositories;

use axum::{Router, routing::get};
use sqlx::PgPool;

/// Estado compartilhado (pool opcional: `/health` responde sem banco).
#[derive(Clone)]
pub struct AppState {
    pub pool: Option<PgPool>,
}

/// Router único do binário e dos testes (sem subir socket real).
/// `GET /api/v1/ping` exige `TenantContext` (header `X-Store-ID` na US01,
/// JWT na US02); sem tenant válido o handler nem é alcançado (400).
pub fn app_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(controllers::health::health))
        .route("/ready", get(controllers::health::ready))
        .route("/api/v1/ping", get(controllers::ping::ping))
        .with_state(state)
}
