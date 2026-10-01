//! Ajuste manual de estoque (US04 Task 4.2). Só gestão, tudo-ou-nada.

use axum::{Json, extract::State};
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::errors::AppError;
use crate::middleware::TenantContext;
use crate::models::Stock;
use crate::services::stock_service::{AdjustItem, MovementError, adjust_stock};

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
#[schema(example = json!({"items": [{"product_id": "11111111-1111-1111-1111-111111111111", "qty_delta": 50}], "reason": "NF 123 fornecedor"}))]
pub struct AdjustStockRequest {
    pub items: Vec<AdjustStockItem>,
    /// Motivo humano do ajuste (ex: "quebra de validade", "NF 123").
    pub reason: String,
}

#[derive(Debug, serde::Deserialize, utoipa::ToSchema)]
pub struct AdjustStockItem {
    #[schema(value_type = String)]
    pub product_id: Uuid,
    #[schema(value_type = String, example = "50")]
    pub qty_delta: Decimal,
}

/// Saldo resultante por item (na ordem do pedido).
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct StockResponse {
    #[schema(value_type = String)]
    pub product_id: Uuid,
    #[schema(value_type = String)]
    pub quantity: Decimal,
}

impl From<Stock> for StockResponse {
    fn from(s: Stock) -> Self {
        Self {
            product_id: s.product_id,
            quantity: s.quantity,
        }
    }
}

fn service_error(e: MovementError) -> AppError {
    match e {
        MovementError::Invalid(m) => AppError::Unprocessable(m),
        MovementError::ProductNotFound => AppError::NotFound,
        MovementError::Db(e) => AppError::Db(e),
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/stock/adjust",
    tag = "estoque",
    security(("bearer" = [])),
    request_body(content = AdjustStockRequest),
    responses(
        (status = 200, description = "Saldos resultantes (lote atômico)", body = [StockResponse]),
        (status = 401, description = "Sem Bearer válido", body = ErrorBody),
        (status = 403, description = "Só gestão ajusta estoque", body = ErrorBody),
        (status = 404, description = "Produto inexistente ou de outra loja", body = ErrorBody),
        (status = 422, description = "Lote vazio, delta inválido ou saldo negativo", body = ErrorBody),
    ),
)]
pub async fn adjust_stock_handler(
    ctx: TenantContext,
    State(state): State<crate::AppState>,
    Json(body): Json<AdjustStockRequest>,
) -> Result<Json<Vec<StockResponse>>, AppError> {
    ctx.require_manager()?;
    let pool = state
        .pool
        .clone()
        .ok_or_else(|| AppError::Internal("banco não configurado".to_string()))?;
    let saldos = adjust_stock(
        &pool,
        ctx.store_id,
        body.items
            .into_iter()
            .map(|i| AdjustItem {
                product_id: i.product_id,
                qty_delta: i.qty_delta,
            })
            .collect(),
        body.reason,
    )
    .await
    .map_err(service_error)?;
    Ok(Json(saldos.into_iter().map(StockResponse::from).collect()))
}
