//! US02 Task 2.2 — Login JWT: emissão, claims e cenários de segurança.
//!
//! - Happy: credencial correta → 200 com `token`, `expires_in` 12h e claims
//!   (`sub`, `store_id`, `role`, `exp`) conferindo na decodificação.
//! - 401 genérico e IDÊNTICO para email inexistente, senha errada e inativo
//!   (anti-enumeração: a resposta não revela o que falhou).
//! - Edge (expiração) e Security (tamper): token adulterado/expirado → 401.
//! - Login sem dica de loja → 400 (a dica `X-Store-ID` só localiza o tenant).

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{
    AppState, app_router,
    auth::{JwtKeys, TOKEN_TTL_SECS},
    models::UserRole,
    repositories::PgUserRepository,
    services::{CreateUserInput, create_user},
};
use serde_json::json;
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use tower::ServiceExt;
use uuid::Uuid;

static SEQ: AtomicU64 = AtomicU64::new(300_001);

/// Segredo só dos testes (nunca em prod).
const TEST_SECRET: &str = "test-only-secret-com-mais-de-32-chars";

fn uniq(prefix: &str) -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    static START_MS: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let start = *START_MS.get_or_init(|| {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis()
    });
    format!(
        "{prefix}-p{}-t{}-{}",
        std::process::id(),
        start,
        SEQ.fetch_add(1, Ordering::SeqCst)
    )
}

fn test_db_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pdv:pdv@localhost:5432/pdv_test".to_string())
}

async fn pool() -> sqlx::PgPool {
    static DONE: OnceCell<()> = OnceCell::const_new();
    DONE.get_or_init(|| async {
        let url = test_db_url();
        let (base, db) = url.rsplit_once('/').expect("TEST_DATABASE_URL sem /banco");
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&format!("{base}/postgres"))
            .await
            .expect("postgres de teste inacessível — suba com: docker compose up -d");
        let _ = sqlx::query(&format!("CREATE DATABASE \"{db}\""))
            .execute(&admin)
            .await;
        let setup = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(10))
            .connect_lazy(&url)
            .expect("TEST_DATABASE_URL inválida");
        sqlx::migrate!("./migrations")
            .run(&setup)
            .await
            .expect("falha ao aplicar migrations");
    })
    .await;
    PgPoolOptions::new()
        .max_connections(4)
        .acquire_timeout(Duration::from_secs(10))
        .connect_lazy(&test_db_url())
        .expect("TEST_DATABASE_URL inválida")
}

async fn mk_store(p: &sqlx::PgPool, tag: &str) -> String {
    sqlx::query_scalar("INSERT INTO store_settings(name, cnpj) VALUES ($1, $2) RETURNING id::text")
        .bind(format!("Loja {tag}"))
        .bind(format!("cnpj-{tag}"))
        .fetch_one(p)
        .await
        .unwrap()
}

/// Usuário criado direto pelo service (bootstrap); login é sempre via HTTP.
async fn mk_user(p: &sqlx::PgPool, store: &str, role: UserRole, password: &str) -> String {
    let email = uniq("login22@loja");
    create_user(
        &PgUserRepository::new(p.clone()),
        store.parse::<Uuid>().unwrap(),
        CreateUserInput {
            name: "F".into(),
            email: email.clone(),
            password: password.into(),
            role,
        },
    )
    .await
    .unwrap();
    email
}

fn app_with_db(p: sqlx::PgPool) -> axum::Router {
    app_router(AppState {
        pool: Some(p),
        jwt: JwtKeys::from_secret(TEST_SECRET).unwrap(),
        expose_docs: true,
    })
}

fn post_login(store: Option<&str>, email: &str, password: &str) -> Request<Body> {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("Content-Type", "application/json");
    if let Some(s) = store {
        builder = builder.header("X-Store-ID", s);
    }
    builder
        .body(Body::from(json!({"email": email, "password": password}).to_string()))
        .unwrap()
}

async fn corpo(res: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

async fn ping(token: &str, p: &sqlx::PgPool) -> StatusCode {
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/ping")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

// --- Happy Path -----------------------------------------------------------------

#[tokio::test]
async fn login_emite_jwt_com_claims_obrigatorios_e_ttl_12h() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("lh")).await;
    let email = mk_user(&p, &store, UserRole::Cashier, "segredo-123").await;
    let (status, json) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, "segredo-123")).await.unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["token_type"], "Bearer");
    assert_eq!(json["expires_in"], TOKEN_TTL_SECS);
    assert_eq!(json["user"]["email"], email);
    assert!(json["user"].get("password_hash").is_none());

    // Claims conferem na decodificação com o segredo.
    let token = json["token"].as_str().unwrap();
    let claims = JwtKeys::from_secret(TEST_SECRET).unwrap().validate(token).unwrap();
    assert_eq!(claims.store_id.to_string(), store);
    assert_eq!(claims.role, "cashier");
    assert_eq!(claims.exp - claims.iat, TOKEN_TTL_SECS, "teto do DoD: 12h");
    assert_eq!(json["user"]["id"], claims.sub.to_string());
}

