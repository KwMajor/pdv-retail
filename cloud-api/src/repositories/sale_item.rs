//! Repositório de `sale_item` (Tax Snapshot). Append-only: `create` + leituras.
//! A FK composta `(sale_id, store_id)` barra item cross-tenant no banco.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::SaleItem;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewSaleItem {
    pub store_id: Uuid,
    pub sale_id: Uuid,
    pub product_id: Uuid,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
    /// Snapshot fiscal copiado de `product` no momento da venda.
    pub ncm_code: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Decimal,
}

pub trait SaleItemRepository {
    async fn create(&self, input: NewSaleItem) -> Result<SaleItem, sqlx::Error>;
    async fn find_by_id(
        &self,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<SaleItem>, sqlx::Error>;
    async fn list_by_sale(
        &self,
        store_id: Uuid,
        sale_id: Uuid,
    ) -> Result<Vec<SaleItem>, sqlx::Error>;
}

#[derive(Debug, Clone)]
pub struct PgSaleItemRepository {
    pool: sqlx::PgPool,
}

impl PgSaleItemRepository {
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }
}

impl SaleItemRepository for PgSaleItemRepository {
    async fn create(&self, input: NewSaleItem) -> Result<SaleItem, sqlx::Error> {
        sqlx::query_as!(
            SaleItem,
            "INSERT INTO sale_item(store_id, sale_id, product_id, quantity, unit_price, discount, total, ncm_code, cest, cfop, icms_origin, icms_rate)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
             RETURNING id, store_id, sale_id, product_id, quantity, unit_price, discount, total, ncm_code, cest, cfop, icms_origin, icms_rate",
            input.store_id,
            input.sale_id,
            input.product_id,
            input.quantity,
            input.unit_price,
            input.discount,
            input.total,
            input.ncm_code,
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
    ) -> Result<Option<SaleItem>, sqlx::Error> {
        sqlx::query_as!(
            SaleItem,
            "SELECT id, store_id, sale_id, product_id, quantity, unit_price, discount, total, ncm_code, cest, cfop, icms_origin, icms_rate
             FROM sale_item WHERE store_id = $1 AND id = $2",
            store_id,
            id,
        )
        .fetch_optional(&self.pool)
        .await
    }

    async fn list_by_sale(
        &self,
        store_id: Uuid,
        sale_id: Uuid,
    ) -> Result<Vec<SaleItem>, sqlx::Error> {
        sqlx::query_as!(
            SaleItem,
            "SELECT id, store_id, sale_id, product_id, quantity, unit_price, discount, total, ncm_code, cest, cfop, icms_origin, icms_rate
             FROM sale_item WHERE store_id = $1 AND sale_id = $2",
            store_id,
            sale_id,
        )
        .fetch_all(&self.pool)
        .await
    }
}
