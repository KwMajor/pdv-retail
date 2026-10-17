//! Fechamento de venda com pagamento fracionado (US07 Task 7.1).
//!
//! Autoridade dos fatos: o payload diz O QUE vender e COMO pagar; preço,
//! custo e snapshot fiscal vêm SEMPRE do `PRODUCT` atual, lidos dentro da
//! transação (aceitar preço do cliente abriria fraude de balcão).
//!
//! Invariantes (tudo-ou-nada numa transação única):
//! - soma exata dos `amount` dos pagamentos == total líquido (senão 422);
//! - dinheiro: `tendered_amount` obrigatório e ≥ `amount` (troco = Σ sobras);
//! - demais métodos: sem troco (`tendered_amount` ausente ou ≤ `amount`);
//! - baixa de estoque no mesmo commit (movimento `SALE` + saldo atômico),
//!   respeitando `allow_negative_stock` (como o ajuste manual da US04);
//! - `SALE_ITEM` congela o Tax Snapshot (ncm/cest/cfop/origem/ICMS/custo):
//!   mudanças futuras no catálogo não corrompem a venda passada.

use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::{Payment, Product, Sale, SaleItem};
use crate::money::canonical_money;
use crate::repositories::{
    CustomerRepository, NewPayment, NewSale, NewSaleItem, NewStockMovement, PaymentRepository,
    PgCustomerRepository, PgPaymentRepository, PgProductRepository, PgSaleItemRepository,
    PgSaleRepository, PgStockMovementRepository, PgStockRepository, ProductRepository,
    SaleItemRepository, SaleRepository, StockMovementRepository, StockRepository,
};

/// Métodos de pagamento aceitos no fechamento (US07).
/// `crediario` fica fora: venda a prazo é o fluxo da US18 (dívida com
/// `pending`), não um pagamento instantâneo — aceitar aqui fecharia a venda
/// como paga sem registrar a dívida.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum PaymentMethod {
    Cash,
    Pix,
    Credit,
    Debit,
}

impl PaymentMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            PaymentMethod::Cash => "cash",
            PaymentMethod::Pix => "pix",
            PaymentMethod::Credit => "credit",
            PaymentMethod::Debit => "debit",
        }
    }
}

pub struct SaleItemInput {
    pub product_id: Uuid,
    pub quantity: Decimal,
    /// Desconto da linha em dinheiro (0 ≤ d ≤ bruto da linha).
    pub discount: Decimal,
}

pub struct SalePaymentInput {
    pub method: PaymentMethod,
    pub amount: Decimal,
    pub tendered_amount: Option<Decimal>,
}

pub struct CreateSaleInput {
    pub customer_id: Option<Uuid>,
    /// "CPF na nota" sem cadastro (11 dígitos) ou CNPJ (14).
    pub anonymous_cpf_cnpj: Option<String>,
    pub items: Vec<SaleItemInput>,
    pub payments: Vec<SalePaymentInput>,
}

/// Venda persistida + filhos (para a resposta da API).
pub struct SaleResult {
    pub sale: Sale,
    pub items: Vec<SaleItem>,
    pub payments: Vec<Payment>,
}

#[derive(Debug, thiserror::Error)]
pub enum SaleError {
    #[error("{0}")]
    Invalid(String),
    #[error("cliente não encontrado")]
    CustomerNotFound,
    #[error("produto não encontrado")]
    ProductNotFound,
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
}

fn money(value: Decimal, field: &str) -> Result<Decimal, SaleError> {
    canonical_money(value, field).map_err(SaleError::Invalid)
}

fn positive_money(value: Decimal, field: &str) -> Result<Decimal, SaleError> {
    let value = money(value, field)?;
    if value == Decimal::ZERO {
        return Err(SaleError::Invalid(format!(
            "{field} deve ser maior que zero"
        )));
    }
    Ok(value)
}

/// Quantidade de venda: > 0, até 3 casas, abaixo de 10^9 (como o ledger).
fn quantity(value: Decimal) -> Result<Decimal, SaleError> {
    if value <= Decimal::ZERO {
        return Err(SaleError::Invalid(
            "quantity deve ser decimal > 0 com até 3 casas".to_string(),
        ));
    }
    if value.scale() > 3 {
        return Err(SaleError::Invalid(
            "quantity deve ser decimal > 0 com até 3 casas".to_string(),
        ));
    }
    if value.abs() >= Decimal::new(1_000_000_000_000, 3) {
        return Err(SaleError::Invalid(
            "quantity fora da faixa suportada".to_string(),
        ));
    }
    Ok(value)
}

/// CPF (11) ou CNPJ (14) anônimo: só dígitos. Validação de DV fica nas USs
/// fiscais (US09/US16); aqui barra lixo e tamanho errado.
fn anonymous_cpf(raw: &Option<String>) -> Result<Option<String>, SaleError> {
    match raw {
        None => Ok(None),
        Some(value) => {
            let digits = value.trim().to_string();
            if ![11, 14].contains(&digits.len()) || !digits.bytes().all(|b| b.is_ascii_digit()) {
                return Err(SaleError::Invalid(
                    "cpf/cnpj deve ter 11 ou 14 dígitos numéricos".to_string(),
                ));
            }
            Ok(Some(digits))
        }
    }
}

