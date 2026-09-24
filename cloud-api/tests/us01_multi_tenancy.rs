//! US01 — Arquitetura Multi-Tenant: todos os caminhos, não só happy path.
//!
//! Cobre: store_id NOT NULL em todas as tabelas operacionais, isolamento por
//! loja (incluindo unicidades por loja), CHECKs, defaults, soft-delete,
//! snapshot fiscal desacoplado, imutabilidade do ledger/auditoria, outbox fiscal
//! e os caminhos negativos (cada constraint violada de propósito).
//!
//! Banco dedicado: `TEST_DATABASE_URL` (default `.../pdv_test`). O setup cria
//! o banco e aplica `./migrations` sozinho; cada teste roda em transação com
//! rollback (exceto os marcados `pool`, que limpam explicitamente).
//! Requer Postgres no ar: `docker compose up -d`.

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::OnceCell;

// --- infra de teste ---------------------------------------------------------

static SEQ: AtomicU64 = AtomicU64::new(1);

/// Sufixo único por chamada E por execução do processo: testes rodam em
/// paralelo e tags sequenciais puras poderiam reaparecer numa próxima run e
/// colidir com linhas órfãs deixadas por um teste que abortou no meio.
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

/// Pool NOVO por teste, criado no runtime do próprio teste: nenhuma conexão
/// atravessa runtimes (sockets morrem com o runtime que os abriu — foi a
/// causa dos flakes "Tokio context is being shutdown" e dos hangs).
/// O pool é descartado no fim do teste, após o rollback da transação.
async fn fresh_pool() -> PgPool {
    ensure_setup().await;
    PgPoolOptions::new()
        .max_connections(2)
        // Fail-fast: prefere falhar em 10s a pendurar a suite em cascata.
        .acquire_timeout(Duration::from_secs(10))
        .connect_lazy(&test_db_url())
        .expect("TEST_DATABASE_URL inválida")
}

