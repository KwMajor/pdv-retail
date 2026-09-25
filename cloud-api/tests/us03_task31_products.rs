//! US03 Task 3.1 — CRUD de produtos + validação fiscal + RBAC.
//!
//! - Happy: gerente cria (201), lista só ativos do tenant, busca ?q, detalhe.
//! - Edge: NCM com letras/tamanho errado/ausente → 422; dinheiro negativo,
//!   escala 3 casas, barcode e SKU fora do padrão → 422.
//! - Security: caixa em POST/PUT/DELETE → 403 (GET liberado p/ o bip);
//!   cross-tenant → 404; DELETE é soft (linha preservada); sem token → 401.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use pdv_cloud_api::{
    AppState, app_router,
    auth::JwtKeys,
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

static SEQ: AtomicU64 = AtomicU64::new(500_001);

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
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), 4096).await.unwrap();
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    json["token"].as_str().unwrap().to_string()
}

/// Cria usuário direto pelo service e devolve o Bearer via login HTTP.
async fn token_for(p: &sqlx::PgPool, store: &str, role: UserRole) -> String {
    let email = uniq("p31@loja");
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
    login_token(p, store, &email, "segredo-123").await
}

fn req(token: Option<&str>, method: &str, uri: &str, body: Option<Value>) -> Request<Body> {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    if let Some(b) = body {
        builder = builder.header("Content-Type", "application/json");
        builder.body(Body::from(b.to_string())).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    }
}

async fn call(
    p: &sqlx::PgPool,
    token: Option<&str>,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let res = app_with_db(p.clone()).oneshot(req(token, method, uri, body)).await.unwrap();
    let status = res.status();
    let bytes = to_bytes(res.into_body(), 8192).await.unwrap();
    // Rejeições do extractor Axum (422) vêm em texto puro, não JSON.
    let json = serde_json::from_slice(&bytes).unwrap_or(Value::String(
        String::from_utf8_lossy(&bytes).into_owned(),
    ));
    (status, json)
}

fn produto_valido(sku: &str) -> Value {
    json!({
        "name": "Arroz T1 5kg",
        "sku": sku,
        "barcode": "7891234567890",
        "price": 27.99,
        "cost": 21.50,
        "ncm": "10063021",
        "cest": "1234567",
        "cfop": "5102",
        "icms_origin": "0",
        "icms_rate": 18.00,
    })
}

// --- Happy Path -----------------------------------------------------------------

