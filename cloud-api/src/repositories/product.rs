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

/// Patch parcial (US03 Task 3.1): `None` mantém o valor atual (COALESCE).
/// Preço passa por aqui, mas a auditoria vive no service (Task 3.2).
#[derive(Debug, Clone, Default)]
pub struct ProductPatch {
    pub name: Option<String>,
    pub barcode: Option<String>,
    pub price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub ncm: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Option<Decimal>,
}

pub trait ProductRepository {
    async fn create(&self, input: NewProduct) -> Result<Product, sqlx::Error>;
    /// Leitura dentro de transação (US03 Task 3.2: read-modify-write atômico).
    async fn find_by_id_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        store_id: Uuid,
        id: Uuid,
    ) -> Result<Option<Product>, sqlx::Error>;
    async fn update_details(
        &self,
        store_id: Uuid,
        id: Uuid,
        patch: ProductPatch,
    ) -> Result<Product, sqlx::Error>;
    /// Mesma atualização dentro de transação (auditoria de preço).
    async fn update_details_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        store_id: Uuid,
        id: Uuid,
        patch: ProductPatch,
    ) -> Result<Product, sqlx::Error>;
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
    /// Busca textual em nome/sku/barcode (US06: achar produto no bip).
    /// Sempre isolada por `store_id` + só ativos.
    async fn search_active(
        &self,
        store_id: Uuid,
        term: &str,
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
    async fn find_by_id_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
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
        .fetch_optional(&mut **tx)
        .await
    }

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

    async fn update_details(
        &self,
        store_id: Uuid,
        id: Uuid,
        patch: ProductPatch,
    ) -> Result<Product, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "UPDATE product SET
               name = COALESCE($3, name),
               barcode = COALESCE($4, barcode),
               price = COALESCE($5, price),
               cost = COALESCE($6, cost),
               ncm = COALESCE($7, ncm),
               cest = COALESCE($8, cest),
               cfop = COALESCE($9, cfop),
               icms_origin = COALESCE($10, icms_origin),
               icms_rate = COALESCE($11, icms_rate)
             WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at",
            store_id,
            id,
            patch.name,
            patch.barcode,
            patch.price,
            patch.cost,
            patch.ncm,
            patch.cest,
            patch.cfop,
            patch.icms_origin,
            patch.icms_rate,
        )
        .fetch_one(&self.pool)
        .await
    }

    async fn search_active(
        &self,
        store_id: Uuid,
        term: &str,
        limit: i64,
    ) -> Result<Vec<Product>, sqlx::Error> {
        // Valor de bind (nunca texto SQL): montado por concatenação de
        // fatias, sem interpolação no texto da query (regra deste diretório).
        let like = ["%", term, "%"].concat();
        sqlx::query_as!(
            Product,
            "SELECT id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at
             FROM product
             WHERE store_id = $1 AND is_active = TRUE
               AND (name ILIKE $2 OR sku ILIKE $2 OR COALESCE(barcode, '') ILIKE $2)
             ORDER BY name ASC LIMIT $3",
            store_id,
            like,
            limit,
        )
        .fetch_all(&self.pool)
        .await
    }

    async fn update_details_tx(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        store_id: Uuid,
        id: Uuid,
        patch: ProductPatch,
    ) -> Result<Product, sqlx::Error> {
        sqlx::query_as!(
            Product,
            "UPDATE product SET
               name = COALESCE($3, name),
               barcode = COALESCE($4, barcode),
               price = COALESCE($5, price),
               cost = COALESCE($6, cost),
               ncm = COALESCE($7, ncm),
               cest = COALESCE($8, cest),
               cfop = COALESCE($9, cfop),
               icms_origin = COALESCE($10, icms_origin),
               icms_rate = COALESCE($11, icms_rate)
             WHERE store_id = $1 AND id = $2
             RETURNING id, store_id, sku, barcode, name, price, cost, ncm, cest, cfop, icms_origin, icms_rate, is_active, created_at, updated_at",
            store_id,
            id,
            patch.name,
            patch.barcode,
            patch.price,
            patch.cost,
            patch.ncm,
            patch.cest,
            patch.cfop,
            patch.icms_origin,
            patch.icms_rate,
        )
        .fetch_one(&mut **tx)
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