/// Cria o banco de teste e aplica as migrations exatamente uma vez, no
/// runtime de quem chegar primeiro. Conexões abertas aqui morrem com esse
/// runtime — por isso o pool compartilhado acima é lazy.
async fn ensure_setup() {
    static DONE: OnceCell<()> = OnceCell::const_new();
    DONE.get_or_init(|| async {
        let url = test_db_url();
        let (maint_url, db_name) = split_db_url(&url);
        // Pool local: usado só durante o setup, descartado no mesmo runtime.
        let admin = PgPoolOptions::new()
            .max_connections(1)
            .acquire_timeout(Duration::from_secs(10))
            .connect(&maint_url)
            .await
            .expect("postgres de teste inacessível — suba com: docker compose up -d");
        // Ignora erro se o banco já existir (ex: criado por outra run).
        let _ = sqlx::query(&format!("CREATE DATABASE \"{db_name}\""))
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

fn split_db_url(url: &str) -> (String, String) {
    let (base, db) = url.rsplit_once('/').expect("TEST_DATABASE_URL sem /banco");
    assert!(
        !db.is_empty() && db.chars().all(|c| c.is_alphanumeric() || c == '_'),
        "nome do banco de teste inseguro para CREATE DATABASE"
    );
    (format!("{base}/postgres"), db.to_string())
}

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

/// Savepoint com nome único: permite vários casos negativos no mesmo teste.
/// (Erro no Postgres aborta até o savepoint; o `ROLLBACK TO` recupera.)
async fn savepoint(tx: &mut Tx<'_>) -> String {
    let name = format!("sp{}", SEQ.fetch_add(1, Ordering::SeqCst));
    sqlx::query(&format!("SAVEPOINT {name}"))
        .execute(&mut **tx)
        .await
        .unwrap();
    name
}

async fn rollback_to(tx: &mut Tx<'_>, name: &str) {
    sqlx::query(&format!("ROLLBACK TO SAVEPOINT {name}"))
        .execute(&mut **tx)
        .await
        .unwrap();
}

// --- factories --------------------------------------------------------------

async fn mk_store(tx: &mut Tx<'_>, tag: &str) -> String {
    sqlx::query_scalar("INSERT INTO store(name, cnpj) VALUES ($1, $2) RETURNING id::text")
        .bind(format!("Loja {tag}"))
        .bind(format!("cnpj-{tag}"))
        .fetch_one(&mut **tx)
        .await
        .unwrap()
}

async fn mk_user(tx: &mut Tx<'_>, store: &str, email: &str, role: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO \"user\"(store_id, name, email, password_hash, role) VALUES ($1::uuid, 'Func', $2, 'hash', $3) RETURNING id::text",
    )
    .bind(store)
    .bind(email)
    .bind(role)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

async fn mk_product(tx: &mut Tx<'_>, store: &str, sku: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO product(store_id, sku, name, price) VALUES ($1::uuid, $2, 'Prod', 10.00) RETURNING id::text",
    )
    .bind(store)
    .bind(sku)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

async fn mk_customer(tx: &mut Tx<'_>, store: &str) -> String {
    let tag = uniq("cust");
    sqlx::query_scalar(
        "INSERT INTO customer(store_id, name, cpf_cnpj) VALUES ($1::uuid, $2, '12345678901') RETURNING id::text",
    )
    .bind(store)
    .bind(format!("Cliente {tag}"))
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

async fn mk_sale(tx: &mut Tx<'_>, store: &str) -> String {
    sqlx::query_scalar(
        "INSERT INTO sale(store_id, total) VALUES ($1::uuid, 10.00) RETURNING id::text",
    )
    .bind(store)
    .fetch_one(&mut **tx)
    .await
    .unwrap()
}

// --- store ------------------------------------------------------------------

#[tokio::test]
async fn store_create_ok_com_timestamps() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let tag = uniq("s");
    let row: (String, bool, bool) = sqlx::query_as(
        "INSERT INTO store(name, cnpj) VALUES ($1, $2) RETURNING id::text, created_at IS NOT NULL, updated_at IS NOT NULL",
    )
    .bind(format!("Loja {tag}"))
    .bind(format!("cnpj-{tag}"))
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!row.0.is_empty());
    assert!(row.1 && row.2);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn store_cnpj_duplicado_falha() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let tag = uniq("dup");
    mk_store(&mut tx, &tag).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query("INSERT INTO store(name, cnpj) VALUES ('Outra', $1)")
        .bind(format!("cnpj-{tag}"))
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("unique") || err.to_string().contains("duplicate"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn store_nome_nulo_falha() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let err = sqlx::query("INSERT INTO store(name, cnpj) VALUES (NULL, 'x')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("null"));
    tx.rollback().await.unwrap();
}

// --- user -------------------------------------------------------------------

#[tokio::test]
async fn user_exige_store_id() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let err = sqlx::query(
        "INSERT INTO \"user\"(store_id, name, email, password_hash, role) VALUES (NULL, 'F', 'e@x', 'h', 'cashier')",
    )
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("null"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn user_role_aceita_3_papeis_e_rejeita_outros() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("r")).await;
    for role in ["admin", "manager", "cashier"] {
        mk_user(&mut tx, &store, &uniq("role"), role).await;
    }
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO \"user\"(store_id, name, email, password_hash, role) VALUES ($1::uuid, 'F', $2, 'h', 'dono')",
    )
    .bind(&store)
    .bind(uniq("role-bad"))
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn user_email_unico_por_loja_mas_reuso_entre_lojas() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let a = mk_store(&mut tx, &uniq("ea")).await;
    let b = mk_store(&mut tx, &uniq("eb")).await;
    let email = uniq("func@loja");
    mk_user(&mut tx, &a, &email, "cashier").await;
    // Mesma loja: falha.
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO \"user\"(store_id, name, email, password_hash, role) VALUES ($1::uuid, 'F2', $2, 'h', 'cashier')",
    )
    .bind(&a)
    .bind(&email)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("duplicate") || err.to_string().contains("unique"));
    rollback_to(&mut tx, &sp).await;
    // Outra loja, mesmo email: ok (multi-tenant de verdade).
    mk_user(&mut tx, &b, &email, "cashier").await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn user_soft_delete_preserva_linha() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("sd")).await;
    let email = uniq("sd@loja");
    let id = mk_user(&mut tx, &store, &email, "cashier").await;
    let active: bool = sqlx::query_scalar("SELECT is_active FROM \"user\" WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert!(active);
    sqlx::query("UPDATE \"user\" SET is_active = FALSE WHERE id = $1::uuid")
        .bind(&id)
        .execute(&mut *tx)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM \"user\" WHERE id = $1::uuid")
        .bind(&id)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "soft-delete não pode remover a linha (FKs do histórico)"
    );
    tx.rollback().await.unwrap();
}

