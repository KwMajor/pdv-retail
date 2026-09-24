//! US01 Task 1.3 — Extractor `TenantContext` + `GET /api/v1/ping`.
//!
//! - Happy Path: header `X-Store-ID` válido → 200 com o UUID confirmado.
//! - Security Case: sem header → 400 antes do handler (barrado pelo Axum).
//! - Edge Case: `X-Store-ID: loja-do-joao` → 400 amigável, sem `panic!`.
//!
//! Sem banco: o ping não toca no Postgres (só prova o isolamento lógico).

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{AppState, app_router};
use tower::ServiceExt;
use uuid::Uuid;

fn app() -> axum::Router {
    app_router(AppState { pool: None })
}

async fn corpo_json(res: axum::response::Response) -> serde_json::Value {
    let bytes = to_bytes(res.into_body(), 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn ping_com_tenant_valido_retorna_200_e_confirma_uuid() {
    let id = Uuid::new_v4();
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/ping")
                .header("X-Store-ID", id.to_string())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let json = corpo_json(res).await;
    assert_eq!(json["status"], "pong");
    assert_eq!(json["store_id"], id.to_string());
}

#[tokio::test]
async fn ping_sem_header_barrado_com_400_antes_do_handler() {
    let res = app()
        .oneshot(
            Request::builder()
                .uri("/api/v1/ping")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let json = corpo_json(res).await;
    assert_eq!(json["code"], "BAD_REQUEST");
    // Handler do ping jamais responde sem tenant: sem "pong" na resposta.
    assert!(json.get("status").is_none());
}

#[tokio::test]
async fn ping_com_tenant_invalido_retorna_400_amigavel() {
    for invalido in ["loja-do-joao", "", "123"] {
        let res = app()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/ping")
                    .header("X-Store-ID", invalido)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{invalido}");
        let json = corpo_json(res).await;
        assert_eq!(json["code"], "BAD_REQUEST");
        assert!(json["message"].as_str().unwrap().contains("UUID"));
    }
}
