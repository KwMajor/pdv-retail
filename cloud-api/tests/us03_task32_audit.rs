//! US03 Task 3.2 — Auditoria de preço em transação atômica.
//!
//! - Happy: PUT 10.00 → 15.00 grava `AUDIT_LOG` (`PRICE_CHANGE`, ator do JWT,
//!   old/new em JSONB) junto do update.
//! - Sem mudança de preço: 200 sem linha de auditoria.
//! - Rollback: falha no log desfaz o update (preço intacto, sem linha órfã).
//! - Inexistente: 404 sem efeito colateral.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{
    AppState, app_router,
    jwt::JwtKeys,
    models::UserRole,
    repositories::{
        AuditLogRepository, NewAuditLog, PgAuditLogRepository, PgProductRepository,
        PgUserRepository, ProductPatch, ProductRepository,
    },
    services::{CreateUserInput, create_user},
};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use tower::ServiceExt;
use uuid::Uuid;

static SEQ: AtomicU64 = AtomicU64::new(600_001);

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

/// Bootstrap via service + login HTTP. Devolve (token, user_id do gerente).
async fn manager_login(p: &sqlx::PgPool, store: &str) -> (String, String) {
    let email = uniq("aud32@loja");
    create_user(
        &PgUserRepository::new(p.clone()),
        store.parse::<Uuid>().unwrap(),
        CreateUserInput {
            name: "Gerente".into(),
            email: email.clone(),
            password: "segredo-123".into(),
            role: UserRole::Manager,
        },
    )
    .await
    .unwrap();
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("X-Store-ID", store)
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"email": email, "password": "segredo-123"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    (
        json["token"].as_str().unwrap().to_string(),
        json["user"]["id"].as_str().unwrap().to_string(),
    )
}