// --- product ----------------------------------------------------------------

#[tokio::test]
async fn product_exige_store_id() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let err =
        sqlx::query("INSERT INTO product(store_id, sku, name, price) VALUES (NULL, 's', 'P', 1)")
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert!(err.to_string().contains("null"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn product_sku_unico_por_loja_mas_reuso_entre_lojas() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let a = mk_store(&mut tx, &uniq("sa")).await;
    let b = mk_store(&mut tx, &uniq("sb")).await;
    let sku = uniq("SKU");
    mk_product(&mut tx, &a, &sku).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO product(store_id, sku, name, price) VALUES ($1::uuid, $2, 'P2', 2)",
    )
    .bind(&a)
    .bind(&sku)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("duplicate") || err.to_string().contains("unique"));
    rollback_to(&mut tx, &sp).await;
    mk_product(&mut tx, &b, &sku).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn product_preco_e_custo_nao_negativos() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("mn")).await;
    for (col, val) in [("price", "-1"), ("cost", "-0.01")] {
        let sp = savepoint(&mut tx).await;
        let err = sqlx::query(&format!(
            "INSERT INTO product(store_id, sku, name, {col}) VALUES ($1::uuid, $2, 'P', {val})"
        ))
        .bind(&store)
        .bind(uniq("neg"))
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("check") || err.to_string().contains("violates"),
            "{col}"
        );
        rollback_to(&mut tx, &sp).await;
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn product_barcode_nulo_pode_repetir() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("bc")).await;
    mk_product(&mut tx, &store, &uniq("b1")).await;
    mk_product(&mut tx, &store, &uniq("b2")).await; // ambos barcode NULL: ok
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM product WHERE store_id = $1::uuid")
        .bind(&store)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 2);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn product_updated_at_atualiza_no_update() {
    // Pool próprio (fora de tx rollback): precisa de 2 transações p/ NOW() mudar.
    let pool = fresh_pool().await;
    let tag = uniq("ts");
    let store: String =
        sqlx::query_scalar("INSERT INTO store(name, cnpj) VALUES ($1, $2) RETURNING id::text")
            .bind(format!("Loja {tag}"))
            .bind(format!("cnpj-{tag}"))
            .fetch_one(&pool)
            .await
            .unwrap();
    let prod: String = sqlx::query_scalar(
        "INSERT INTO product(store_id, sku, name, price) VALUES ($1::uuid, $2, 'P', 1) RETURNING id::text",
    )
    .bind(&store)
    .bind(format!("sku-{tag}"))
    .fetch_one(&pool)
    .await
    .unwrap();
    let before: f64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM updated_at)::float8 FROM product WHERE id = $1::uuid",
    )
    .bind(&prod)
    .fetch_one(&pool)
    .await
    .unwrap();
    sqlx::query("SELECT pg_sleep(0.05)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE product SET name = 'P2' WHERE id = $1::uuid")
        .bind(&prod)
        .execute(&pool)
        .await
        .unwrap();
    let after: f64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM updated_at)::float8 FROM product WHERE id = $1::uuid",
    )
    .bind(&prod)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(after > before, "trigger set_updated_at não disparou");
    sqlx::query("DELETE FROM product WHERE id = $1::uuid")
        .bind(&prod)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM store WHERE id = $1::uuid")
        .bind(&store)
        .execute(&pool)
        .await
        .unwrap();
}

// --- customer ---------------------------------------------------------------

