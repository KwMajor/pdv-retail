//! US04 Task 4.1 — Motor de movimentação: ledger + saldo na mesma transação.
//!
//! - Happy (QA): `+50 MANUAL_ADD` → saldo 50; `-2 SALE` → saldo 48.
//! - Ledger preservado: 2 linhas imutáveis, motivos tipados.
//! - Delta zero → erro; produto de outra loja → NotFound (sem vazar).

use pdv_cloud_api::{
    repositories::{
        NewProduct, PgProductRepository, PgStockRepository, ProductRepository, StockRepository,
    },
    services::stock_service::{ApplyMovementInput, MovementError, MovementType, apply_movement},
};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use uuid::Uuid;

static SEQ: AtomicU64 = AtomicU64::new(700_001);

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

fn dec(cents: i64) -> Decimal {
    Decimal::new(cents, 2)
}

async fn mk_store(p: &sqlx::PgPool, tag: &str) -> Uuid {
    sqlx::query_scalar("INSERT INTO store_settings(name, cnpj) VALUES ($1, $2) RETURNING id")
        .bind(format!("Loja {tag}"))
        .bind(format!("cnpj-{tag}"))
        .fetch_one(p)
        .await
        .unwrap()
}

async fn mk_product(p: &sqlx::PgPool, store: Uuid, sku: &str) -> Uuid {
    PgProductRepository::new(p.clone())
        .create(NewProduct {
            store_id: store,
            sku: sku.into(),
            barcode: None,
            name: "P".into(),
            price: dec(100),
            cost: Decimal::ZERO,
            ncm: Some("12345678".into()),
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap()
        .id
}

async fn saldo(p: &sqlx::PgPool, store: Uuid, product: Uuid) -> Decimal {
    PgStockRepository::new(p.clone())
        .get(store, product)
        .await
        .unwrap()
        .map(|s| s.quantity)
        .unwrap_or(Decimal::ZERO)
}

#[tokio::test]
async fn ledger_e_saldo_no_cenario_do_qa() {
    let p = pool().await;
    let store = mk_store(&p, &uniq("qa")).await;
    let product = mk_product(&p, store, &uniq("QA")).await;

    let (_, s1) = apply_movement(
        &p,
        store,
        ApplyMovementInput {
            product_id: product,
            qty_delta: Decimal::new(50, 0),
            movement: MovementType::ManualAdd,
            ref_sale_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(s1.quantity, Decimal::new(50, 0));

    let (_, s2) = apply_movement(
        &p,
        store,
        ApplyMovementInput {
            product_id: product,
            qty_delta: Decimal::new(-2, 0),
            movement: MovementType::Sale,
            ref_sale_id: None,
        },
    )
    .await
    .unwrap();
    assert_eq!(s2.quantity, Decimal::new(48, 0));
    assert_eq!(saldo(&p, store, product).await, Decimal::new(48, 0));

    // Ledger: 2 linhas imutáveis com os motivos tipados.
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM stock_movement WHERE store_id = $1 AND product_id = $2",
    )
    .bind(store)
    .bind(product)
    .fetch_one(&p)
    .await
    .unwrap();
    assert_eq!(n, 2);
    let reasons: Vec<String> = sqlx::query_scalar(
        "SELECT reason FROM stock_movement WHERE store_id = $1 AND product_id = $2 ORDER BY created_at",
    )
    .bind(store)
    .bind(product)
    .fetch_all(&p)
    .await
    .unwrap();
    assert_eq!(reasons, vec!["MANUAL_ADD".to_string(), "SALE".to_string()]);
}

#[tokio::test]
async fn delta_zero_e_produto_alheio_rejeitados() {
    let p = pool().await;
    let a = mk_store(&p, &uniq("za")).await;
    let b = mk_store(&p, &uniq("zb")).await;
    let prod_a = mk_product(&p, a, &uniq("ZA")).await;

    let err = apply_movement(
        &p,
        a,
        ApplyMovementInput {
            product_id: prod_a,
            qty_delta: Decimal::ZERO,
            movement: MovementType::ManualAdd,
            ref_sale_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MovementError::Invalid(_)));

    // Produto da loja A movimentado com tenant B: NotFound, nada gravado.
    let err = apply_movement(
        &p,
        b,
        ApplyMovementInput {
            product_id: prod_a,
            qty_delta: Decimal::ONE,
            movement: MovementType::ManualAdd,
            ref_sale_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MovementError::ProductNotFound));
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movement WHERE store_id = $1")
        .bind(b)
        .fetch_one(&p)
        .await
        .unwrap();
    assert_eq!(n, 0);
}
