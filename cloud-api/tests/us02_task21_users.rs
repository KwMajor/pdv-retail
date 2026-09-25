//! US02 Task 2.1 (atualizada na Task 2.2) — Cadastro via JWT + RBAC.
//!
//! - Happy Path: `POST /api/v1/users` com Bearer de gerente → 201, sem hashes.
//! - Security (credencial): banco guarda Argon2id com salt, nunca plaintext.
//! - Security (tenant): payload forçando `store_id` de outra loja é ignorado
//!   (o DTO sequer tem o campo; vale o tenant do token).
//! - RBAC (A01): Bearer de caixa → 403; sem credencial → 401.
//! - DoD: `role` fora da lista rejeitado na rota; email duplicado → 409
//!   (reusável entre lojas); validações → 400.

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordVerifier},
};
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{
    AppState, app_router,
    auth::JwtKeys,
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

// --- infra ------------------------------------------------------------------

static SEQ: AtomicU64 = AtomicU64::new(200_001);

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

fn app_with_db(p: sqlx::PgPool) -> axum::Router {
    app_router(AppState {
        pool: Some(p),
        jwt: JwtKeys::from_secret(TEST_SECRET).unwrap(),
        expose_docs: true,
    })
}

/// Bootstrap fora do HTTP (via service): cria gerente e devolve Bearer via login.
async fn manager_token(p: &sqlx::PgPool, store: &str) -> String {
    let store_id: Uuid = store.parse().unwrap();
    let email = uniq("gerente@loja");
    create_user(
        &PgUserRepository::new(p.clone()),
        store_id,
        CreateUserInput {
            name: "Gerente".into(),
            email: email.clone(),
            password: "segredo-123".into(),
            role: UserRole::Manager,
        },
    )
    .await
    .unwrap();
    login_token(p, store, &email, "segredo-123").await
}

async fn login_token(p: &sqlx::PgPool, store: &str, email: &str, password: &str) -> String {
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("X-Store-ID", store)
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"email": email, "password": password}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let (status, json) = corpo(res).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    json["token"].as_str().unwrap().to_string()
}

