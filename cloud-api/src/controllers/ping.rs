//! Rota de teste do isolamento (`GET /api/v1/ping`).
//!
//! Recebe o [`TenantContext`] por extractor: se a requisição chega aqui,
//! o tenant já foi validado. Retorna o `store_id` identificado.

use axum::Json;
use serde_json::{Value, json};

use crate::middleware::TenantContext;

pub async fn ping(ctx: TenantContext) -> Json<Value> {
    Json(json!({"status": "pong", "store_id": ctx.store_id}))
}