async fn call(
    p: &sqlx::PgPool,
    token: &str,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("authorization", format!("Bearer {token}"));
    let req = if let Some(b) = body {
        builder
            .header("Content-Type", "application/json")
            .body(Body::from(b.to_string()))
            .unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let res = app_with_db(p.clone()).oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 8192).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

fn produto(sku: &str, price: f64) -> Value {
    json!({
        "name": "Arroz T1 5kg",
        "sku": sku,
        "price": price,
        "cost": 8.00,
        "ncm": "10063021",
        "icms_rate": 18.00,
    })
}

async fn audit_rows(p: &sqlx::PgPool, store: &str) -> Vec<(String, String, String, Value, Value)> {
    sqlx::query_as(
        "SELECT action, entity, entity_id, old_data, new_data FROM audit_log WHERE store_id = $1::uuid ORDER BY created_at",
    )
    .bind(store)
    .fetch_all(p)
    .await
    .unwrap()
}

#[tokio::test]
async fn preco_alterado_grava_audit_com_ator_e_divergencia() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("h32")).await;
    let (gerente, gerente_id) = manager_login(&p, &store).await;

    let (s, j) = call(
        &p,
        &gerente,
        "POST",
        "/api/v1/products",
        Some(produto(&uniq("H32"), 10.00)),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let id = j["id"].as_str().unwrap().to_string();

    let (s, j) = call(
        &p,
        &gerente,
        "PUT",
        &format!("/api/v1/products/{id}"),
        Some(json!({"price": 15.00})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    // Dinheiro compara numericamente: escala ("15" vs "15.00") não é canônica.
    let preco: Decimal = j["price"].as_str().unwrap().parse().unwrap();
    assert_eq!(preco, Decimal::new(1500, 2));

    let rows = audit_rows(&p, &store).await;
    assert_eq!(rows.len(), 1, "uma linha de auditoria por mudança");
    let (action, entity, entity_id, old, new) = &rows[0];
    assert_eq!(action, "PRICE_CHANGE");
    assert_eq!(entity, "product");
    assert_eq!(entity_id, &id);
    let velho: Decimal = old["price"].as_str().unwrap().parse().unwrap();
    let novo: Decimal = new["price"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        (velho, novo),
        (Decimal::new(1000, 2), Decimal::new(1500, 2))
    );
    // Ator = gerente do JWT (QA confere no banco).
    let ator: Option<String> =
        sqlx::query_scalar("SELECT actor_user_id::text FROM audit_log WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(ator.as_deref(), Some(gerente_id.as_str()));
}

#[tokio::test]
async fn preco_igual_nao_gera_auditoria() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("n32")).await;
    let (gerente, _) = manager_login(&p, &store).await;

    let (s, j) = call(
        &p,
        &gerente,
        "POST",
        "/api/v1/products",
        Some(produto(&uniq("N32"), 10.00)),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let id = j["id"].as_str().unwrap().to_string();

    // PUT com o mesmo preço + outro campo: 200, sem linha de auditoria.
    let (s, _) = call(
        &p,
        &gerente,
        "PUT",
        &format!("/api/v1/products/{id}"),
        Some(json!({"price": 10.00, "name": "Arroz T1 1kg"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(audit_rows(&p, &store).await.is_empty());
}

#[tokio::test]
async fn falha_no_log_desfaz_o_update() {
    // Rollback atômico via métodos _tx: update OK + log com FK inválida
    // → transação aborta → preço intacto e sem linha órfã.
    let p = pool().await;
    let store = mk_store(&p, &uniq("r32")).await;
    let (gerente, _) = manager_login(&p, &store).await;

    let (s, j) = call(
        &p,
        &gerente,
        "POST",
        "/api/v1/products",
        Some(produto(&uniq("R32"), 10.00)),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED);
    let id: Uuid = j["id"].as_str().unwrap().parse().unwrap();
    let store_id: Uuid = store.parse().unwrap();

    let products = PgProductRepository::new(p.clone());
    let audits = PgAuditLogRepository::new(p.clone());
    let mut tx = p.begin().await.unwrap();
    products
        .update_details_tx(
            &mut tx,
            store_id,
            id,
            ProductPatch {
                price: Some("15.00".parse().unwrap()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    // Loja fantasma: FK do audit_log estoura aqui.
    let err = audits
        .record_tx(
            &mut tx,
            NewAuditLog {
                store_id: Uuid::new_v4(),
                actor_user_id: None,
                action: "PRICE_CHANGE".into(),
                entity: "product".into(),
                entity_id: id.to_string(),
                old_data: None,
                new_data: None,
            },
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("violates") || err.to_string().contains("foreign"));
    tx.rollback().await.unwrap();

    let preco: String = sqlx::query_scalar("SELECT price::text FROM product WHERE id = $1")
        .bind(id)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(preco, "10.00", "rollback preservou o preço");
    assert!(audit_rows(&p, &store).await.is_empty(), "sem linha órfã");
}

#[tokio::test]
async fn audit_nunca_carrega_credencial_ou_segredo() {
    // LGPD: old/new_data guardam SÓ dado fiscal. Mesmo com usuário criado
    // (senha!) e preço alterado na mesma loja, nada sensível pode vazar.
    let p = pool().await;
    let store = mk_store(&p, &uniq("hg")).await;
    let (gerente, _) = manager_login(&p, &store).await;

    let (s, j) = call(&p, &gerente, "POST", "/api/v1/products", Some(produto(&uniq("HG"), 10.00))).await;
    assert_eq!(s, StatusCode::CREATED);
    let id = j["id"].as_str().unwrap().to_string();
    let (s, _) = call(&p, &gerente, "PUT", &format!("/api/v1/products/{id}"), Some(json!({"price": 11.00}))).await;
    assert_eq!(s, StatusCode::OK);

    let raw: Vec<(String, Option<Value>, Option<Value>)> = sqlx::query_as(
        "SELECT action, old_data, new_data FROM audit_log WHERE store_id = $1::uuid",
    )
    .bind(&store)
    .fetch_all(&p)
    .await
    .unwrap();
    assert!(!raw.is_empty());
    for (action, old, new) in &raw {
        let blob = format!("{action}{old:?}{new:?}").to_lowercase();
        for proibido in ["password_hash", "pin_hash", "segredo", "argon2", "bearer ", "jwt_secret"] {
            assert!(!blob.contains(proibido), "auditoria vazou {proibido}: {blob}");
        }
    }
}

#[tokio::test]
async fn put_em_produto_inexistente_404_sem_efeito() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("x32")).await;
    let (gerente, _) = manager_login(&p, &store).await;
    let fantasma = Uuid::new_v4();

    let (s, j) = call(
        &p,
        &gerente,
        "PUT",
        &format!("/api/v1/products/{fantasma}"),
        Some(json!({"price": 99.00})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{j}");
    assert!(audit_rows(&p, &store).await.is_empty());
}