#[tokio::test]
async fn customer_exige_store_id() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let err = sqlx::query("INSERT INTO customer(store_id, name) VALUES (NULL, 'C')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("null"));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn customer_pj_roundtrip_razao_e_ie() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("pj")).await;
    let tag = uniq("pj");
    let id: String = sqlx::query_scalar(
        "INSERT INTO customer(store_id, name, cpf_cnpj, corporate_name, state_registration) VALUES ($1::uuid, $2, $3, $4, $5) RETURNING id::text",
    )
    .bind(&store)
    .bind(format!("Mercado {tag}"))
    .bind("11222333000181")
    .bind(format!("Mercado {tag} LTDA"))
    .bind("123456789")
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let row: (String, String, String) = sqlx::query_as(
        "SELECT cpf_cnpj, corporate_name, state_registration FROM customer WHERE id = $1::uuid",
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "11222333000181".into(),
            format!("Mercado {tag} LTDA"),
            "123456789".into()
        )
    );
    tx.rollback().await.unwrap();
}

// --- sale -------------------------------------------------------------------

#[tokio::test]
async fn sale_cpf_anonimo_valido_sem_cadastro() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("an")).await;
    let id: String = sqlx::query_scalar(
        "INSERT INTO sale(store_id, anonymous_cpf_cnpj, total) VALUES ($1::uuid, '12345678901', 5) RETURNING id::text",
    )
    .bind(&store)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert!(!id.is_empty());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_cpf_anonimo_curto_falha() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("anbad")).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO sale(store_id, anonymous_cpf_cnpj, total) VALUES ($1::uuid, '1234', 5)",
    )
    .bind(&store)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_sem_cliente_e_sem_cpf_ok() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap(); // venda balcão anônima total
    let store = mk_store(&mut tx, &uniq("balc")).await;
    mk_sale(&mut tx, &store).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_vinculada_a_cliente_cadastrado() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("cli")).await;
    let customer = mk_customer(&mut tx, &store).await;
    let sale: String = sqlx::query_scalar(
        "INSERT INTO sale(store_id, customer_id, total) VALUES ($1::uuid, $2::uuid, 30) RETURNING id::text",
    )
    .bind(&store)
    .bind(&customer)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    let back: String = sqlx::query_scalar("SELECT customer_id::text FROM sale WHERE id = $1::uuid")
        .bind(&sale)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(back, customer);
    // Cliente inexistente: FK barra.
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO sale(store_id, customer_id, total) VALUES ($1::uuid, gen_random_uuid(), 1)",
    )
    .bind(&store)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("foreign") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_status_aceita_4_e_rejeita_outros() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("st")).await;
    for status in ["open", "closed", "cancelled", "pending"] {
        sqlx::query("INSERT INTO sale(store_id, status, total) VALUES ($1::uuid, $2, 1)")
            .bind(&store)
            .bind(status)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let sp = savepoint(&mut tx).await;
    let err =
        sqlx::query("INSERT INTO sale(store_id, status, total) VALUES ($1::uuid, 'extraviada', 1)")
            .bind(&store)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_total_e_troco_nao_negativos() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("neg")).await;
    for (col, val) in [("total", "-1"), ("change_amount", "-0.01")] {
        let sp = savepoint(&mut tx).await;
        let err = sqlx::query(&format!(
            "INSERT INTO sale(store_id, {col}) VALUES ($1::uuid, {val})"
        ))
        .bind(&store)
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("check") || err.to_string().contains("violates"),
            "{col}"
        );
        rollback_to(&mut tx, &sp).await;
    }
    tx.rollback().await.unwrap();
}

// --- sale_item (snapshot fiscal) --------------------------------------------

