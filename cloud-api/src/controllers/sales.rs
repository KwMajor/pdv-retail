//! Fechamento de venda com pagamento fracionado (US07 Task 7.1).
//!
//! Qualquer papel autenticado fecha venda (o caixa opera o PDV); o tenant
//! vem do JWT e o backend revalida preço, fiscal, matemática e estoque.

use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::services::sale_service::{
    CreateSaleInput, PaymentMethod, SaleError, SaleItemInput, SalePaymentInput, SaleResult,
    create_sale,
};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({
    "customer_id": null,
    "anonymous_cpf_cnpj": "12345678901",
    "items": [
        {"product_id": "11111111-1111-1111-1111-111111111111", "quantity": 2, "discount": "1.00"}
    ],
    "payments": [
        {"method": "cash", "amount": "15.00", "tendered_amount": "20.00"},
        {"method": "pix", "amount": "4.00"}
    ]
}))]
pub struct CreateSaleRequest {
    #[schema(value_type = Option<String>)]
    pub customer_id: Option<Uuid>,
    pub anonymous_cpf_cnpj: Option<String>,
    pub items: Vec<CreateSaleItemRequest>,
    pub payments: Vec<CreateSalePaymentRequest>,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct CreateSaleItemRequest {
    #[schema(value_type = String)]
    pub product_id: Uuid,
    #[schema(value_type = String, example = "2")]
    pub quantity: Decimal,
    /// Desconto da linha (default 0; nunca maior que o bruto da linha).
    #[schema(value_type = Option<String>, example = "1.00")]
    pub discount: Option<Decimal>,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct CreateSalePaymentRequest {
    pub method: PaymentMethod,
    #[schema(value_type = String, example = "15.00")]
    pub amount: Decimal,
    /// Obrigatório no dinheiro (≥ amount); proibido exceder `amount` nos demais.
    #[schema(value_type = Option<String>, example = "20.00")]
    pub tendered_amount: Option<Decimal>,
}

/// Venda fechada + snapshot fiscal dos itens + pagamentos registrados.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({
    "id": "33333333-3333-3333-3333-333333333333",
    "status": "closed",
    "subtotal": "20.00",
    "discount": "1.00",
    "total": "19.00",
    "change_amount": "5.00",
    "items": [{"product_id": "11111111-1111-1111-1111-111111111111", "quantity": "2", "unit_price": "10.00", "discount": "1.00", "total": "19.00", "ncm_code": "10063021"}],
    "payments": [{"method": "cash", "amount": "15.00", "tendered_amount": "20.00"}, {"method": "pix", "amount": "4.00", "tendered_amount": null}]
}))]
pub struct SaleResponse {
    #[schema(value_type = String)]
    pub id: Uuid,
    pub status: String,
    #[schema(value_type = String)]
    pub subtotal: Decimal,
    #[schema(value_type = String)]
    pub discount: Decimal,
    #[schema(value_type = String)]
    pub total: Decimal,
    /// Troco (Σ sobras do dinheiro).
    #[schema(value_type = String)]
    pub change_amount: Decimal,
    #[schema(value_type = String)]
    pub created_at: DateTime<Utc>,
    pub items: Vec<SaleItemResponse>,
    pub payments: Vec<SalePaymentResponse>,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SaleItemResponse {
    #[schema(value_type = String)]
    pub product_id: Uuid,
    #[schema(value_type = String)]
    pub quantity: Decimal,
    #[schema(value_type = String)]
    pub unit_price: Decimal,
    #[schema(value_type = String)]
    pub discount: Decimal,
    #[schema(value_type = String)]
    pub total: Decimal,
    pub ncm_code: Option<String>,
    pub cest: Option<String>,
    pub cfop: Option<String>,
    pub icms_origin: Option<String>,
    #[schema(value_type = String)]
    pub icms_rate: Decimal,
}

#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct SalePaymentResponse {
    pub method: String,
    #[schema(value_type = String)]
    pub amount: Decimal,
    #[schema(value_type = Option<String>)]
    pub tendered_amount: Option<Decimal>,
}

impl From<SaleResult> for SaleResponse {
    fn from(result: SaleResult) -> Self {
        Self {
            id: result.sale.id,
            status: result.sale.status,
            subtotal: cents(result.sale.subtotal),
            discount: cents(result.sale.discount),
            total: cents(result.sale.total),
            change_amount: cents(result.sale.change_amount),
            created_at: result.sale.created_at,
            items: result
                .items
                .into_iter()
                .map(|i| SaleItemResponse {
                    product_id: i.product_id,
                    quantity: i.quantity,
                    unit_price: cents(i.unit_price),
                    discount: cents(i.discount),
                    total: cents(i.total),
                    ncm_code: i.ncm_code,
                    cest: i.cest,
                    cfop: i.cfop,
                    icms_origin: i.icms_origin,
                    icms_rate: cents(i.icms_rate),
                })
                .collect(),
            payments: result
                .payments
                .into_iter()
                .map(|p| SalePaymentResponse {
                    method: p.method,
                    amount: cents(p.amount),
                    tendered_amount: p.tendered_amount.map(cents),
                })
                .collect(),
        }
    }
}

/// O NUMERIC do Postgres devolve zero sem escala (`0`); o contrato da API é
/// string com 2 casas (`"0.00"`) — normaliza todo dinheiro na saída.
fn cents(value: Decimal) -> Decimal {
    let mut value = value;
    value.rescale(2);
    value
}

fn service_error(e: SaleError) -> AppError {
    match e {
        SaleError::Invalid(m) => AppError::Unprocessable(m),
        SaleError::CustomerNotFound => AppError::NotFound,
        SaleError::ProductNotFound => AppError::NotFound,
        SaleError::Db(e) => AppError::Db(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/sales",
    tag = "vendas",
    security(("bearer" = [])),
    request_body(content = CreateSaleRequest),
    responses(
        (status = 201, description = "Sale closed with split payments (stock debited atomically)", body = SaleResponse),
        (status = 401, description = "Invalid or missing Bearer token", body = ErrorBody),
        (status = 404, description = "Missing customer or product from another store", body = ErrorBody),
        (status = 422, description = "Payment sum mismatch, cash change rules or field outside the domain", body = ErrorBody),
    ),
)]
pub async fn create_sale_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Json(body): Json<CreateSaleRequest>,
) -> Result<(StatusCode, Json<SaleResponse>), AppError> {
    let pool = state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let result = create_sale(
        &pool,
        ctx.store_id,
        ctx.user_id,
        CreateSaleInput {
            customer_id: body.customer_id,
            anonymous_cpf_cnpj: body.anonymous_cpf_cnpj,
            items: body
                .items
                .into_iter()
                .map(|i| SaleItemInput {
                    product_id: i.product_id,
                    quantity: i.quantity,
                    discount: i.discount.unwrap_or(Decimal::ZERO),
                })
                .collect(),
            payments: body
                .payments
                .into_iter()
                .map(|p| SalePaymentInput {
                    method: p.method,
                    amount: p.amount,
                    tendered_amount: p.tendered_amount,
                })
                .collect(),
        },
    )
    .await
    .map_err(service_error)?;
    Ok((StatusCode::CREATED, Json(SaleResponse::from(result))))
}
