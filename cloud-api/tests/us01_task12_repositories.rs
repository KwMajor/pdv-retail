//! US01 Task 1.2 — Models + Traits de Repositório.
//!
//! - Happy Path: structs compilam, tipos batem com o banco (queries checadas
//!   em compilação via `query!`/`query_as!` + `cargo sqlx prepare`), roundtrip
//!   via repositórios com o `store_id` correto.
//! - Security Case: isolamento via repositório (tenant errado → `None` ou FK)
//!   e varredura automatizada de `src/repositories/` proibindo montagem
//!   dinâmica de SQL (só prepared statements com binds).
//!
//! Banco dedicado `pdv_test` (mesmo da suite US01): setup próprio cria o banco
//! e aplica `./migrations`. Requer Postgres: `docker compose up -d`.

use pdv_cloud_api::repositories::*;
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;
use uuid::Uuid;

// --- infra (mesmo padrão da suite US01) -------------------------------------

static SEQ: AtomicU64 = AtomicU64::new(100_001);

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

async fn fresh_pool() -> sqlx::PgPool {
    ensure_setup().await;
    PgPoolOptions::new()
        .max_connections(2)
        .acquire_timeout(Duration::from_secs(10))
        .connect_lazy(&test_db_url())
        .expect("TEST_DATABASE_URL inválida")
}

async fn ensure_setup() {
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
        let setup_pool = PgPoolOptions::new()
            .max_connections(2)
            .acquire_timeout(Duration::from_secs(10))
            .connect_lazy(&url)
            .expect("TEST_DATABASE_URL inválida");
        sqlx::migrate!("./migrations")
            .run(&setup_pool)
            .await
            .expect("falha ao aplicar migrations no banco de teste");
    })
    .await;
}

fn dec(cents: i64) -> Decimal {
    Decimal::new(cents, 2)
}

// --- Happy Path: structs + serde --------------------------------------------

#[test]
fn models_serializam_sem_perder_tipos() {
    // UUID/Decimal sobrevivem ao roundtrip JSON (pecisão monetária intacta).
    let store_id = Uuid::new_v4();
    let input = NewProduct {
        store_id,
        sku: "SKU-1".into(),
        barcode: None,
        name: "Arroz".into(),
        price: dec(1099),
        cost: dec(800),
        ncm: Some("10063021".into()),
        cest: None,
        cfop: Some("5102".into()),
        icms_origin: Some("0".into()),
        icms_rate: dec(1800),
    };
    let json = serde_json::to_value(&input).unwrap();
    // rust_decimal serializa dinheiro como string: sem perda de precisão via float.
    assert_eq!(json["price"], serde_json::Value::String("10.99".into()));
    assert_eq!(
        json["store_id"],
        serde_json::Value::String(store_id.to_string())
    );
    // E o roundtrip preserva os tipos exatos.
    let back: NewProduct = serde_json::from_value(json).unwrap();
    assert_eq!(back.price, dec(1099));
    assert_eq!(back.store_id, store_id);
}

// --- Happy Path via repositórios --------------------------------------------

#[tokio::test]
async fn store_settings_crud_via_repo() {
    let pool = fresh_pool().await;
    let repo = PgStoreSettingsRepository::new(pool);
    let tag = uniq("loja12");
    let created = repo
        .create(NewStoreSettings {
            name: format!("Loja {tag}"),
            cnpj: format!("cnpj-{tag}"),
        })
        .await
        .unwrap();
    assert_eq!(created.name, format!("Loja {tag}"));
    let found = repo.find_by_id(created.id).await.unwrap().unwrap();
    assert_eq!(found, created);
    let renamed = repo
        .rename(created.id, &format!("Loja Nova {tag}"))
        .await
        .unwrap();
    assert!(renamed.name.contains("Nova"));
    assert_eq!(repo.delete(renamed.id).await.unwrap(), 1);
    assert!(repo.find_by_id(created.id).await.unwrap().is_none());
}

