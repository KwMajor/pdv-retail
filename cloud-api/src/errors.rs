//! Erro único da API: nunca `unwrap`/`panic` em handler,
//! nunca vazar PII ou detalhe interno no JSON público.

use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

#[derive(Debug, thiserror::Error)]
// Variantes de auth serão consumidas na US02; mantém o build sem warnings até lá.
#[allow(dead_code)]
pub enum AppError {
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
    #[error("não autenticado")]
    Unauthorized,
    #[error("acesso negado")]
    Forbidden,
    #[error("não encontrado")]
    NotFound,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Conflict(String),
    #[error("erro interno")]
    Internal(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, code, message) = match &self {
            AppError::Db(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "DB_ERROR",
                "erro de banco de dados".to_string(),
            ),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "UNAUTHORIZED", self.to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "FORBIDDEN", self.to_string()),
            AppError::NotFound => (StatusCode::NOT_FOUND, "NOT_FOUND", self.to_string()),
            AppError::BadRequest(m) => (StatusCode::BAD_REQUEST, "BAD_REQUEST", m.clone()),
            AppError::Conflict(m) => (StatusCode::CONFLICT, "CONFLICT", m.clone()),
            // Detalhe interno vai só pro log, nunca pro cliente.
            AppError::Internal(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "INTERNAL",
                "erro interno".to_string(),
            ),
        };
        if matches!(self, AppError::Db(_) | AppError::Internal(_)) {
            tracing::error!(?self, "app error");
        }
        (status, Json(json!({"code": code, "message": message}))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn status_and_json(err: AppError) -> (StatusCode, serde_json::Value) {
        let res = err.into_response();
        let status = res.status();
        let body = to_bytes(res.into_body(), 1024).await.unwrap();
        (status, serde_json::from_slice(&body).unwrap())
    }

    #[tokio::test]
    async fn unauthorized_maps_401() {
        let (status, json) = status_and_json(AppError::Unauthorized).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(json["code"], "UNAUTHORIZED");
    }

    #[tokio::test]
    async fn forbidden_maps_403() {
        let (status, json) = status_and_json(AppError::Forbidden).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(json["code"], "FORBIDDEN");
    }

    #[tokio::test]
    async fn not_found_maps_404() {
        let (status, json) = status_and_json(AppError::NotFound).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(json["code"], "NOT_FOUND");
    }

    #[tokio::test]
    async fn bad_request_maps_400_with_message() {
        let (status, json) = status_and_json(AppError::BadRequest("cpf inválido".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(json["code"], "BAD_REQUEST");
        assert_eq!(json["message"], "cpf inválido");
    }

    #[tokio::test]
    async fn conflict_maps_409_with_message() {
        let (status, json) =
            status_and_json(AppError::Conflict("email já cadastrado nesta loja".into())).await;
        assert_eq!(status, StatusCode::CONFLICT);
        assert_eq!(json["code"], "CONFLICT");
        assert_eq!(json["message"], "email já cadastrado nesta loja");
    }

    #[tokio::test]
    async fn db_error_maps_500_generic() {
        // Mensagem genérica: detalhe do driver nunca chega ao cliente.
        let (status, json) = status_and_json(AppError::Db(sqlx::Error::RowNotFound)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["code"], "DB_ERROR");
        assert_eq!(json["message"], "erro de banco de dados");
        assert!(!json["message"].as_str().unwrap().contains("RowNotFound"));
    }

    #[tokio::test]
    async fn internal_hides_detail() {
        // Segredo interno (ex: string de conexão) não pode vazar no JSON.
        let (status, json) =
            status_and_json(AppError::Internal("postgres://pdv:SENHA@host".into())).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(json["code"], "INTERNAL");
        assert_eq!(json["message"], "erro interno");
        assert!(!json.to_string().contains("SENHA"));
    }
}
