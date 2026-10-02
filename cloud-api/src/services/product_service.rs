//! Catálogo de produtos (US03 Task 3.1).
//!
//! Validação autoritária de todos os campos (tipos, tamanhos, padrões):
//! tipos, tamanhos, padrões e faixas — fora do domínio → `Invalid` (422).
//! NCM é obrigatório na criação (rigor SEFAZ); `store_id` sempre do token.

use rust_decimal::Decimal;
use uuid::Uuid;

use crate::models::Product;
use crate::repositories::{
    AuditLogRepository, NewAuditLog, NewProduct, PgAuditLogRepository, PgProductRepository,
    ProductPatch, ProductRepository,
};

/// Teto alinhado às colunas `VARCHAR` do banco.
const NAME_MAX: usize = 255;
const SKU_MAX: usize = 64;
/// Teto `DECIMAL(12,2)`: abaixo de 10^10 (escala ≤ 2).
static MONEY_MAX: std::sync::LazyLock<Decimal> =
    std::sync::LazyLock::new(|| Decimal::new(1_000_000_000_000, 2));

pub struct CreateProductInput {
    pub name: String,
    pub sku: String,
    pub barcode: Option<String>,
    pub price: Decimal,
    pub cost: Decimal,
    pub ncm: String,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Decimal,
}

#[derive(Debug, Default)]
pub struct UpdateProductInput {
    pub name: Option<String>,
    pub barcode: Option<String>,
    pub price: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub ncm: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    pub icms_rate: Option<Decimal>,
    pub allow_negative_stock: Option<bool>,
}

#[derive(Debug, thiserror::Error)]
pub enum ProductError {
    #[error("{0}")]
    Invalid(String),
    #[error("sku já cadastrado nesta loja")]
    SkuTaken,
    #[error("produto não encontrado")]
    NotFound,
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
}

fn digits(value: &str, len: usize) -> bool {
    value.len() == len && value.bytes().all(|b| b.is_ascii_digit())
}

fn money(value: Decimal, field: &str) -> Result<Decimal, ProductError> {
    if value < Decimal::ZERO || value >= *MONEY_MAX || value.scale() > 2 {
        return Err(ProductError::Invalid(format!(
            "{field} deve ser decimal ≥ 0 com até 2 casas"
        )));
    }
    Ok(value)
}

fn rate(value: Decimal, field: &str) -> Result<Decimal, ProductError> {
    if value < Decimal::ZERO || value > Decimal::from(100) || value.scale() > 2 {
        return Err(ProductError::Invalid(format!(
            "{field} deve ser decimal entre 0 e 100"
        )));
    }
    Ok(value)
}

fn name(name: &str) -> Result<String, ProductError> {
    let name = name.trim().to_string();
    if name.is_empty() {
        return Err(ProductError::Invalid("nome é obrigatório".to_string()));
    }
    if name.chars().count() > NAME_MAX {
        return Err(ProductError::Invalid(
            "nome deve ter no máximo 255 caracteres".to_string(),
        ));
    }
    if name.chars().any(|c| c.is_control()) {
        return Err(ProductError::Invalid(
            "nome contém caracteres de controle".to_string(),
        ));
    }
    Ok(name)
}

fn sku(sku: &str) -> Result<String, ProductError> {
    let sku = sku.trim().to_string();
    if sku.is_empty() {
        return Err(ProductError::Invalid("sku é obrigatório".to_string()));
    }
    if sku.chars().count() > SKU_MAX {
        return Err(ProductError::Invalid(
            "sku deve ter no máximo 64 caracteres".to_string(),
        ));
    }
    if !sku
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '/' | '.'))
    {
        return Err(ProductError::Invalid(
            "sku aceita só letras, números e - _ / .".to_string(),
        ));
    }
    Ok(sku)
}

fn barcode(barcode: &Option<String>) -> Result<Option<String>, ProductError> {
    match barcode {
        None => Ok(None),
        Some(raw) => {
            let code = raw.trim().to_string();
            if ![8, 12, 13, 14].contains(&code.len()) || !code.bytes().all(|b| b.is_ascii_digit()) {
                return Err(ProductError::Invalid(
                    "código de barras deve ter 8, 12, 13 ou 14 dígitos".to_string(),
                ));
            }
            Ok(Some(code))
        }
    }
}

