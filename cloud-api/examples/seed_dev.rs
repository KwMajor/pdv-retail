//! Seed de desenvolvimento (TESTE MANUAL LOCAL).
//!
//! Cria, de forma idempotente: loja demo + gerente + catálogo de exemplo.
//! Uso: `cargo run --example seed_dev` (com o Postgres do compose no ar).
//!
//! NUNCA em produção: recusa `APP_ENV=production`. Credenciais fictícias.

use pdv_cloud_api::{
    models::UserRole,
    repositories::{NewStoreSettings, PgStoreSettingsRepository, StoreSettingsRepository},
    services::{
        CreateUserInput, create_user,
        product_service::{CreateProductInput, create_product},
    },
};
use rust_decimal::Decimal;
use sqlx::postgres::PgPoolOptions;

const DEMO_CNPJ: &str = "00.000.000/0001-91";
const DEMO_EMAIL: &str = "gerente@demo.exemplo";
const DEMO_PASSWORD: &str = "demo1234";

fn dec(cents: i64) -> Decimal {
    Decimal::new(cents, 2)
}

#[tokio::main]
async fn main() {
    let _ = dotenvy::dotenv();
    let app_env = std::env::var("APP_ENV").unwrap_or_default();
    if app_env == "production" {
        eprintln!("seed_dev: recusado com APP_ENV=production.");
        std::process::exit(1);
    }
    let url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://pdv:pdv@localhost:5432/pdv".to_string());
    let pool = PgPoolOptions::new()
        .max_connections(2)
        .connect(&url)
        .await
        .expect("postgres inacessível (suba com docker compose up -d)");

    // Loja demo (reaproveita se já existir).
    let stores = PgStoreSettingsRepository::new(pool.clone());
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT id::text FROM store_settings WHERE cnpj = $1",
    )
    .bind(DEMO_CNPJ)
    .fetch_optional(&pool)
    .await
    .expect("select loja");
    let store_id: uuid::Uuid = match existing {
        Some(id) => {
            println!("loja demo já existe: {id}");
            id.parse().expect("uuid da loja")
        }
        None => {
            let store = stores
                .create(NewStoreSettings { name: "Loja Demo".into(), cnpj: DEMO_CNPJ.into() })
                .await
                .expect("criar loja");
            println!("loja demo criada: {}", store.id);
            store.id
        }
    };

    // Gerente demo (reaproveita se já existir).
    let users = pdv_cloud_api::repositories::PgUserRepository::new(pool.clone());
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM \"user\" WHERE store_id = $1 AND email = $2)",
    )
    .bind(store_id)
    .bind(DEMO_EMAIL)
    .fetch_one(&pool)
    .await
    .expect("select usuário");
    if !exists {
        create_user(
            &users,
            store_id,
            CreateUserInput {
                name: "Gerente Demo".into(),
                email: DEMO_EMAIL.into(),
                password: DEMO_PASSWORD.into(),
                role: UserRole::Manager,
            },
        )
        .await
        .expect("criar gerente");
        println!("gerente demo criado: {DEMO_EMAIL}");
    } else {
        println!("gerente demo já existe: {DEMO_EMAIL}");
    }

    // Catálogo de exemplo (SKU fixa; ignora duplicado).
    let products = pdv_cloud_api::repositories::PgProductRepository::new(pool.clone());
    let catalogo = [
        ("Arroz T1 5kg", "ARROZ-T1-5KG", "7891234567890", 2799, 2150, "10063021"),
        ("Feijão Carioca 1kg", "FEIJAO-1KG", "7891234567891", 899, 620, "07133319"),
        ("Leite UHT 1L", "LEITE-UHT-1L", "7891234567892", 599, 430, "04012010"),
        ("Café Torrado 500g", "CAFE-500G", "7891234567893", 1899, 1340, "09012100"),
    ];
    for (name, sku, barcode, price, cost, ncm) in catalogo {
        let r = create_product(
            &products,
            store_id,
            CreateProductInput {
                name: name.into(),
                sku: sku.into(),
                barcode: Some(barcode.into()),
                price: dec(price),
                cost: dec(cost),
                ncm: ncm.into(),
                cest: None,
                cfop: Some("5102".into()),
                icms_origin: Some("0".into()),
                icms_rate: dec(1800),
            },
        )
        .await;
        match r {
            Ok(_) => println!("produto {sku} criado"),
            Err(_) => println!("produto {sku} já existe"),
        }
    }

    println!("\n=== login manual ===");
    println!("loja (UUID): {store_id}");
    println!("email:       {DEMO_EMAIL}");
    println!("senha:       {DEMO_PASSWORD}");
}
