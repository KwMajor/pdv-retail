//! Repositório de `product`. Sem `DELETE` físico: só `deactivate`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::Product;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewProduct {
    pub store_id: Uuid,
    pub sku: String,
    pub barcode: Option<String>,
    pub name: String,
    pub price: Decimal,
    pub cost: Decimal,
    pub ncm: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Decimal,
}

pub trait ProductRepository {
    async fn create(&self, input: NewProduct) -> Result<Product, sqlx::Error>;
    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Product>, sqlx::Error>;
    async fn find_by_sku(
        &self,
        store_id: Uuid,
        sku: &str,
    ) -> Result<Option<Product>, sqlx::Error>;
    async fn find_by_barcode(
        &self,
        store_id: Uuid,
        barcode: &str,
    ) -> Result<Option<Product>, sqlx::Error>;
    async fn list_active(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Product>, sqlx::Error>;
    async fn set_price(
        &self,
        store_id: Uuid,
        id: Uuid,
        price: Decimal,
    ) -> Result<Product, sqlx::Error>;
    /// Soft delete (`is_active = FALSE`). Preserva FKs do histórico.
    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<Product, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgProductRepository {
    pool: sqlx::PgPool,
}

impl PgProductRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl ProductRepository for PgProductRepository {
    async fn create(&self, input: NewProduct) -> Result<Product, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "INSERT INTO product(store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at",
            input.store_id,
            input.sku,
            input.barcode,
            input.name,
            input.price,
            input.cost,
            input.ncm,
            input.cest,
            input.cfop,
            input.icms_origin,
            input.icms_rate,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "SELECT id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at
             FROM product WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_sku(
        &self,
        store_id: Uuid,
        sku: &str,
    ) -> Result<Option<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "SELECT id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at
             FROM product WHERE store_id = $1 AND sku = $2",
            store_id,
            sku,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn find_by_barcode(
        &self,
        store_id: Uuid,
        barcode: &str,
    ) -> Result<Option<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "SELECT id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at
             FROM product WHERE store_id = $1 AND barcode = $2",
            store_id,
            barcode,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_active(
        &self,
        store_id: Uuid,
        limit: i64,
    ) -> Result<Vec<Product>, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "SELECT id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at
             FROM product WHERE store_id = $1 AND is_active = TRUE
             ORDER BY name ASC LIMIT $2",
            store_id,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn set_price(
        &self,
        store_id: Uuid,
        id: Uuid,
        price: Decimal,
    ) -> Result<Product, sqlx::Error> {
        // Toda mudança de preço deve gerar `audit_log` na camada de serviço (US03).
        sqlx::query_as!(
            Product,
            "UPDATE product SET price = $3 WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at",
            store_id,
            id,
            price,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn deactivate(&self, store_id: Uuid, id: Uuid) -> Result<Product, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "UPDATE product SET is_active = FALSE WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at",
            store_id,
            id,
        )
        .fetch_one(&self.pool)
        .await
    }
}