#[tokio::test]
async fn sale_item_snapshot_desacoplado_do_produto() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("snap")).await;
    let prod = mk_product(&mut tx, &store, &uniq("snap")).await;
    sqlx::query(
        "UPDATE product SET ncm = '10011010', cfop = '5102', icms_rate = 18 WHERE id = $1::uuid",
    )
    .bind(&prod)
    .execute(&mut *tx)
    .await
    .unwrap();
    let sale = mk_sale(&mut tx, &store).await;
    // Item congela fiscal DIFERENTE do atual (ex: regra antiga): permitido.
    sqlx::query(
        "INSERT INTO sale_item(sale_id, product_id, quantity, unit_price, total, ncm_code, cfop, icms_rate) VALUES ($1::uuid, $2::uuid, 1, 10, 10, '99999999', '6102', 4)",
    )
    .bind(&sale)
    .bind(&prod)
    .execute(&mut *tx)
    .await
    .unwrap();
    // Mudança futura no produto NÃO contamina o item já gravado.
    sqlx::query("UPDATE product SET ncm = '00000000' WHERE id = $1::uuid")
        .bind(&prod)
        .execute(&mut *tx)
        .await
        .unwrap();
    let ncm: String = sqlx::query_scalar("SELECT ncm_code FROM sale_item WHERE sale_id = $1::uuid")
        .bind(&sale)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(ncm, "99999999");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn sale_item_quantidade_positiva() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("q")).await;
    let prod = mk_product(&mut tx, &store, &uniq("q")).await;
    let sale = mk_sale(&mut tx, &store).await;
    for qty in ["0", "-2"] {
        let sp = savepoint(&mut tx).await;
        let err = sqlx::query(&format!(
            "INSERT INTO sale_item(sale_id, product_id, quantity, unit_price, total) VALUES ($1::uuid, $2::uuid, {qty}, 10, 10)"
        ))
        .bind(&sale)
        .bind(&prod)
        .execute(&mut *tx)
        .await
        .unwrap_err();
        assert!(
            err.to_string().contains("check") || err.to_string().contains("violates"),
            "qty={qty}"
        );
        rollback_to(&mut tx, &sp).await;
    }
    tx.rollback().await.unwrap();
}

// --- payment ----------------------------------------------------------------

#[tokio::test]
async fn payment_metodos_validos_e_invalido() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("pm")).await;
    let sale = mk_sale(&mut tx, &store).await;
    for method in ["cash", "pix", "credit", "debit", "crediario"] {
        let tendered: Option<String> = if method == "cash" {
            Some("10.00".into())
        } else {
            None
        };
        sqlx::query("INSERT INTO payment(sale_id, method, amount, tendered_amount) VALUES ($1::uuid, $2, 10, $3::numeric)")
            .bind(&sale)
            .bind(method)
            .bind(tendered)
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO payment(sale_id, method, amount) VALUES ($1::uuid, 'bitcoin', 10)",
    )
    .bind(&sale)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn payment_dinheiro_regras_do_troco() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("cash")).await;
    let sale = mk_sale(&mut tx, &store).await;
    // Sem valor entregue: falha.
    let sp = savepoint(&mut tx).await;
    let err =
        sqlx::query("INSERT INTO payment(sale_id, method, amount) VALUES ($1::uuid, 'cash', 10)")
            .bind(&sale)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    // Entregue menor que a conta: falha.
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO payment(sale_id, method, amount, tendered_amount) VALUES ($1::uuid, 'cash', 10, 9.99)",
    )
    .bind(&sale)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    // Exato e com troco: ok.
    for tendered in ["10.00", "20.00"] {
        sqlx::query(
            "INSERT INTO payment(sale_id, method, amount, tendered_amount) VALUES ($1::uuid, 'cash', 10, $2::numeric)",
        )
        .bind(&sale)
        .bind(tendered)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn payment_valor_zero_falha_pix_sem_troco_ok() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("px")).await;
    let sale = mk_sale(&mut tx, &store).await;
    sqlx::query("INSERT INTO payment(sale_id, method, amount) VALUES ($1::uuid, 'pix', 10)")
        .bind(&sale)
        .execute(&mut *tx)
        .await
        .unwrap();
    let sp = savepoint(&mut tx).await;
    let err =
        sqlx::query("INSERT INTO payment(sale_id, method, amount) VALUES ($1::uuid, 'pix', 0)")
            .bind(&sale)
            .execute(&mut *tx)
            .await
            .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

// --- stock_movement (ledger imutável) ---------------------------------------

