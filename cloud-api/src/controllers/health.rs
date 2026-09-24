use axum::{Json, extract::State, http::StatusCode};
use serde_json::json;

use crate::AppState;

/// Liveness: sempre 200 se o processo está de pé.
pub async fn health() -> (StatusCode, Json<serde_json::Value>) {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

/// Readiness: 200 só com pool + `SELECT 1` ok; 503 caso contrário.
pub async fn ready(State(state): State<AppState>) -> (StatusCode, Json<serde_json::Value>) {
    match state.pool {
        Some(ref pool) => match sqlx::query("SELECT 1").execute(pool).await {
            Ok(_) => (StatusCode::OK, Json(json!({"status": "ready"}))),
            Err(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status": "degraded", "db": "unreachable"})),
            ),
        },
        None => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({"status": "degraded", "db": "not_configured"})),
        ),
    }
}