/// Multiplicação de quantidade fracionada gera mais de 2 casas (ex:
/// 10.50 × 1.5 = 15.750): linha e totais são arredondados para centavos com
/// half-away-from-zero (regra comercial). Entradas do cliente NUNCA são
/// arredondadas — essas passam por `canonical_money` (reject-not-round).
fn to_cents(value: Decimal) -> Decimal {
    value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero)
}

/// Fecha a venda: valida, congela o fiscal, persiste e baixa o estoque.
pub async fn create_sale(
    pool: &sqlx::PgPool,
    store_id: Uuid,
    created_by: Uuid,
    input: CreateSaleInput,
) -> Result<SaleResult, SaleError> {
    if input.items.is_empty() {
        return Err(SaleError::Invalid(
            "venda precisa de ao menos um item".to_string(),
        ));
    }
    if input.payments.is_empty() {
        return Err(SaleError::Invalid(
            "venda precisa de ao menos um pagamento".to_string(),
        ));
    }
    // Cliente cadastrado (se houver): existe, é da loja e está ativo.
    if let Some(customer_id) = input.customer_id {
        let customer = PgCustomerRepository::new(pool.clone())
            .find_by_id(store_id, customer_id)
            .await?;
        match customer {
            None => return Err(SaleError::CustomerNotFound),
            Some(c) if !c.is_active => {
                return Err(SaleError::Invalid(
                    "cliente está inativo e não pode receber venda".to_string(),
                ));
            }
            Some(_) => {}
        }
    }
    let cpf = anonymous_cpf(&input.anonymous_cpf_cnpj)?;

    let mut tx = pool.begin().await.map_err(SaleError::Db)?;
    let products = PgProductRepository::new(pool.clone());

    struct Line {
        product: Product,
        quantity: Decimal,
        discount: Decimal,
        total: Decimal,
    }

    let mut lines = Vec::with_capacity(input.items.len());
    let mut subtotal = Decimal::ZERO;
    let mut sale_discount = Decimal::ZERO;
    for item in &input.items {
        let qty = quantity(item.quantity)?;
        let product = products
            .find_by_id_tx(&mut tx, store_id, item.product_id)
            .await?
            .ok_or(SaleError::ProductNotFound)?;
        if !product.is_active {
            return Err(SaleError::Invalid(format!(
                "produto {} está inativo",
                product.sku
            )));
        }
        // Custo também é congelado (lucro bruto futuro — US17).
        let gross = money(to_cents(product.price * qty), "total do item")?;
        let discount = money(item.discount, "discount")?;
        if discount > gross {
            return Err(SaleError::Invalid(format!(
                "desconto maior que o bruto da linha do produto {}",
                product.sku
            )));
        }
        subtotal = money(subtotal + gross, "subtotal")?;
        sale_discount = money(sale_discount + discount, "discount")?;
        lines.push(Line {
            product,
            quantity: qty,
            discount,
            total: gross - discount,
        });
    }
    let total = subtotal - sale_discount;

    // Pagamentos: soma exata + regras de troco (fraude de troco em PIX = 422).
    let mut paid = Decimal::ZERO;
    let mut change = Decimal::ZERO;
    for payment in &input.payments {
        let amount = positive_money(payment.amount, "amount")?;
        match payment.method {
            PaymentMethod::Cash => {
                let raw = payment.tendered_amount.ok_or_else(|| {
                    SaleError::Invalid("pagamento em dinheiro exige tendered_amount".to_string())
                })?;
                let tendered = money(raw, "tendered_amount")?;
                if tendered < amount {
                    return Err(SaleError::Invalid(
                        "tendered_amount não pode ser menor que amount no dinheiro".to_string(),
                    ));
                }
                change = money(change + (tendered - amount), "change_amount")?;
            }
            _ => {
                if let Some(raw) = payment.tendered_amount {
                    let tendered = money(raw, "tendered_amount")?;
                    if tendered > amount {
                        return Err(SaleError::Invalid(
                            "tendered_amount não pode ser maior que amount fora do dinheiro"
                                .to_string(),
                        ));
                    }
                }
            }
        }
        paid = money(paid + amount, "payments")?;
    }
    if paid != total {
        return Err(SaleError::Invalid(format!(
            "soma dos pagamentos ({paid}) difere do total da venda ({total})"
        )));
    }

    let sales = PgSaleRepository::new(pool.clone());
    let sale = sales
        .create_tx(
            &mut tx,
            NewSale {
                store_id,
                customer_id: input.customer_id,
                anonymous_cpf_cnpj: cpf,
                status: "closed".to_string(),
                subtotal,
                discount: sale_discount,
                total,
                change_amount: change,
                created_by: Some(created_by),
            },
        )
        .await?;

    let sale_items = PgSaleItemRepository::new(pool.clone());
    let mut items_out = Vec::with_capacity(lines.len());
    for line in &lines {
        items_out.push(
            sale_items
                .create_tx(
                    &mut tx,
                    NewSaleItem {
                        store_id,
                        sale_id: sale.id,
                        product_id: line.product.id,
                        quantity: line.quantity,
                        unit_price: line.product.price,
                        unit_cost_price: line.product.cost,
                        discount: line.discount,
                        total: line.total,
                        ncm_code: line.product.ncm.clone(),
                        cest: line.product.cest.clone(),
                        cfop: line.product.cfop.clone(),
                        icms_origin: line.product.icms_origin.clone(),
                        icms_rate: line.product.icms_rate,
                    },
                )
                .await?,
        );
    }

    let payments_repo = PgPaymentRepository::new(pool.clone());
    let mut payments_out = Vec::with_capacity(input.payments.len());
    for payment in &input.payments {
        payments_out.push(
            payments_repo
                .create_tx(
                    &mut tx,
                    NewPayment {
                        store_id,
                        sale_id: sale.id,
                        method: payment.method.as_str().to_string(),
                        amount: money(payment.amount, "amount")?,
                        tendered_amount: payment
                            .tendered_amount
                            .map(|t| money(t, "tendered_amount"))
                            .transpose()?,
                    },
                )
                .await?,
        );
    }

    // Baixa de estoque na MESMA transação (ledger + saldo atômico).
    let movements = PgStockMovementRepository::new(pool.clone());
    let stocks = PgStockRepository::new(pool.clone());
    for line in &lines {
        let delta = -line.quantity;
        movements
            .record_tx(
                &mut tx,
                NewStockMovement {
                    store_id,
                    product_id: line.product.id,
                    qty_delta: delta,
                    reason: "SALE".to_string(),
                    ref_sale_id: Some(sale.id),
                },
            )
            .await?;
        let saldo = stocks
            .add_tx(&mut tx, store_id, line.product.id, delta)
            .await?;
        if saldo.quantity < Decimal::ZERO && !line.product.allow_negative_stock {
            return Err(SaleError::Invalid(format!(
                "estoque insuficiente do produto {}",
                line.product.sku
            )));
        }
    }

    tx.commit().await.map_err(SaleError::Db)?;
    Ok(SaleResult {
        sale,
        items: items_out,
        payments: payments_out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(cents: i64) -> Decimal {
        Decimal::new(cents, 2)
    }

    #[test]
    fn method_serializa_minusculo() {
        assert_eq!(PaymentMethod::Cash.as_str(), "cash");
        assert_eq!(PaymentMethod::Pix.as_str(), "pix");
        assert_eq!(PaymentMethod::Credit.as_str(), "credit");
        assert_eq!(PaymentMethod::Debit.as_str(), "debit");
        let parsed: PaymentMethod = serde_json::from_str("\"pix\"").unwrap();
        assert_eq!(parsed, PaymentMethod::Pix);
        assert!(serde_json::from_str::<PaymentMethod>("\"crediario\"").is_err());
    }

    #[test]
    fn quantidade_rejeita_zero_negativo_e_escala() {
        assert!(quantity(Decimal::new(1, 0)).is_ok());
        assert!(quantity(Decimal::new(15, 1)).is_ok());
        assert!(quantity(Decimal::new(1, 3)).is_ok());
        assert!(quantity(Decimal::ZERO).is_err());
        assert!(quantity(Decimal::new(-1, 0)).is_err());
        assert!(quantity(Decimal::new(1, 4)).is_err());
        assert!(quantity(Decimal::new(1_000_000_000_000, 3)).is_err());
    }

    #[test]
    fn arredonda_linha_para_centavos() {
        // 10.50 × 1.5 = 15.750 → 15.75 (half away from zero).
        assert_eq!(to_cents(dec(1050) * Decimal::new(15, 1)), dec(1575));
        // 0.335 → 0.34; 0.325 → 0.33 (empate afasta do zero).
        assert_eq!(to_cents(Decimal::new(335, 3)), dec(34));
        assert_eq!(to_cents(Decimal::new(325, 3)), dec(33));
    }

    #[test]
    fn cpf_anonimo_aceita_11_ou_14_digitos() {
        assert!(anonymous_cpf(&None).unwrap().is_none());
        assert_eq!(
            anonymous_cpf(&Some(" 12345678901 ".into())).unwrap(),
            Some("12345678901".to_string())
        );
        assert_eq!(
            anonymous_cpf(&Some("12345678000199".into())).unwrap(),
            Some("12345678000199".to_string())
        );
        for invalido in ["", "123", "123456789012", "1234567890a", "123.456.789-01"] {
            assert!(anonymous_cpf(&Some(invalido.into())).is_err(), "{invalido}");
        }
    }

    #[test]
    fn dinheiro_positivo_rejeita_zero_negativo_e_escala() {
        assert!(positive_money(dec(100), "amount").is_ok());
        assert!(positive_money(Decimal::ZERO, "amount").is_err());
        assert!(positive_money(Decimal::new(-1, 2), "amount").is_err());
        assert!(positive_money(Decimal::new(10999, 3), "amount").is_err());
    }
}
