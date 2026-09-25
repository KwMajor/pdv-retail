//! Item da venda (`sale_item`) com Tax Snapshot: cópia do fiscal vigente no
//! momento da venda — imune a mudanças futuras em `product`.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, sqlx::FromRow)]
pub struct SaleItem {
    pub id: Uuid,
    pub store_id: Uuid,
    pub sale_id: Uuid,
    pub product_id: Uuid,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    /// Snapshot de `product.cost` na venda (lucro bruto). 0.00 se isento.
    pub unit_cost_price: Decimal,
    pub discount: Decimal,
    pub total: Decimal,
    pub ncm_code: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Decimal,
}