#[tokio::test]
async fn produto_isolado_por_store_id_via_repo() {
    let pool = fresh_pool().await;
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let products = PgProductRepository::new(pool);
    let ta = uniq("pa");
    let tb = uniq("pb");
    let a = stores
        .create(NewStoreSettings {
            name: format!("A {ta}"),
            cnpj: format!("cnpj-{ta}"),
        })
        .await
        .unwrap();
    let b = stores
        .create(NewStoreSettings {
            name: format!("B {tb}"),
            cnpj: format!("cnpj-{tb}"),
        })
        .await
        .unwrap();

    let sku = uniq("SKU12");
    let created = products
        .create(NewProduct {
            store_id: a.id,
            sku: sku.clone(),
            barcode: None,
            name: "Feijão".into(),
            price: dec(899),
            cost: Decimal::ZERO,
            ncm: None,
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap();

    // Tenant correto enxerga; tenant errado recebe None (nunca a linha alheia).
    assert_eq!(
        products
            .find_by_id(a.id, created.id)
            .await
            .unwrap()
            .unwrap()
            .id,
        created.id
    );
    assert!(
        products
            .find_by_id(b.id, created.id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(products.find_by_sku(b.id, &sku).await.unwrap().is_none());
    assert!(products.list_active(b.id, 50).await.unwrap().is_empty());
    assert_eq!(products.list_active(a.id, 50).await.unwrap().len(), 1);
}

#[tokio::test]
async fn usuario_soft_delete_via_repo() {
    let pool = fresh_pool().await;
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let users = PgUserRepository::new(pool);
    let tag = uniq("u12");
    let store = stores
        .create(NewStoreSettings {
            name: format!("Loja {tag}"),
            cnpj: format!("cnpj-{tag}"),
        })
        .await
        .unwrap();
    let email = uniq("caixa12@loja");
    let created = users
        .create(NewUser {
            store_id: store.id,
            name: "Caixa".into(),
            email: email.clone(),
            password_hash: "argon2:hash".into(),
            role: "cashier".into(),
        })
        .await
        .unwrap();
    assert!(created.is_active);
    assert_eq!(
        users
            .find_by_email(store.id, &email)
            .await
            .unwrap()
            .unwrap()
            .id,
        created.id
    );
    let off = users.deactivate(store.id, created.id).await.unwrap();
    assert!(!off.is_active);
    // Linha preservada (FKs do histórico) mas fora da listagem de ativos.
    assert!(
        users
            .find_by_id(store.id, created.id)
            .await
            .unwrap()
            .is_some()
    );
    assert!(users.list_active(store.id, 50).await.unwrap().is_empty());
}

#[tokio::test]
async fn sale_item_snapshot_e_payment_via_repo() {
    let pool = fresh_pool().await;
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let products = PgProductRepository::new(pool.clone());
    let sales = PgSaleRepository::new(pool.clone());
    let items = PgSaleItemRepository::new(pool.clone());
    let payments = PgPaymentRepository::new(pool.clone());
    let tag = uniq("v12");
    let store = stores
        .create(NewStoreSettings {
            name: format!("Loja {tag}"),
            cnpj: format!("cnpj-{tag}"),
        })
        .await
        .unwrap();
    let product = products
        .create(NewProduct {
            store_id: store.id,
            sku: uniq("SNAP12"),
            barcode: None,
            name: "Leite".into(),
            price: dec(599),
            cost: dec(350),
            ncm: Some("04012010".into()),
            cest: None,
            cfop: Some("5102".into()),
            icms_origin: Some("0".into()),
            icms_rate: dec(1800),
        })
        .await
        .unwrap();
    let sale = sales
        .create(NewSale {
            store_id: store.id,
            customer_id: None,
            anonymous_cpf_cnpj: Some("12345678901".into()),
            status: "closed".into(),
            subtotal: dec(599),
            discount: Decimal::ZERO,
            total: dec(599),
            change_amount: Decimal::ZERO,
            created_by: None,
        })
        .await
        .unwrap();

    // Snapshot congela o fiscal vigente, mesmo que o produto mude depois.
    let item = items
        .create(NewSaleItem {
            store_id: store.id,
            sale_id: sale.id,
            product_id: product.id,
            quantity: Decimal::ONE,
            unit_price: dec(599),
            // Mapeamento explícito de product.cost (regra anti-NULL).
            unit_cost_price: dec(350),
            discount: Decimal::ZERO,
            total: dec(599),
            ncm_code: Some("04012010".into()),
            cest: None,
            cfop: Some("5102".into()),
            icms_origin: Some("0".into()),
            icms_rate: dec(1800),
        })
        .await
        .unwrap();
    products
        .set_price(store.id, product.id, dec(799))
        .await
        .unwrap();
    sqlx::query("UPDATE product SET cost = $1 WHERE id = $2::uuid")
        .bind(dec(400))
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();
    let frozen = items.find_by_id(store.id, item.id).await.unwrap().unwrap();
    assert_eq!(frozen.unit_price, dec(599));
    assert_eq!(frozen.unit_cost_price, dec(350), "custo congelado na venda");
    assert_eq!(frozen.ncm_code.as_deref(), Some("04012010"));
    assert_eq!(
        items.list_by_sale(store.id, sale.id).await.unwrap().len(),
        1
    );

    // Pagamento fracionado: PIX + dinheiro com troco implícito.
    payments
        .create(NewPayment {
            store_id: store.id,
            sale_id: sale.id,
            method: "pix".into(),
            amount: dec(400),
            tendered_amount: None,
        })
        .await
        .unwrap();
    payments
        .create(NewPayment {
            store_id: store.id,
            sale_id: sale.id,
            method: "cash".into(),
            amount: dec(199),
            tendered_amount: Some(dec(200)),
        })
        .await
        .unwrap();
    let got = payments.list_by_sale(store.id, sale.id).await.unwrap();
    assert_eq!(got.len(), 2);
}

// --- Security Case -----------------------------------------------------------

#[tokio::test]
async fn cross_tenant_barrado_no_banco_via_repo() {
    let pool = fresh_pool().await;
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let products = PgProductRepository::new(pool.clone());
    let sales = PgSaleRepository::new(pool.clone());
    let items = PgSaleItemRepository::new(pool.clone());
    let payments = PgPaymentRepository::new(pool);
    let ta = uniq("xa");
    let tb = uniq("xb");
    let a = stores
        .create(NewStoreSettings {
            name: format!("A {ta}"),
            cnpj: format!("cnpj-{ta}"),
        })
        .await
        .unwrap();
    let b = stores
        .create(NewStoreSettings {
            name: format!("B {tb}"),
            cnpj: format!("cnpj-{tb}"),
        })
        .await
        .unwrap();
    let product = products
        .create(NewProduct {
            store_id: a.id,
            sku: uniq("XA"),
            barcode: None,
            name: "P".into(),
            price: dec(100),
            cost: Decimal::ZERO,
            ncm: None,
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap();
    let sale = sales
        .create(NewSale {
            store_id: a.id,
            customer_id: None,
            anonymous_cpf_cnpj: None,
            status: "open".into(),
            subtotal: dec(100),
            discount: Decimal::ZERO,
            total: dec(100),
            change_amount: Decimal::ZERO,
            created_by: None,
        })
        .await
        .unwrap();

    // Venda da loja A carimbada com a loja B: a FK composta barra no banco.
    let item_err = items
        .create(NewSaleItem {
            store_id: b.id,
            sale_id: sale.id,
            product_id: product.id,
            quantity: Decimal::ONE,
            unit_price: dec(100),
            unit_cost_price: Decimal::ZERO,
            discount: Decimal::ZERO,
            total: dec(100),
            ncm_code: None,
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap_err();
    assert!(item_err.to_string().contains("violates") || item_err.to_string().contains("foreign"));
    let pay_err = payments
        .create(NewPayment {
            store_id: b.id,
            sale_id: sale.id,
            method: "pix".into(),
            amount: dec(100),
            tendered_amount: None,
        })
        .await
        .unwrap_err();
    assert!(pay_err.to_string().contains("violates") || pay_err.to_string().contains("foreign"));
}

#[tokio::test]
async fn sale_item_congela_custo_para_lucro_bruto() {
    // Lucro futuro usa o custo CONGELADO, nunca o atual.
    // 2 un a R$ 5,99 com custo R$ 3,00 → lucro R$ 5,98 mesmo após o custo virar R$ 9,99.
    let pool = fresh_pool().await;
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let products = PgProductRepository::new(pool.clone());
    let sales = PgSaleRepository::new(pool.clone());
    let items = PgSaleItemRepository::new(pool.clone());
    let tag = uniq("lc");
    let store = stores
        .create(NewStoreSettings {
            name: format!("Loja {tag}"),
            cnpj: format!("cnpj-{tag}"),
        })
        .await
        .unwrap();
    let product = products
        .create(NewProduct {
            store_id: store.id,
            sku: uniq("LC"),
            barcode: None,
            name: "Custo".into(),
            price: dec(599),
            cost: dec(300),
            ncm: None,
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap();
    let sale = sales
        .create(NewSale {
            store_id: store.id,
            customer_id: None,
            anonymous_cpf_cnpj: None,
            status: "closed".into(),
            subtotal: dec(1198),
            discount: Decimal::ZERO,
            total: dec(1198),
            change_amount: Decimal::ZERO,
            created_by: None,
        })
        .await
        .unwrap();
    items
        .create(NewSaleItem {
            store_id: store.id,
            sale_id: sale.id,
            product_id: product.id,
            quantity: Decimal::new(2, 0),
            unit_price: dec(599),
            // Mapeamento explícito do custo vigente (regra anti-NULL).
            unit_cost_price: dec(300),
            discount: Decimal::ZERO,
            total: dec(1198),
            ncm_code: None,
            cest: None,
            cfop: None,
            icms_origin: None,
            icms_rate: Decimal::ZERO,
        })
        .await
        .unwrap();

    // Custo atual dispara — o item vendido não pode se mover.
    sqlx::query("UPDATE product SET cost = $1 WHERE id = $2::uuid")
        .bind(dec(999))
        .bind(product.id)
        .execute(&pool)
        .await
        .unwrap();

    // Lucro bruto da venda (US17): SUM(qty*price - qty*cost_congelado).
    // O SUM é Option por semântica SQL; com linhas e colunas NOT NULL, sempre Some.
    let lucro: Option<Decimal> = sqlx::query_scalar(
        "SELECT SUM(quantity * unit_price - quantity * unit_cost_price)
         FROM sale_item WHERE store_id = $1::uuid AND sale_id = $2::uuid",
    )
    .bind(store.id)
    .bind(sale.id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        lucro.unwrap(),
        dec(598),
        "lucro usa o custo congelado (5.99-3.00)*2"
    );
}

/// Security Case (injeção): nenhum arquivo de `src/repositories/` pode montar
/// SQL por concatenação — só prepared statements com binds (`$1`, …).
/// Este teste falha no code review automatizado se alguém introduzir o padrão.
#[test]
fn repositorios_proibem_montagem_dinamica_de_sql() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/repositories");
    let mut arquivos = Vec::new();
    for entrada in std::fs::read_dir(&dir).unwrap() {
        let caminho = entrada.unwrap().path();
        if caminho.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let fonte = std::fs::read_to_string(&caminho).unwrap();
        // Padrão proibido pelo DoD: interpolação de valores no texto SQL.
        let proibido = "format!";
        assert!(
            !fonte.contains(proibido),
            "{} monta SQL dinamicamente — use query!/query_as! com binds",
            caminho.display()
        );
        // Todo SELECT/UPDATE/DELETE precisa filtrar por tenant…
        let tem_dml =
            fonte.contains("SELECT") || fonte.contains("UPDATE") || fonte.contains("DELETE");
        // …exceto o repositório raiz (a loja É o tenant).
        let e_raiz = caminho.file_stem().and_then(|s| s.to_str()) == Some("store_settings");
        if tem_dml && !e_raiz {
            assert!(
                fonte.contains("store_id"),
                "{} faz DML sem store_id",
                caminho.display()
            );
        }
        arquivos.push(caminho);
    }
    assert!(
        arquivos.len() >= 11,
        "repositórios incompletos: {arquivos:?}"
    );
}