fn post_users(token: &str, body: serde_json::Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/v1/users")
        .header("authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn corpo(res: axum::response::Response) -> (StatusCode, serde_json::Value) {
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

// --- Happy Path ---------------------------------------------------------------

#[tokio::test]
async fn gerente_cria_usuario_201_sem_hashes() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("h")).await;
    let token = manager_token(&p, &store).await;
    let email = uniq("caixa21@loja");
    let (status, json) = corpo(
        app_with_db(p)
            .oneshot(post_users(
                &token,
                json!({"name": "Caixa", "email": email, "password": "segredo-123", "role": "cashier"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["store_id"], store);
    assert_eq!(json["email"], email);
    assert_eq!(json["role"], "cashier");
    assert!(json["is_active"].as_bool().unwrap());
    // Credencial nunca volta ao cliente, nem em campo nulo.
    assert!(json.get("password_hash").is_none());
    assert!(json.get("pin_hash").is_none());
}

// --- Security Case: credencial protegida --------------------------------------

#[tokio::test]
async fn banco_guarda_argon2_irreversivel_e_nao_plaintext() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("s")).await;
    let token = manager_token(&p, &store).await;
    let email = uniq("argon21@loja");
    let plain = "segredo-123";
    let (status, json) = corpo(
        app_with_db(p.clone())
            .oneshot(post_users(
                &token,
                json!({"name": "F", "email": email, "password": plain, "role": "manager"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let id = json["id"].as_str().unwrap().to_string();

    // Inspeção direta do banco (QA acessa via DBeaver/psql).
    let hash: String = sqlx::query_scalar("SELECT password_hash FROM \"user\" WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_ne!(hash, plain, "senha em texto plano no banco!");
    assert!(hash.starts_with("$argon2id$"), "hash não é Argon2: {hash}");
    // Salt embutido: verifica o plaintext mas não o revela.
    let parsed = PasswordHash::new(&hash).unwrap();
    assert!(Argon2::default().verify_password(plain.as_bytes(), &parsed).is_ok());
    assert!(Argon2::default().verify_password(b"outra", &parsed).is_err());
}

// --- Security Case: isolamento de tenant ---------------------------------------

#[tokio::test]
async fn payload_forcando_outra_loja_e_ignorado_token_manda() {
    let p = pool().await;
    let a = mk_store(&p, &uniq("ta21")).await;
    let b = mk_store(&p, &uniq("tb21")).await;
    // Token da Loja A; payload tenta forçar a Loja B (campo inexistente no DTO).
    let token = manager_token(&p, &a).await;
    let email = uniq("spy21@loja");
    let (status, json) = corpo(
        app_with_db(p.clone())
            .oneshot(post_users(
                &token,
                json!({"name": "Spy", "email": email, "password": "segredo-123", "role": "cashier", "store_id": b}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(json["store_id"], a, "só o tenant do token vale!");
    // E o usuário não existe na Loja B.
    let na_b: Option<String> = sqlx::query_scalar(
        "SELECT id::text FROM \"user\" WHERE store_id = $1::uuid AND email = $2",
    )
    .bind(&b)
    .bind(&email)
    .fetch_optional(&p)
    .await
    .unwrap();
    assert!(na_b.is_none());
}

// --- RBAC: caixa não cadastra ----------------------------------------------------

#[tokio::test]
async fn caixa_recebe_403_ao_tentar_criar_usuario() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("rbac")).await;
    let gerente = manager_token(&p, &store).await;
    // Gerente cria o caixa; caixa tenta criar alguém.
    let email_caixa = uniq("caixa-rbac@loja");
    let (s, _) = corpo(
        app_with_db(p.clone())
            .oneshot(post_users(
                &gerente,
                json!({"name": "Caixa", "email": email_caixa, "password": "segredo-123", "role": "cashier"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let token_caixa = login_token(&p, &store, &email_caixa, "segredo-123").await;
    let (status, json) = corpo(
        app_with_db(p.clone())
            .oneshot(post_users(
                &token_caixa,
                json!({"name": "X", "email": uniq("x-rbac@loja"), "password": "segredo-123", "role": "cashier"}),
            ))
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert_eq!(json["code"], "FORBIDDEN");
}

#[tokio::test]
async fn sem_credencial_rejeitado_401_antes_do_handler() {
    let p = pool().await;
    let res = app_with_db(p)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users")
                .header("Content-Type", "application/json")
                .body(Body::from(json!({"name": "F", "email": "x@y", "password": "segredo-123", "role": "cashier"}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// --- DoD: role tipado, unicidade por loja, validações ----------------------------

#[tokio::test]
async fn role_fora_da_lista_rejeitado_na_rota() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("r21")).await;
    let token = manager_token(&p, &store).await;
    for role in ["dono", "ADMIN", "CASHIER", "", "gerente"] {
        // A rejeição do extractor Json é texto puro (não JSON): só o status importa.
        let res = app_with_db(p.clone())
            .oneshot(post_users(
                &token,
                json!({"name": "F", "email": uniq("r21@loja"), "password": "segredo-123", "role": role}),
            ))
            .await
            .unwrap();
        let status = res.status();
        assert!(
            status == StatusCode::UNPROCESSABLE_ENTITY || status == StatusCode::BAD_REQUEST,
            "role={role} retornou {status}"
        );
    }
    for role in ["admin", "manager", "cashier"] {
        let (status, json) = corpo(
            app_with_db(p.clone())
                .oneshot(post_users(
                    &token,
                    json!({"name": "F", "email": uniq("ok21@loja"), "password": "segredo-123", "role": role}),
                ))
                .await
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "role={role}");
        assert_eq!(json["role"], role);
    }
}

#[tokio::test]
async fn email_duplicado_na_loja_409_mas_reuso_entre_lojas() {
    let p = pool().await;
    let a = mk_store(&p, &uniq("ea21")).await;
    let b = mk_store(&p, &uniq("eb21")).await;
    let ta = manager_token(&p, &a).await;
    let tb = manager_token(&p, &b).await;
    let email = uniq("dup21@loja");
    let payload = || json!({"name": "F", "email": email, "password": "segredo-123", "role": "cashier"});
    let (s1, _) = corpo(app_with_db(p.clone()).oneshot(post_users(&ta, payload())).await.unwrap()).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, j2) = corpo(app_with_db(p.clone()).oneshot(post_users(&ta, payload())).await.unwrap()).await;
    assert_eq!(s2, StatusCode::CONFLICT);
    assert_eq!(j2["code"], "CONFLICT");
    let (s3, _) = corpo(app_with_db(p.clone()).oneshot(post_users(&tb, payload())).await.unwrap()).await;
    assert_eq!(s3, StatusCode::CREATED, "mesmo email em outra loja deve passar");
}

#[tokio::test]
async fn validacoes_basicas_retornam_400() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("v21")).await;
    let token = manager_token(&p, &store).await;
    for (caso, payload) in [
        ("senha curta", json!({"name": "F", "email": uniq("v21@loja"), "password": "123", "role": "cashier"})),
        ("email sem @", json!({"name": "F", "email": "sem-arroba", "password": "segredo-123", "role": "cashier"})),
        ("nome vazio", json!({"name": "  ", "email": uniq("v21@loja"), "password": "segredo-123", "role": "cashier"})),
    ] {
        let (status, json) = corpo(
            app_with_db(p.clone()).oneshot(post_users(&token, payload)).await.unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{caso}");
        assert_eq!(json["code"], "BAD_REQUEST");
    }
}