#[tokio::test]
async fn gerente_cria_lista_busca_e_detalha() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("h31")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let sku = uniq("ARROZ31");

    let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/products", Some(produto_valido(&sku))).await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["store_id"], store);
    assert_eq!(j["sku"], sku);
    assert_eq!(j["ncm"], "10063021");
    assert!(j["is_active"].as_bool().unwrap());
    let id = j["id"].as_str().unwrap().to_string();

    // Detalhe e listagem. Dinheiro sai como string (rust_decimal: sem perda
    // de precisão via float) — e entrou como número, provando o parse exato.
    let (s, j) = call(&p, Some(&gerente), "GET", &format!("/api/v1/products/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(j["price"], "27.99");
    assert_eq!(j["cost"], "21.50");
    let (s, j) = call(&p, Some(&gerente), "GET", "/api/v1/products", None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(j.as_array().unwrap().len(), 1);

    // Busca ?q por nome, sku e barcode.
    for q in ["arroz", &sku.to_lowercase(), "7891234567890"] {
        let (s, j) = call(&p, Some(&gerente), "GET", &format!("/api/v1/products?q={q}"), None).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(j.as_array().unwrap().len(), 1, "q={q}");
    }
    let (s, j) = call(&p, Some(&gerente), "GET", "/api/v1/products?q=feijao-xyz", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(j.as_array().unwrap().is_empty());
}

// --- Edge: validação fiscal e de domínio → 422 ------------------------------------

#[tokio::test]
async fn campos_fora_do_dominio_retornam_422() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("v31")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let base = produto_valido(&uniq("V31"));

    let caso = |campo: &str, valor: Value| {
        let mut payload = base.clone();
        payload[campo] = valor;
        payload
    };
    for (nome, payload) in [
        ("ncm letras", caso("ncm", json!("1234567a"))),
        ("ncm 7 dígitos", caso("ncm", json!("1234567"))),
        ("ncm 9 dígitos", caso("ncm", json!("123456789"))),
        ("ncm vazio", caso("ncm", json!(""))),
        ("preço negativo", caso("price", json!(-1))),
        ("preço 3 casas", caso("price", json!(10.999))),
        ("barcode curto", caso("barcode", json!("123"))),
        ("barcode letra", caso("barcode", json!("7891234a"))),
        ("sku símbolo", caso("sku", json!("ARROZ@!"))),
        ("cfop 3 dígitos", caso("cfop", json!("510"))),
        ("origem 9", caso("icms_origin", json!("9"))),
        ("alíquota >100", caso("icms_rate", json!(101))),
    ] {
        let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/products", Some(payload)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{nome}: {j}");
        assert_eq!(j["code"], "UNPROCESSABLE");
    }

    // NCM ausente: extractor barra antes do handler (422 texto puro).
    let mut sem_ncm = base.clone();
    sem_ncm.as_object_mut().unwrap().remove("ncm");
    let (s, _) = call(&p, Some(&gerente), "POST", "/api/v1/products", Some(sem_ncm)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

// --- Security: RBAC, tenant, soft delete --------------------------------------------

#[tokio::test]
async fn caixa_barrado_em_escrita_mas_leitura_liberada() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("rb31")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let caixa = token_for(&p, &store, UserRole::Cashier).await;

    let (s, _) = call(&p, Some(&caixa), "POST", "/api/v1/products", Some(produto_valido(&uniq("RB31")))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // Produto criado pelo gerente para os GETs do caixa.
    let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/products", Some(produto_valido(&uniq("RB31B")))).await;
    assert_eq!(s, StatusCode::CREATED);
    let id = j["id"].as_str().unwrap().to_string();

    let (s, _) = call(&p, Some(&caixa), "GET", "/api/v1/products", None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = call(&p, Some(&caixa), "GET", &format!("/api/v1/products/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, j) = call(&p, Some(&caixa), "PUT", &format!("/api/v1/products/{id}"), Some(json!({"name": "X"}))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{j}");
    let (s, _) = call(&p, Some(&caixa), "DELETE", &format!("/api/v1/products/{id}"), None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // Sem credencial: 401 em tudo.
    let (s, _) = call(&p, None, "GET", "/api/v1/products", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn isolamento_tenant_404_e_soft_delete() {
    let p = pool().await;
    let a = mk_store(&p, &uniq("ta31")).await;
    let b = mk_store(&p, &uniq("tb31")).await;
    let ta = token_for(&p, &a, UserRole::Manager).await;
    let tb = token_for(&p, &b, UserRole::Manager).await;

    let (s, j) = call(&p, Some(&ta), "POST", "/api/v1/products", Some(produto_valido(&uniq("TA31")))).await;
    assert_eq!(s, StatusCode::CREATED);
    let id = j["id"].as_str().unwrap().to_string();

    // Outra loja: 404 em detalhe/PUT/DELETE (nunca vaza nem confirma existência).
    for (m, uri, body) in [
        ("GET", format!("/api/v1/products/{id}"), None),
        ("PUT", format!("/api/v1/products/{id}"), Some(json!({"name": "Spy"}))),
        ("DELETE", format!("/api/v1/products/{id}"), None),
    ] {
        let (s, _) = call(&p, Some(&tb), m, &uri, body).await;
        assert_eq!(s, StatusCode::NOT_FOUND, "{m}");
    }
    // Lista da loja B vazia.
    let (s, j) = call(&p, Some(&tb), "GET", "/api/v1/products", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(j.as_array().unwrap().is_empty());

    // PUT válido na loja dona.
    let (s, j) = call(&p, Some(&ta), "PUT", &format!("/api/v1/products/{id}"), Some(json!({"name": "Arroz T1 1kg", "cfop": "6102"}))).await;
    assert_eq!(s, StatusCode::OK, "{j}");
    assert_eq!(j["name"], "Arroz T1 1kg");
    assert_eq!(j["cfop"], "6102");

    // DELETE é soft: some da lista, linha preservada com is_active=false.
    let (s, j) = call(&p, Some(&ta), "DELETE", &format!("/api/v1/products/{id}"), None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(!j["is_active"].as_bool().unwrap());
    let (s, j) = call(&p, Some(&ta), "GET", "/api/v1/products", None).await;
    assert_eq!(s, StatusCode::OK);
    assert!(j.as_array().unwrap().is_empty());
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(n, 1, "soft delete não remove a linha");
}

#[tokio::test]
async fn sku_duplicado_409_mas_reuso_entre_lojas() {
    let p = pool().await;
    let a = mk_store(&p, &uniq("ea31")).await;
    let b = mk_store(&p, &uniq("eb31")).await;
    let ta = token_for(&p, &a, UserRole::Manager).await;
    let tb = token_for(&p, &b, UserRole::Manager).await;
    let sku = uniq("DUP31");

    let (s1, _) = call(&p, Some(&ta), "POST", "/api/v1/products", Some(produto_valido(&sku))).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, j2) = call(&p, Some(&ta), "POST", "/api/v1/products", Some(produto_valido(&sku))).await;
    assert_eq!(s2, StatusCode::CONFLICT);
    assert_eq!(j2["code"], "CONFLICT");
    let (s3, _) = call(&p, Some(&tb), "POST", "/api/v1/products", Some(produto_valido(&sku))).await;
    assert_eq!(s3, StatusCode::CREATED, "mesmo sku em outra loja deve passar");
}
