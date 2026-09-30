//! CORS do web dev (fix): preflight libera origem dev, barra desconhecida.
//!
//! Sem esta camada o `fetch` do navegador morre antes do login e a UI mostra
//! erro genérico. Origens fora da allowlist não recebem header CORS.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{AppState, app_router, jwt::JwtKeys};
use tower::ServiceExt;

/// Segredo só dos testes (nunca em prod).
fn keys() -> JwtKeys {
    JwtKeys::from_secret("test-only-secret-com-mais-de-32-chars").unwrap()
}

fn app() -> axum::Router {
    app_router(AppState {
        pool: None,
        jwt: keys(),
        expose_docs: false,
    })
}

async fn preflight(origin: &str) -> axum::response::Response {
    app()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v1/auth/login")
                .header("origin", origin)
                .header("access-control-request-method", "POST")
                .header("access-control-request-headers", "content-type,x-store-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn origem_dev_liberada_no_preflight() {
    let res = preflight("http://localhost:1420").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(
        res.headers()["access-control-allow-origin"],
        "http://localhost:1420"
    );
}

#[tokio::test]
async fn origem_desconhecida_sem_header_cors() {
    let res = preflight("https://evil.example").await;
    assert!(
        res.headers().get("access-control-allow-origin").is_none(),
        "origem fora da allowlist não pode ganhar CORS"
    );
}
