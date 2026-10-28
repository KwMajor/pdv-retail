//! US07 Task 7.1 — `POST /api/v1/sales` com pagamento fracionado.
//!
//! - Happy: venda fecha com troco, snapshot fiscal congelado, pagamentos
//!   persistidos e baixa de estoque atômica (ledger `SALE`).
//! - Matemática: soma dos pagamentos != total → 422 sem persistir nada.
//! - Anti-fraude: troco em PIX/cartão → 422; dinheiro exige `tendered`.
//! - Validações: itens/pagamentos vazios, escala de dinheiro, quantidade,
//!   desconto maior que o bruto da linha → 422.
//! - Estoque: insuficiente sem flag → 422 com rollback; com flag → passa.
//! - Isolamento: produto/cliente de outra loja → 404; sem token → 401;
//!   caixa (não-gestão) pode vender.

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

static SEQ: AtomicU64 = AtomicU64::new(900_001);

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
        cert_master_key: [7u8; 32],
    })
}

async fn token_for(p: &sqlx::PgPool, store: &str, role: UserRole) -> String {
    let email = uniq("s71@loja");
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
    let bytes = to_bytes(res.into_body(), 16384).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

/// Payload deliberadamente inválido: a desserialização pode cair em 422
/// (erro de dados) ou 400 (erro de sintaxe) com corpo cru — sem envelope.
async fn call_raw(p: &sqlx::PgPool, token: &str, body: Value) -> StatusCode {
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/sales")
                .header("authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    res.status()
}

async fn mk_product(p: &sqlx::PgPool, token: &str, sku: &str, price: &str) -> String {
    let res = app_with_db(p.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/products")
                .header("authorization", format!("Bearer {token}"))
                .header("Content-Type", "application/json")
                .body(Body::from(
                    json!({
                        "name": "P",
                        "sku": sku,
                        "price": price,
                        "cost": "6.00",
                        "ncm": "10063021",
                        "cest": "1234567",
                        "cfop": "5102",
                        "icms_origin": "0",
                        "icms_rate": "18.00",
                    })
                    .to_string(),
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

async fn add_stock(p: &sqlx::PgPool, token: &str, product: &str, qty: f64) {
    let (s, j) = call(
        p,
        Some(token),
        "POST",
        "/api/v1/stock/adjust",
        Some(json!({"items": [{"product_id": product, "qty_delta": qty}], "reason": "carga inicial"})),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{j}");
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

async fn count(p: &sqlx::PgPool, sql: &str, store: &str) -> i64 {
    sqlx::query_scalar(sql)
        .bind(store)
        .fetch_one(p)
        .await
        .unwrap()
}

fn sale_payload(product: &str, payments: Value) -> Value {
    json!({
        "items": [{"product_id": product, "quantity": 2, "discount": "1.00"}],
        "payments": payments,
    })
}

#[tokio::test]
async fn venda_fracionada_fecha_com_troco_snapshot_e_estoque() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("h71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let prod = mk_product(&p, &gerente, &uniq("H71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 10.0).await;

    // 2×10.00 - 1.00 = 19.00; cash 15 (entregou 20 → troco 5) + pix 4.
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(
            &prod,
            json!([
                {"method": "cash", "amount": "15.00", "tendered_amount": "20.00"},
                {"method": "pix", "amount": "4.00"},
            ]),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["status"], "closed");
    assert_eq!(j["subtotal"], "20.00");
    assert_eq!(j["discount"], "1.00");
    assert_eq!(j["total"], "19.00");
    assert_eq!(j["change_amount"], "5.00");
    let items = j["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["unit_price"], "10.00");
    assert_eq!(items[0]["total"], "19.00");
    // Snapshot fiscal congelado do produto (não do payload).
    assert_eq!(items[0]["ncm_code"], "10063021");
    assert_eq!(items[0]["cfop"], "5102");
    assert_eq!(items[0]["icms_origin"], "0");
    assert_eq!(items[0]["icms_rate"], "18.00");
    let payments = j["payments"].as_array().unwrap();
    assert_eq!(payments.len(), 2);
    assert_eq!(payments[0]["method"], "cash");
    assert_eq!(payments[0]["tendered_amount"], "20.00");

    // Estoque baixado e ledger com o vínculo da venda.
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(8, 0))
    );
    let (reason, qty, ref_sale): (String, rust_decimal::Decimal, Option<String>) = sqlx::query_as(
        "SELECT reason, qty_delta, ref_sale_id::text FROM stock_movement
             WHERE store_id = $1::uuid AND product_id = $2::uuid AND reason = 'SALE'",
    )
    .bind(&store)
    .bind(&prod)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(reason, "SALE");
    assert_eq!(qty, rust_decimal::Decimal::new(-2, 0));
    assert_eq!(ref_sale.as_deref(), j["id"].as_str());
}

#[tokio::test]
async fn soma_divergente_nao_persiste_nada() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("mm71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let prod = mk_product(&p, &gerente, &uniq("MM71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 10.0).await;

    // Total 19.00; pagamentos 15 + 3 = 18.00 → 422 (falta 1 centavo).
    for pagos in [
        json!([{"method": "cash", "amount": "15.00", "tendered_amount": "15.00"}, {"method": "pix", "amount": "3.00"}]),
        json!([{"method": "cash", "amount": "20.00", "tendered_amount": "20.00"}]),
    ] {
        let (s, j) = call(
            &p,
            Some(&gerente),
            "POST",
            "/api/v1/sales",
            Some(sale_payload(&prod, pagos)),
        )
        .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    }
    // Rollback total: nada de venda, item, pagamento ou movimento.
    assert_eq!(
        count(
            &p,
            "SELECT COUNT(*) FROM sale WHERE store_id = $1::uuid",
            &store
        )
        .await,
        0
    );
    assert_eq!(
        count(
            &p,
            "SELECT COUNT(*) FROM stock_movement WHERE store_id = $1::uuid AND reason = 'SALE'",
            &store
        )
        .await,
        0
    );
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(10, 0))
    );
}

#[tokio::test]
async fn troco_so_no_dinheiro_e_tendered_obrigatorio() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("tr71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let prod = mk_product(&p, &gerente, &uniq("TR71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 10.0).await;

    // PIX com troco (entregou 100 p/ consumir 19) → fraude barrada.
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(
            &prod,
            json!([{"method": "pix", "amount": "19.00", "tendered_amount": "100.00"}]),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");

    // Dinheiro sem tendered → 422 (não há como dar troco sem valor entregue).
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(
            &prod,
            json!([{"method": "cash", "amount": "19.00"}]),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");

    // Dinheiro com tendered menor que amount → 422.
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(
            &prod,
            json!([{"method": "cash", "amount": "19.00", "tendered_amount": "10.00"}]),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");

    // Exato: troco 0.00 e venda liberada.
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(
            &prod,
            json!([{"method": "cash", "amount": "19.00", "tendered_amount": "19.00"}]),
        )),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(j["change_amount"], "0.00");
}

#[tokio::test]
async fn validacoes_de_dominio_retornam_422() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("v71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let caixa = token_for(&p, &store, UserRole::Cashier).await;
    let prod = mk_product(&p, &gerente, &uniq("V71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 10.0).await;

    let bom_cash = json!([{"method": "cash", "amount": "19.00", "tendered_amount": "19.00"}]);
    for (caso, payload) in [
        (
            "itens vazios",
            json!({"items": [], "payments": bom_cash.clone()}),
        ),
        (
            "pagamentos vazios",
            json!({"items": [{"product_id": prod, "quantity": 1}], "payments": []}),
        ),
        (
            "quantidade zero",
            json!({"items": [{"product_id": prod, "quantity": 0}], "payments": bom_cash.clone()}),
        ),
        (
            "quantidade 4 casas",
            json!({"items": [{"product_id": prod, "quantity": "1.0001"}], "payments": bom_cash.clone()}),
        ),
        (
            "desconto maior que o bruto",
            json!({"items": [{"product_id": prod, "quantity": 1, "discount": "11.00"}], "payments": bom_cash.clone()}),
        ),
        (
            "desconto com 3 casas",
            json!({"items": [{"product_id": prod, "quantity": 1, "discount": "0.001"}], "payments": bom_cash.clone()}),
        ),
        (
            "amount com 3 casas",
            json!({"items": [{"product_id": prod, "quantity": 2, "discount": "1.00"}], "payments": [{"method": "cash", "amount": "19.001", "tendered_amount": "19.01"}]}),
        ),
        (
            "amount negativo",
            json!({"items": [{"product_id": prod, "quantity": 2, "discount": "1.00"}], "payments": [{"method": "cash", "amount": "-1.00", "tendered_amount": "19.00"}]}),
        ),
        (
            "cpf curto",
            json!({"anonymous_cpf_cnpj": "123", "items": [{"product_id": prod, "quantity": 2, "discount": "1.00"}], "payments": bom_cash.clone()}),
        ),
    ] {
        let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/sales", Some(payload)).await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{caso}: {j}");
    }

    // Método fora do enum fechado (crediario é da US18) → 422/400 na
    // desserialização, sem tocar no banco.
    let s = call_raw(
        &p,
        &gerente,
        json!({"items": [{"product_id": prod, "quantity": 2, "discount": "1.00"}], "payments": [{"method": "crediario", "amount": "19.00"}]}),
    )
    .await;
    assert!(
        s == StatusCode::UNPROCESSABLE_ENTITY || s == StatusCode::BAD_REQUEST,
        "crediario: {s}"
    );
    assert_eq!(
        count(
            &p,
            "SELECT COUNT(*) FROM sale WHERE store_id = $1::uuid",
            &store
        )
        .await,
        0
    );

    // CPF anônimo válido persiste.
    let (s, j) = call(
        &p,
        Some(&caixa),
        "POST",
        "/api/v1/sales",
        Some(json!({
            "anonymous_cpf_cnpj": "12345678901",
            "items": [{"product_id": prod, "quantity": 2, "discount": "1.00"}],
            "payments": bom_cash,
        })),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    let cpf: Option<String> =
        sqlx::query_scalar("SELECT anonymous_cpf_cnpj FROM sale WHERE store_id = $1::uuid")
            .bind(&store)
            .fetch_one(&p)
            .await
            .unwrap();
    assert_eq!(cpf.as_deref(), Some("12345678901"));
}

#[tokio::test]
async fn estoque_insuficiente_rollback_e_flag_libera() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("neg71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let prod = mk_product(&p, &gerente, &uniq("NG71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 1.0).await;

    let payload = || sale_payload(&prod, json!([{"method": "pix", "amount": "19.00"}]));
    // Saldo 1, venda de 2 → 422 e rollback (nem venda nem movimento).
    let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/sales", Some(payload())).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
    assert_eq!(
        count(
            &p,
            "SELECT COUNT(*) FROM sale WHERE store_id = $1::uuid",
            &store
        )
        .await,
        0
    );
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(1, 0))
    );

    // Com allow_negative_stock, a venda passa e o saldo fica -1.
    let (s, _) = call(
        &p,
        Some(&gerente),
        "PUT",
        &format!("/api/v1/products/{prod}"),
        Some(json!({"allow_negative_stock": true})),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/sales", Some(payload())).await;
    assert_eq!(s, StatusCode::CREATED, "{j}");
    assert_eq!(
        saldo(&p, &store, &prod).await,
        Some(rust_decimal::Decimal::new(-1, 0))
    );
}

#[tokio::test]
async fn isolamento_de_tenant_rbac_e_cliente() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("ten71")).await;
    let gerente = token_for(&p, &store, UserRole::Manager).await;
    let caixa = token_for(&p, &store, UserRole::Cashier).await;
    let prod = mk_product(&p, &gerente, &uniq("TN71"), "10.00").await;
    add_stock(&p, &gerente, &prod, 10.0).await;

    let pag = json!([{"method": "pix", "amount": "19.00"}]);
    // Sem token → 401; caixa pode vender (é o operador do PDV).
    let (s, _) = call(
        &p,
        None,
        "POST",
        "/api/v1/sales",
        Some(sale_payload(&prod, pag.clone())),
    )
    .await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, j) = call(
        &p,
        Some(&caixa),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(&prod, pag.clone())),
    )
    .await;
    assert_eq!(s, StatusCode::CREATED, "{j}");

    // Produto de outra loja → 404 (não vaza existência).
    let outra = mk_store(&p, &uniq("out71")).await;
    let tb = token_for(&p, &outra, UserRole::Manager).await;
    let prod_b = mk_product(&p, &tb, &uniq("OUT71"), "10.00").await;
    let (s, _) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(&prod_b, pag.clone())),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    // Produto inativo → 422.
    let (s, _) = call(
        &p,
        Some(&gerente),
        "DELETE",
        &format!("/api/v1/products/{prod}"),
        None,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, j) = call(
        &p,
        Some(&gerente),
        "POST",
        "/api/v1/sales",
        Some(sale_payload(&prod, pag.clone())),
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");

    // Cliente fantasma → 404; cliente inativo da loja → 422.
    let fantasma = Uuid::new_v4().to_string();
    let payload = json!({
        "customer_id": fantasma,
        "items": [{"product_id": prod, "quantity": 1}],
        "payments": [{"method": "pix", "amount": "10.00"}],
    });
    let (s, _) = call(&p, Some(&gerente), "POST", "/api/v1/sales", Some(payload)).await;
    assert_eq!(s, StatusCode::NOT_FOUND);

    let inativo: String = sqlx::query_scalar(
        "INSERT INTO customer(store_id, name, is_active) VALUES ($1::uuid, 'Inativo', false) RETURNING id::text",
    )
    .bind(&store)
    .fetch_one(&p)
    .await
    .unwrap();
    let payload = json!({
        "customer_id": inativo,
        "items": [{"product_id": prod, "quantity": 1}],
        "payments": [{"method": "pix", "amount": "10.00"}],
    });
    let (s, j) = call(&p, Some(&gerente), "POST", "/api/v1/sales", Some(payload)).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{j}");
}
