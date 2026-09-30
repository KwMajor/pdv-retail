//! CORS para o web dev (US03 fix).
//!
//! O navegador exige preflight em cross-origin (`:1420` → `:3000`); sem esta
//! camada o `fetch` do login morre no preflight e a UI mostra erro genérico.
//! Allowlist explícita (nunca `*`): origens dev + esquemas do Tauri, ou
//! `CORS_ORIGINS` (vírgula) no ambiente. Métodos/headers: só o que a API usa.

use axum::http::{HeaderValue, Method};
use tower_http::cors::{AllowHeaders, AllowOrigin, CorsLayer};

const DEV_ORIGINS: &[&str] = &[
    "http://localhost:1420",
    "http://127.0.0.1:1420",
    "tauri://localhost",
    "http://tauri.localhost",
];

fn allowed_origins() -> Vec<HeaderValue> {
    let raw = std::env::var("CORS_ORIGINS").unwrap_or_default();
    let from_env: Vec<HeaderValue> = raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    if from_env.is_empty() {
        DEV_ORIGINS.iter().filter_map(|s| s.parse().ok()).collect()
    } else {
        from_env
    }
}

pub fn cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin: &HeaderValue, _| {
            allowed_origins().iter().any(|allowed| allowed == origin)
        }))
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers(AllowHeaders::list([
            axum::http::header::AUTHORIZATION,
            axum::http::header::CONTENT_TYPE,
            axum::http::HeaderName::from_static("x-store-id"),
        ]))
}
