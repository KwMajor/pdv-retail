use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;

use crate::AppState;

/// Corpo do `/health` 200 e do `/ready` 200.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(example = json!({"status": "ok"}))]
pub struct StatusResponse {
    pub status: String,
}

/// Corpo do `/ready` 503 (motivo em `db`, sem detalhe interno).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[schema(example = json!({"status": "degraded", "db": "unreachable"}))]
pub struct DegradedResponse {
    pub status: String,
    pub db: String,
}

/// Liveness: sempre 200 se o processo está de pé.
#[utoipa::path(
    get,
    path = "/health",
    tag = "sistema",
    responses(
        (status = 200, description = "Process alive", body = StatusResponse),
    ),
)]
pub async fn health() -> (StatusCode, Json<StatusResponse>) {
    (
        StatusCode::OK,
        Json(StatusResponse {
            status: "ok".to_string(),
        }),
    )
}

/// Readiness: 200 só com pool + `SELECT 1` ok; 503 caso contrário.
#[utoipa::path(
    get,
    path = "/ready",
    tag = "sistema",
    responses(
        (status = 200, description = "Database reachable", body = StatusResponse),
        (status = 503, description = "Database missing/unreachable", body = DegradedResponse),
    ),
)]
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    // Mantido como `Value` aqui porque o formato varia por ramo (200 vs 503);
    // o Swagger documenta cada ramo com seu schema (`StatusResponse`,
    // `DegradedResponse`). TODO(2.5+): unificar em enum untagged.
    match state.pool {
        Some(ref pool) => match sqlx::query("SELECT 1").execute(pool).await {
            Ok(_) => (StatusCode::OK, Json(serde_json::json!({"status": "ready"}))),
            Err(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({"status": "degraded", "db": "unreachable"})),
            ),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({"status": "degraded", "db": "not_configured"})),
        ),
    }
}
