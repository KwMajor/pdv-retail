use std::net::SocketAddr;

use axum::{Json, Router, http::StatusCode, routing::get};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use pdv_cloud_api::{AppState, app_router, config, jwt::JwtKeys};

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
    // Config ausente/insegura = boot recusado (fail-fast sem panic: log + exit,
    // mesmo padrão do JwtKeys abaixo). Nenhum `expect` no caminho do boot.
    let cfg = match Config::from_env() {
        Ok(cfg) => cfg,
        Err(detail) => {
            tracing::error!(detail = %detail, "configuração inválida, abortando boot");
            std::process::exit(1);
        }
    };

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

    // Segredo fraco = boot recusado (fail-fast sem panic: log + exit).
    let jwt = match JwtKeys::from_secret(&cfg.jwt_secret) {
        Ok(keys) => keys,
        Err(detail) => {
            tracing::error!(detail = %detail, "JWT_SECRET inválido, abortando boot");
            std::process::exit(1);
        }
    };

    // Docs OpenAPI SÓ fora de produção (US02 Task 2.5).
    let expose_docs = matches!(cfg.app_env.as_str(), "development" | "test");
    if !expose_docs {
        tracing::info!("docs OpenAPI desativadas (APP_ENV={})", cfg.app_env);
    }

    let state = AppState {
        pool,
        jwt,
        expose_docs,
    };
    let app = app_router(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("ouvindo em {addr}");
    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(listener) => listener,
        Err(e) => {
            tracing::error!("falha ao abrir a porta {addr} (em uso ou sem permissão?): {e}");
            std::process::exit(1);
        }
    };
    match axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
    {
        Ok(()) => {}
        Err(e) => {
            tracing::error!("servidor HTTP encerrou com erro: {e}");
            std::process::exit(1);
        }
    }
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

    fn test_state(pool: Option<sqlx::PgPool>) -> AppState {
        AppState {
            pool,
            jwt: JwtKeys::from_secret("test-only-secret-com-mais-de-32-chars").unwrap(),
            expose_docs: false,
        }
    }

    #[tokio::test]
    async fn ready_without_pool_is_503_not_configured() {
        let state = test_state(None);
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
        let state = test_state(Some(pool));
        let (status, body) = controllers::health::ready(axum::extract::State(state)).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(body["status"], "degraded");
    }
}
