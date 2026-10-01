//! US04 Task 4.2 — `POST /api/v1/stock/adjust` em lote atômico.
//!
//! - Happy: gerente ajusta lote, saldos resultantes, ledger com o motivo.
//! - Atomicidade: item inválido aborta o lote inteiro (nada persiste).
//! - Trava: saldo negativo sem flag → 422; com flag → passa.
//! - RBAC: caixa → 403; sem token → 401; produto alheio → 404.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{
    AppState, app_router,
    jwt::JwtKeys,
    models::UserRole,
    repositories::PgUserRepository,
    services::{CreateUserInput, create_user},
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use tower::ServiceExt;
use uuid::Uuid;

static SEQ: AtomicU64 = AtomicU64::new(800_001);

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

async fn token_for(p: &sqlx::PgPool, store: &str, role: UserRole) -> String {
    let email = uniq("adj42@loja");
    create_user(
        &PgUserRepository::new(p.clone()),
        store.parse::<Uuid>().unwrap(),
        CreateUserInput {
            name: "F".into(),
            email: email.clone(),
            password: "segredo-123".into(),
            role,
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
    json["token"].as_str().unwrap().to_string()
}

async fn mk_product(p: &sqlx::PgPool, token: &str, sku: &str) -> String {
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/products")
                .header("authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({"name": "P", "sku": sku, "price": 10.00, "cost": 6.00, "ncm": "12345678", "icms_rate": 0}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["id"].as_str().unwrap().to_string()
}

async fn call(
    p: &sqlx::PgPool,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
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

async fn saldo(p: &sqlx::PgPool, store: &str, product: &str) -> Option<rust_decimal::Decimal> {
    sqlx::query_scalar(
        "SELECT quantity FROM stock WHERE store_id = $1::uuid AND product_id = $2::uuid",
    )
    .bind(store)
    .bind(product)
    .fetch_optional(p)
    .await
    .unwrap()
}

#[tokio::test]
async fn ajuste_em_lote_atualiza_saldos_e_ledger() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("h42")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let a = mk_product(&p, &gerente, &uniq("HA")).await;
    let b = mk_product(&p, &gerente, &uniq("HB")).await;

    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({
            "items": [
                {"product_id": a, "qty_delta": 50},
                {"product_id": b, "qty_delta": 3.5},
            ],
            "reason": "NF 123 fornecedor",
        })),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    let saldos: Vec<(String, rust_decimal::Decimal)> = j
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            (
                s["product_id"].as_str().unwrap().to_string(),
                s["quantity"].as_str().unwrap().parse().unwrap(),
            )
        })
        .collect();
    assert!(saldos.contains(&(a.clone(), rust_decimal::Decimal::new(50, 0))));
    assert!(saldos.contains(&(b.clone(), rust_decimal::Decimal::new(35, 1))));
    assert_eq!(
        saldo(&p, &store, &a).await,
        Some(rust_decimal::Decimal::new(50, 0))
    );

    // Ledger com o motivo humano.
    let reasons: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT reason FROM stock_movement WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_all(&p)
            .await
            .unwrap();
    assert_eq!(reasons, vec!["NF 123 fornecedor".to_string()]);
}

#[tokio::test]
async fn lote_com_item_invalido_aborta_tudo() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("ab")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let a = mk_product(&p, &gerente, &uniq("AB")).await;
    let fantasma = Uuid::new_v4().to_string();

    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({
            "items": [
                {"product_id": a, "qty_delta": 10},
                {"product_id": fantasma, "qty_delta": 5},
            ],
            "reason": "inventário",
        })),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND, "{j}");
    // O primeiro item NÃO persistiu (tudo-ou-nada).
    assert!(saldo(&p, &store, &a).await.is_none());
    let n: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM stock_movement WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(n, 0);
}

#[tokio::test]
async fn saldo_negativo_trava_sem_flag_e_passa_com_flag() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("neg")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let prod = mk_product(&p, &gerente, &uniq("NG")).await;

    // Saldo 5; tenta tirar 10 sem flag → 422, nada muda.
    let (s, _) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({"items": [{"product_id": prod, "qty_delta": 5}], "reason": "inicial"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({"items": [{"product_id": prod, "qty_delta": -10}], "reason": "quebra"})),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(5, 0))
    );

    // Liga a flag via PUT e repete: agora passa, saldo -5.
    let (s, _) = call(
        &p,
        Some(&gerente),
        "PUT",
        &format!("/api/v1/products/{prod}"),
        Some(json!({"allow_negative_stock": true})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({"items": [{"product_id": prod, "qty_delta": -10}], "reason": "encomenda"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(-5, 0))
    );
}

#[tokio::test]
async fn caixa_barrado_e_validacoes_422() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("v42")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let caixa = token_for(&p, &store, UserRole::Cashier).await;
    let prod = mk_product(&p, &gerente, &uniq("V42")).await;
    let bom = || json!({"items": [{"product_id": prod, "qty_delta": 1}], "reason": "ok"});

    let (s, j) = call(
        &p,
        Some(&caixa),
        "POST",
        "/api/v1/stock/adjust",
        Some(bom()),
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{j}");
    let (s, _) = call(&p, None, "POST", "/api/v1/stock/adjust", Some(bom())).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);

    for (caso, payload) in [
        ("lote vazio", json!({"items": [], "reason": "x"})),
        (
            "sem motivo",
            json!({"items": [{"product_id": prod, "qty_delta": 1}], "reason": "  "}),
        ),
        (
            "delta zero",
            json!({"items": [{"product_id": prod, "qty_delta": 0}], "reason": "x"}),
        ),
        (
            "delta texto",
            json!({"items": [{"product_id": prod, "qty_delta": "muito"}], "reason": "x"}),
        ),
    ] {
        // Desserialização quebrada cai em 422 texto puro (sem JSON): só o status.
        let res = app_with_db(p.clone())
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/v1/stock/adjust")
                    .header("authorization", format!("Bearer {gerente}"))
                    .header("Content-Type", "application/json")
                    .body(Body::from(payload.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let s = res.status();
        assert!(
            s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
            "{caso}: {s}"
        );
    }

    // Produto de outra loja → 404.
    let outra = mk_store(&p, &uniq("out")).await;
    let tb = token_for(&p, &outra, UserRole::Manager).await;
    let (s, _) = call(
        &p,
        Some(&tb),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({"items": [{"product_id": prod, "qty_delta": 1}], "reason": "spy"})),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