// --- 401 genérico (anti-enumeração) ------------------------------------------------

#[tokio::test]
async fn credencial_errada_inativo_e_inexistente_retornam_mesmo_401() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("le")).await;
    let email = mk_user(&p, &store, UserRole::Manager, "segredo-123").await;
    // Desativa um deles para o caso "inativo".
    let off = mk_user(&p, &store, UserRole::Cashier, "segredo-123").await;
    sqlx::query("UPDATE \"user\" SET is_active = FALSE WHERE store_id = $1::uuid AND email = $2")
        .bind(&store)
        .bind(&off)
        .execute(&p)
        .await
        .unwrap();

    let mut corpos = Vec::new();
    for (email, pass) in [
        (uniq("fantasma22@loja"), "segredo-123"), // inexistente
        (email.clone(), "senha-errada"),          // senha errada
        (off.clone(), "segredo-123"),             // inativo
    ] {
        let (status, json) = corpo(
            app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, pass)).await.unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["code"], "UNAUTHORIZED");
        corpos.push(json.to_string());
    }
    // Respostas idênticas: nada revela qual campo falhou.
    assert_eq!(corpos[0], corpos[1]);
    assert_eq!(corpos[0], corpos[2]);
}

#[tokio::test]
async fn senha_gigante_cai_no_401_generico_sem_argon2() {
    // Acima do teto do cadastro: nunca é credencial válida; mesmo corpo 401.
    let p = pool().await;
    let store = mk_store(&p, &uniq("lg")).await;
    let email = mk_user(&p, &store, UserRole::Cashier, "segredo-123").await;
    let gigante = "y".repeat(5000);
    let (s1, j1) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, &gigante)).await.unwrap(),
    )
    .await;
    let (s2, j2) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, "errada")).await.unwrap(),
    )
    .await;
    assert_eq!(s1, StatusCode::UNAUTHORIZED);
    assert_eq!(j1.to_string(), j2.to_string());
}

#[tokio::test]
async fn login_sem_dica_de_loja_retorna_400() {
    let p = pool().await;
    let (status, json) = corpo(
        app_with_db(p).oneshot(post_login(None, "x@y", "segredo-123")).await.unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(json["code"], "BAD_REQUEST");
}

// --- Edge (expiração) + Security (tamper) ponta a ponta ------------------------------

#[tokio::test]
async fn token_expirado_barrado_com_401() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("lx")).await;
    let email = mk_user(&p, &store, UserRole::Admin, "segredo-123").await;
    let (status, json) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, "segredo-123")).await.unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let token = json["token"].as_str().unwrap();
    assert_eq!(ping(token, &p).await, StatusCode::OK);

    // Mesmo formato, mas expirado bem além do leeway: a API barra (DoD Edge sem esperar 12h).
    let keys = JwtKeys::from_secret(TEST_SECRET).unwrap();
    let claims = keys.validate(token).unwrap();
    let past = chrono::Utc::now().timestamp() - 3600;
    let (old, _) = keys.issue_at(claims.sub, claims.store_id, UserRole::Admin, past).unwrap();
    assert_eq!(ping(&old, &p).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn token_adulterado_cashier_para_manager_barrado_com_401() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("lt")).await;
    let caixa = mk_user(&p, &store, UserRole::Cashier, "segredo-123").await;
    let gerente = mk_user(&p, &store, UserRole::Manager, "segredo-123").await;
    let (_, j_caixa) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &caixa, "segredo-123")).await.unwrap(),
    )
    .await;
    let (_, j_ger) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &gerente, "segredo-123")).await.unwrap(),
    )
    .await;
    let t_caixa = j_caixa["token"].as_str().unwrap();
    let t_ger = j_ger["token"].as_str().unwrap();
    assert_eq!(ping(t_caixa, &p).await, StatusCode::OK);

    // Atacante cola a assinatura do token de gerente no payload do caixa:
    // a assinatura deixa de conferir → 401 (DoD Security tampering).
    let mut c: Vec<&str> = t_caixa.split('.').collect();
    let g: Vec<&str> = t_ger.split('.').collect();
    c[2] = g[2];
    assert_eq!(ping(&c.join("."), &p).await, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn me_devolve_identidade_do_token_para_o_desktop() {
    // O PDV restaura a sessão do cofre e revalida aqui (Task 2.4 `restore()`).
    let p = pool().await;
    let store = mk_store(&p, &uniq("me")).await;
    let email = mk_user(&p, &store, UserRole::Cashier, "segredo-123").await;
    let (_, login) = corpo(
        app_with_db(p.clone()).oneshot(post_login(Some(&store), &email, "segredo-123")).await.unwrap(),
    )
    .await;
    let token = login["token"].as_str().unwrap();
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .header("authorization", format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = corpo(res).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(json["email"], email);
    assert_eq!(json["store_id"], store);
    assert!(json.get("password_hash").is_none());
}
