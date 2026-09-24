use std::net::SocketAddr;

use axum::{Json, Router, http::StatusCode, routing::get};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pdv_cloud_api::{AppState, app_router, config};

use config::Config;

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Não falha sem banco: /health responde sempre; /ready reflete o pool.
    // (dotenvy carrega .env local; em prod vêm do ambiente.)
    let _ = dotenvy::dotenv();
    let cfg = Config::from_env();

    let pool = match PgPoolOptions::new()
        .max_connections(5)
        .connect(&cfg.database_url)
        .await
    {
        Ok(p) => {
            tracing::info!("postgres conectado");
            Some(p)
        }
        Err(e) => {
            // Sem PII/detalhe sensível no log público: só a causa resumida.
            tracing::warn!("postgres indisponível no boot (verifique DATABASE_URL/compose): {e}");
            None
        }
    };

    let state = AppState { pool };
    let app = app_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("ouvindo em {addr}");
    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("serve");
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
    tracing::info!("shutdown");
}

/// Rota mínima usada pelos testes sem subir socket real.
pub fn router_for_tests() -> Router {
    Router::new().route(
        "/health",
        get(|| async { (StatusCode::OK, Json(json!({"status":"ok"}))) }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use pdv_cloud_api::controllers;
    use tower::ServiceExt;

    #[tokio::test]
    async fn health_ok() {
        let app = router_for_tests();
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn ready_without_pool_is_503_not_configured() {
        let state = AppState { pool: None };
        let (status, body) = controllers::health::ready(axum::extract::State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "degraded");
        assert_eq!(body["db"], "not_configured");
    }

    #[tokio::test]
    async fn ready_with_unreachable_db_is_503() {
        // Pool lazy: cria sem conectar; o SELECT 1 falha rápido (timeout 2s).
        let pool = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(std::time::Duration::from_secs(2))
            .connect_lazy("postgres://pdv:pdv@127.0.0.1:1/pdv_test")
            .unwrap();
        let state = AppState { pool: Some(pool) };
        let (status, body) = controllers::health::ready(axum::extract::State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "degraded");
    }
}
