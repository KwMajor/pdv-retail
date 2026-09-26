//! US01 Task 1.3 (atualizada na US02 Task 2.2) — `TenantContext` + `GET /api/v1/ping`.
//!
//! O extractor temporário `X-Store-ID` foi SUBSTITUÍDO por JWT Bearer:
//! - Happy Path: `Authorization: Bearer <JWT>` válido → 200 com o `store_id`.
//! - Sem credencial / lixo / adulterado / expirado / papel estranho → 401
//!   antes do handler (barrado pelo Axum), sem `panic!`.
//!
//! Sem banco: o ping não toca no Postgres (só prova o isolamento lógico).

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{AppState, app_router, auth::JwtKeys, models::UserRole};
use tower::ServiceExt;
use uuid::Uuid;

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

fn bearer(token: &str) -> String {
    format!("Bearer {token}")
}

async fn ping(auth: Option<String>) -> (StatusCode, serde_json::Value) {
    let mut builder = Request::builder().uri("/api/v1/ping");
    if let Some(value) = auth {
        builder = builder.header("authorization", value);
    }
    let res = app()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn mint(role: UserRole) -> (String, Uuid, Uuid) {
    let user = Uuid::new_v4();
    let store = Uuid::new_v4();
    let (token, _) = keys().issue(user, store, role).unwrap();
    (token, user, store)
}

#[tokio::test]
async fn ping_com_jwt_valido_retorna_200_e_confirma_store() {
    let (token, _, store) = mint(UserRole::Cashier);
    let (status, json) = ping(Some(bearer(&token))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["status"], "pong");
    assert_eq!(json["store_id"], store.to_string());
}

#[tokio::test]
async fn ping_sem_credencial_barrado_com_401() {
    for auth in [
        None,
        Some("Token abc".into()),
        Some("Bearer ".into()),
        Some("lixo".into()),
    ] {
        let (status, json) = ping(auth).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["code"], "UNAUTHORIZED");
        // Handler jamais responde sem tenant: sem "pong".
        assert!(json.get("status").is_none());
    }
}

#[tokio::test]
async fn ping_com_token_adulterado_rejeita_401() {
    let (token, _, _) = mint(UserRole::Cashier);
    // 1. Byte virado no payload: assinatura deixa de conferir.
    let mut flip = token.clone();
    let dot = flip.find('.').unwrap();
    let byte = flip.as_bytes()[dot + 1];
    flip.replace_range(dot + 1..dot + 2, if byte == b'A' { "B" } else { "A" });
    // 2. Assinatura transplantada de outro token (ex: "vira manager" sem a chave).
    let (other, _, _) = mint(UserRole::Manager);
    let mut parts: Vec<&str> = token.split('.').collect();
    let sig_other: Vec<&str> = other.split('.').collect();
    parts[2] = sig_other[2];
    let swap = parts.join(".");
    for adulterado in [flip, swap] {
        let (status, json) = ping(Some(bearer(&adulterado))).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["code"], "UNAUTHORIZED");
    }
}

#[tokio::test]
async fn ping_com_papel_desconhecido_rejeita_401() {
    // Assinatura válida, mas `role` fora da lista: o extractor barra.
    use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
    use pdv_cloud_api::auth::Claims;
    let now = chrono::Utc::now().timestamp();
    let token = encode(
        &Header::new(Algorithm::HS256),
        &Claims {
            sub: Uuid::new_v4(),
            store_id: Uuid::new_v4(),
            role: "dono".to_string(),
            exp: now + 3600,
            iat: now,
        },
        &EncodingKey::from_secret(b"test-only-secret-com-mais-de-32-chars"),
    )
    .unwrap();
    let (status, json) = ping(Some(bearer(&token))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["code"], "UNAUTHORIZED");
}

#[tokio::test]
async fn ping_com_token_expirado_rejeita_401() {
    // Bem além do leeway de 30s: determinístico.
    let past = chrono::Utc::now().timestamp() - 3600;
    let (token, _) = keys()
        .issue_at(Uuid::new_v4(), Uuid::new_v4(), UserRole::Admin, past)
        .unwrap();
    let (status, _) = ping(Some(bearer(&token))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