fn ncm(ncm: &str) -> Result<String, ProductError> {
    let ncm = ncm.trim().to_string();
    if !digits(&ncm, 8) {
        return Err(ProductError::Invalid(
            "ncm deve ter exatamente 8 dígitos numéricos".to_string(),
        ));
    }
    Ok(ncm)
}

fn opt_digits(
    value: &Option<String>,
    len: usize,
    field: &str,
) -> Result<Option<String>, ProductError> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let code = raw.trim().to_string();
            if !digits(&code, len) {
                return Err(ProductError::Invalid(format!(
                    "{field} deve ter exatamente {len} dígitos numéricos"
                )));
            }
            Ok(Some(code))
        }
    }
}

fn icms_origin(value: &Option<String>) -> Result<Option<String>, ProductError> {
    match value {
        None => Ok(None),
        Some(raw) => {
            let code = raw.trim().to_string();
            if code.len() != 1
                || !matches!(
                    code.as_str(),
                    "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8"
                )
            {
                return Err(ProductError::Invalid(
                    "origem do ICMS deve ser um dígito de 0 a 8".to_string(),
                ));
            }
            Ok(Some(code))
        }
    }
}

pub async fn create_product(
    repo: &PgProductRepository,
    store_id: Uuid,
    input: CreateProductInput,
) -> Result<Product, ProductError> {
    let sku_value = sku(&input.sku)?;
    if repo.find_by_sku(store_id, &sku_value).await?.is_some() {
        return Err(ProductError::SkuTaken);
    }
    let created = repo
        .create(NewProduct {
            store_id,
            sku: sku_value,
            barcode: barcode(&input.barcode)?,
            name: name(&input.name)?,
            price: money(input.price, "price")?,
            cost: money(input.cost, "cost")?,
            ncm: Some(ncm(&input.ncm)?),
            cest: opt_digits(&input.cest, 7, "cest")?,
            cfop: opt_digits(&input.cfop, 4, "cfop")?,
            icms_origin: icms_origin(&input.icms_origin)?,
            icms_rate: rate(input.icms_rate, "icms_rate")?,
        })
        .await
        .map_err(|e| match &e {
            sqlx::Error::Database(db) if db.is_unique_violation() => ProductError::SkuTaken,
            _ => ProductError::Db(e),
        })?;
    Ok(created)
}

/// Atualização com auditoria de preço (US03 Task 3.2).
/// `actor` = `sub` do JWT (quem alterou, gravado no `AUDIT_LOG`).
/// Sem mudança de preço: update simples. Com mudança: transação atômica
/// (produto + log); qualquer falha — inclusive no log — faz rollback.
pub async fn update_product(
    pool: &sqlx::PgPool,
    store_id: Uuid,
    id: Uuid,
    input: UpdateProductInput,
    actor: Option<Uuid>,
) -> Result<Product, ProductError> {
    // Validação primeiro, fora de transação (não segura lock à toa).
    let patch = ProductPatch {
        name: input.name.map(|n| name(&n)).transpose()?,
        barcode: input
            .barcode
            .map(|b| barcode(&Some(b)))
            .transpose()?
            .flatten(),
        price: input.price.map(|v| money(v, "price")).transpose()?,
        cost: input.cost.map(|v| money(v, "cost")).transpose()?,
        ncm: input.ncm.map(|n| ncm(&n)).transpose()?,
        cest: input
            .cest
            .map(|c| opt_digits(&Some(c), 7, "cest"))
            .transpose()?
            .flatten(),
        cfop: input
            .cfop
            .map(|c| opt_digits(&Some(c), 4, "cfop"))
            .transpose()?
            .flatten(),
        icms_origin: input
            .icms_origin
            .map(|o| icms_origin(&Some(o)))
            .transpose()?
            .flatten(),
        icms_rate: input.icms_rate.map(|v| rate(v, "icms_rate")).transpose()?,
        allow_negative_stock: input.allow_negative_stock,
    };

    let products = PgProductRepository::new(pool.clone());
    let Some(new_price) = patch.price else {
        // Sem preço no patch: update simples (existe? senão NotFound).
        if products.find_by_id(store_id, id).await?.is_none() {
            return Err(ProductError::NotFound);
        }
        return products
            .update_details(store_id, id, patch)
            .await
            .map_err(ProductError::Db);
    };

    // Com preço: transação (leitura atual + update + log, tudo ou nada).
    let mut tx = pool.begin().await.map_err(ProductError::Db)?;
    let current = products
        .find_by_id_tx(&mut tx, store_id, id)
        .await?
        .ok_or(ProductError::NotFound)?;
    let updated = products
        .update_details_tx(&mut tx, store_id, id, patch)
        .await?;
    if current.price != new_price {
        PgAuditLogRepository::new(pool.clone())
            .record_tx(
                &mut tx,
                NewAuditLog {
                    store_id,
                    actor_user_id: actor,
                    action: "PRICE_CHANGE".to_string(),
                    entity: "product".to_string(),
                    entity_id: id.to_string(),
                    old_data: Some(serde_json::json!({"price": current.price})),
                    new_data: Some(serde_json::json!({"price": new_price})),
                },
            )
            .await?;
    }
    tx.commit().await.map_err(ProductError::Db)?;
    Ok(updated)
}

