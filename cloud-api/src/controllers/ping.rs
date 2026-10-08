//! Rota de teste do isolamento (`GET /api/v1/ping`).
//!
//! Recebe o [`TenantContext`] por extractor: se a requisição chega aqui,
//! o tenant já foi validado. Retorna o `store_id` identificado.

use axum::Json;
use uuid::Uuid;

use crate::middleware::TenantContext;

/// Prova de isolamento: ecoa o tenant autenticado.
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
#[schema(example = json!({"status": "pong", "store_id": "22222222-2222-2222-2222-222222222222"}))]
pub struct PingResponse {
    pub status: String,
    #[schema(value_type = String)]
    pub store_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/api/v1/ping",
    tag = "isolamento",
    security(("bearer" = [])),
    responses(
        (status = 200, description = "Tenant identified (store_id echo)", body = PingResponse),
        (status = 401, description = "Missing Bearer, tampered/expired token or unknown role", body = ErrorBody),
    ),
)]
pub async fn ping(ctx: TenantContext) -> Json<PingResponse> {
    Json(PingResponse {
        status: "pong".to_string(),
        store_id: ctx.store_id,
    })
}
