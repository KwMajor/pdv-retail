//! US02 Task 2.5 — Swagger/OpenAPI: o que entra e o que sai de cada rota.
//!
//! - Anti-drift: toda rota de `routes::DOCUMENTED` (as mesmas consts que o
//!   `app_router` registra) PRECISA existir no spec, com responses.
//! - Conteúdo: `/users` documenta 201/403/409; Bearer exigido onde há JWT.
//! - Exposição: dev/teste servem `openapi.json` + UI; produção retorna 404.
//! - Higiene: spec sem hashes, segredos ou PII real nos schemas/exemplos.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{AppState, app_router, auth::JwtKeys, docs::routes};
use tower::ServiceExt;

/// Segredo só dos testes (nunca em prod).
const TEST_SECRET: &str = "test-only-secret-com-mais-de-32-chars";

fn app(expose_docs: bool) -> axum::Router {
    app_router(AppState {
        pool: None,
        jwt: JwtKeys::from_secret(TEST_SECRET).unwrap(),
        expose_docs,
    })
}

async fn get(app: axum::Router, uri: &str) -> (StatusCode, Vec<u8>, String) {
    let res = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let content_type = res
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = to_bytes(res.into_body(), 256 * 1024)
        .await
        .unwrap()
        .to_vec();
    (status, bytes, content_type)
}

async fn spec_json() -> serde_json::Value {
    let (status, bytes, content_type) = get(app(true), routes::OPENAPI_JSON).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.contains("application/json"), "{content_type}");
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn anti_drift_toda_rota_registrada_esta_no_spec() {
    let spec = spec_json().await;
    let paths = spec["paths"].as_object().unwrap();
    for route in routes::DOCUMENTED {
        let item = paths
            .get(*route)
            .unwrap_or_else(|| panic!("rota {route} sem documentação no OpenAPI"));
        // Rota documentada de verdade: tem ao menos um método com responses.
        let methods: Vec<&String> = item.as_object().unwrap().keys().collect();
        assert!(!methods.is_empty(), "rota {route} sem métodos no spec");
        for (_, operation) in item.as_object().unwrap() {
            assert!(
                operation.get("responses").is_some(),
                "rota {route} sem responses documentados"
            );
        }
    }
}

#[tokio::test]
async fn conteudo_users_ping_login_documenta_auth_e_erros() {
    let spec = spec_json().await;
    // Esquema Bearer global.
    assert_eq!(
        spec["components"]["securitySchemes"]["bearer"]["scheme"], "bearer",
        "esquema Bearer ausente"
    );
    // POST /users: 201 + erros da matriz (403 RBAC, 409 duplicado).
    let users = &spec["paths"][routes::USERS]["post"];
    for code in ["201", "400", "401", "403", "409"] {
        assert!(users["responses"].get(code).is_some(), "/users sem {code}");
    }
    assert!(
        users["security"].to_string().contains("bearer"),
        "/users sem exigência Bearer"
    );
    // Schemas públicos existem; o interno com hashes, jamais.
    let schemas = spec["components"]["schemas"].as_object().unwrap();
    for name in [
        "UserResponse",
        "CreateUserRequest",
        "LoginResponse",
        "ErrorBody",
        "PingResponse",
    ] {
        assert!(schemas.contains_key(name), "schema {name} ausente");
    }
    assert!(
        !schemas.contains_key("User"),
        "struct interna User vazou no spec!"
    );
    // Login documenta o TTL e o envelope de erro.
    assert!(
        spec["paths"][routes::LOGIN]["post"]["responses"]
            .get("200")
            .is_some()
    );
}

#[tokio::test]
async fn spec_sem_hashes_segredos_ou_pii() {
    let spec = spec_json().await;
    let raw = spec.to_string();
    for proibido in [
        "password_hash",
        "pin_hash",
        "JWT_SECRET",
        "$argon2",
        "segredo-123",
    ] {
        assert!(
            !raw.contains(proibido),
            "spec vaza dado sensível: {proibido}"
        );
    }
}

#[tokio::test]
async fn docs_expostas_em_dev_e_sumidas_em_producao() {
    // Dev/teste: JSON + UI.
    let (status, _, content_type) = get(app(true), routes::OPENAPI_JSON).await;
    assert_eq!(status, StatusCode::OK);
    assert!(content_type.contains("application/json"));
    // A UI redireciona /docs → /docs/ (303); a página final é HTML 200.
    let (status, _, _) = get(app(true), routes::DOCS_UI).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "UI sem redirect em dev");
    let (status, _, content_type) = get(app(true), &format!("{}/", routes::DOCS_UI)).await;
    assert_eq!(status, StatusCode::OK, "Swagger UI fora do ar em dev");
    assert!(content_type.contains("text/html"), "{content_type}");

    // Produção: nem JSON nem UI existem (404, sem vazar superfície).
    for uri in [routes::OPENAPI_JSON, routes::DOCS_UI] {
        let (status, _, _) = get(app(false), uri).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{uri} exposta em produção!");
    }
}