#[tokio::test]
async fn stock_exige_store_id() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let err = sqlx::query("INSERT INTO stock_movement(store_id, product_id, qty_delta, reason) VALUES (NULL, gen_random_uuid(), 1, 'x')")
        .execute(&mut *tx)
        .await
        .unwrap_err();
    let msg = err.to_string();
    assert!(msg.contains("null") || msg.contains("violates"), "{msg}");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn stock_delta_zero_falha() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("z")).await;
    let prod = mk_product(&mut tx, &store, &uniq("z")).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO stock_movement(store_id, product_id, qty_delta, reason) VALUES ($1::uuid, $2::uuid, 0, 'x')",
    )
    .bind(&store)
    .bind(&prod)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("check") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    // Entrada e saída passam.
    for delta in ["5", "-2"] {
        sqlx::query(
            "INSERT INTO stock_movement(store_id, product_id, qty_delta, reason) VALUES ($1::uuid, $2::uuid, $3::numeric, 'x')",
        )
        .bind(&store)
        .bind(&prod)
        .bind(delta)
        .execute(&mut *tx)
        .await
        .unwrap();
    }
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn stock_ledger_imutavel_update_e_delete_falham() {
    // Tudo numa transação com rollback: sem TRUNCATE (que travaria lock
    // exclusivo contra os demais testes paralelos) e sem deixar órfãos.
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let tag = uniq("imu");
    let store = mk_store(&mut tx, &tag).await;
    let prod = mk_product(&mut tx, &store, &tag).await;
    let reason = uniq("mov");
    sqlx::query(
        "INSERT INTO stock_movement(store_id, product_id, qty_delta, reason) VALUES ($1::uuid, $2::uuid, 1, $3)",
    )
    .bind(&store)
    .bind(&prod)
    .bind(&reason)
    .execute(&mut *tx)
    .await
    .unwrap();
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query("UPDATE stock_movement SET reason = 'adulterado' WHERE reason = $1")
        .bind(&reason)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("imutável"), "{err}");
    rollback_to(&mut tx, &sp).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query("DELETE FROM stock_movement WHERE reason = $1")
        .bind(&reason)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("imutável"), "{err}");
    rollback_to(&mut tx, &sp).await;
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stock_movement WHERE reason = $1")
        .bind(&reason)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(n, 1, "ledger não pode perder linhas");
    tx.rollback().await.unwrap();
}

// --- audit_log ---------------------------------------------------------------

#[tokio::test]
async fn audit_imutavel_update_e_delete_falham() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("aud")).await;
    let entity = uniq("ent");
    sqlx::query(
        "INSERT INTO audit_log(store_id, action, entity, entity_id, old_data, new_data) VALUES ($1::uuid, 'price.change', 'product', $2, '{\"price\": 10}', '{\"price\": 12}')",
    )
    .bind(&store)
    .bind(&entity)
    .execute(&mut *tx)
    .await
    .unwrap();
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query("UPDATE audit_log SET action = 'x' WHERE entity_id = $1")
        .bind(&entity)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("imutável"), "{err}");
    rollback_to(&mut tx, &sp).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query("DELETE FROM audit_log WHERE entity_id = $1")
        .bind(&entity)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("imutável"), "{err}");
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn audit_jsonb_roundtrip_valor_antigo_e_novo() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("js")).await;
    sqlx::query(
        "INSERT INTO audit_log(store_id, action, entity, entity_id, old_data, new_data) VALUES ($1::uuid, 'price.change', 'product', 'p9', '{\"price\": 10.5}', '{\"price\": 12.0}')",
    )
    .bind(&store)
    .execute(&mut *tx)
    .await
    .unwrap();
    let row: (serde_json::Value, serde_json::Value) =
        sqlx::query_as("SELECT old_data, new_data FROM audit_log WHERE entity_id = 'p9'")
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(row.0["price"], serde_json::json!(10.5));
    assert_eq!(row.1["price"], serde_json::json!(12.0));
    tx.rollback().await.unwrap();
}

// --- fiscal_queue (outbox) ----------------------------------------------------