pub async fn get_product(
    repo: &PgProductRepository,
    store_id: Uuid,
    id: Uuid,
) -> Result<Product, ProductError> {
    repo.find_by_id(store_id, id)
        .await?
        .ok_or(ProductError::NotFound)
}

pub async fn list_products(
    repo: &PgProductRepository,
    store_id: Uuid,
    q: Option<&str>,
    limit: i64,
) -> Result<Vec<Product>, ProductError> {
    let limit = limit.clamp(1, 200);
    match q.map(str::trim).filter(|s| !s.is_empty()) {
        Some(term) => Ok(repo.search_active(store_id, term, limit).await?),
        None => Ok(repo.list_active(store_id, limit).await?),
    }
}

pub async fn deactivate_product(
    repo: &PgProductRepository,
    store_id: Uuid,
    id: Uuid,
) -> Result<Product, ProductError> {
    if repo.find_by_id(store_id, id).await?.is_none() {
        return Err(ProductError::NotFound);
    }
    repo.deactivate(store_id, id)
        .await
        .map_err(ProductError::Db)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn dec(cents: i64) -> Decimal {
        Decimal::new(cents, 2)
    }

    #[test]
    fn ncm_exige_8_digitos_numericos() {
        assert!(ncm("12345678").is_ok());
        for invalido in [
            "",
            "1234567",
            "123456789",
            "1234567a",
            " 12 345678 ",
            "abcdefgh",
        ] {
            assert!(ncm(invalido).is_err(), "{invalido}");
        }
    }

    #[test]
    fn barcode_aceita_gtin_valido() {
        assert!(barcode(&None).is_ok());
        for valido in [
            "78912340",
            "789123456789",
            "7891234567890",
            "17891234567890",
        ] {
            assert!(barcode(&Some(valido.into())).is_ok(), "{valido}");
        }
        for invalido in ["123", "123456789012345", "7891234a", "7891 2340"] {
            assert!(barcode(&Some(invalido.into())).is_err(), "{invalido}");
        }
        // Espaços nas bordas são aparados antes de validar.
        assert!(barcode(&Some("  78912340  ".into())).is_ok());
    }

    #[test]
    fn dinheiro_rejeita_negativo_escala_e_teto() {
        assert!(money(dec(1099), "price").is_ok());
        assert!(money(Decimal::ZERO, "price").is_ok());
        assert!(money(Decimal::new(-1, 2), "price").is_err());
        assert!(money(Decimal::new(1099, 3), "price").is_err());
        assert!(money(Decimal::new(10_000_000_000, 0), "price").is_err());
        assert!(rate(dec(1800), "icms_rate").is_ok());
        assert!(rate(Decimal::new(10001, 2), "icms_rate").is_err());
    }

    #[test]
    fn sku_bloqueia_simbolos() {
        assert!(sku("ABC-123_x.y/z").is_ok());
        let longo = "a".repeat(65);
        for invalido in ["", "com espaço", "com@arroba", "çã", longo.as_str()] {
            assert!(sku(invalido).is_err(), "{invalido}");
        }
    }
}