#[tokio::test]
async fn fiscal_defaults_pending_e_tentativas_zero() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("fq")).await;
    let sale = mk_sale(&mut tx, &store).await;
    let row: (String, i32) = sqlx::query_as(
        "INSERT INTO fiscal_queue(store_id, sale_id) VALUES ($1::uuid, $2::uuid) RETURNING status, attempts",
    )
    .bind(&store)
    .bind(&sale)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(row, ("pending".into(), 0));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn fiscal_status_invalido_falha_e_fk_venda_exigida() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("fq2")).await;
    let sale = mk_sale(&mut tx, &store).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO fiscal_queue(store_id, sale_id, status) VALUES ($1::uuid, $2::uuid, 'voando')",
    )
    .bind(&store)
    .bind(&sale)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(
        err.to_string().contains("check") || err.to_string().contains("violates"),
        "status inválido deveria violar CHECK, obteve: {err}"
    );
    rollback_to(&mut tx, &sp).await;
    let sp = savepoint(&mut tx).await;
    let err = sqlx::query(
        "INSERT INTO fiscal_queue(store_id, sale_id) VALUES ($1::uuid, gen_random_uuid())",
    )
    .bind(&store)
    .execute(&mut *tx)
    .await
    .unwrap_err();
    assert!(err.to_string().contains("foreign") || err.to_string().contains("violates"));
    rollback_to(&mut tx, &sp).await;
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn fiscal_outbox_venda_e_fila_na_mesma_transacao() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let store = mk_store(&mut tx, &uniq("ob")).await;
    // Padrão outbox da contingência offline: sale + enqueue atômicos.
    let sale: String = sqlx::query_scalar(
        "INSERT INTO sale(store_id, total) VALUES ($1::uuid, 7.50) RETURNING id::text",
    )
    .bind(&store)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO fiscal_queue(store_id, sale_id) VALUES ($1::uuid, $2::uuid)")
        .bind(&store)
        .bind(&sale)
        .execute(&mut *tx)
        .await
        .unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM fiscal_queue WHERE sale_id = $1::uuid AND status = 'pending'",
    )
    .bind(&sale)
    .fetch_one(&mut *tx)
    .await
    .unwrap();
    assert_eq!(n, 1);
    tx.rollback().await.unwrap();
}

// --- isolamento (coração da US01) --------------------------------------------

#[tokio::test]
async fn isolamento_produtos_filtrados_por_loja() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    let a = mk_store(&mut tx, &uniq("la")).await;
    let b = mk_store(&mut tx, &uniq("lb")).await;
    let sku_a = uniq("ska");
    let sku_b = uniq("skb");
    mk_product(&mut tx, &a, &sku_a).await;
    mk_product(&mut tx, &b, &sku_b).await;
    let only_a: Vec<String> =
        sqlx::query_scalar("SELECT sku FROM product WHERE store_id = $1::uuid ORDER BY sku")
            .bind(&a)
            .fetch_all(&mut *tx)
            .await
            .unwrap();
    assert_eq!(only_a, vec![sku_a.clone()]);
    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM product WHERE store_id IN ($1::uuid, $2::uuid)")
            .bind(&a)
            .bind(&b)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
    assert_eq!(total, 2);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn isolamento_toda_tabela_operacional_tem_store_id_not_null() {
    let pool = fresh_pool().await;
    let mut tx = pool.begin().await.unwrap();
    for table in [
        "user",
        "product",
        "customer",
        "sale",
        "stock_movement",
        "audit_log",
        "fiscal_queue",
    ] {
        let nullable: String = sqlx::query_scalar(
            "SELECT is_nullable FROM information_schema.columns WHERE table_schema = 'public' AND table_name = $1 AND column_name = 'store_id'",
        )
        .bind(table)
        .fetch_one(&mut *tx)
        .await
        .unwrap_or_else(|_| panic!("tabela {table} sem coluna store_id"));
        assert_eq!(
            nullable, "NO",
            "tabela {table}: store_id precisa ser NOT NULL"
        );
    }
    tx.rollback().await.unwrap();
}
