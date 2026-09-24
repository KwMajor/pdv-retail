//! `TenantContext`: extractor Axum que injeta o `store_id` nas rotas.
//!
//! - US01 (esta): lê o header temporário `X-Store-ID` e valida como UUID.
//! - US02 (futuro): a leitura passa a vir do payload do JWT; handlers não mudam.
//!
//! Ausente ou inválido → rejeição imediata `400 Bad Request` antes do handler
//! (o controller nem é alcançado). Sem `unwrap`/`panic`, sem vazar PII em log.

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::errors::AppError;

/// Header temporário de tenant (US01). Removido quando o JWT chegar (US02).
pub const STORE_ID_HEADER: &str = "x-store-id";

/// `store_id` do tenant, extraído da requisição e pronto para os repositórios.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantContext {
    pub store_id: Uuid,
}

#[async_trait::async_trait]
impl<S> FromRequestParts<S> for TenantContext
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let raw = parts.headers.get(STORE_ID_HEADER).ok_or_else(|| {
            AppError::BadRequest("header X-Store-ID ausente".to_string())
        })?;
        let text = raw.to_str().map_err(|_| {
            AppError::BadRequest("header X-Store-ID inválido: UUID esperado".to_string())
        })?;
        let store_id = text.parse::<Uuid>().map_err(|_| {
            AppError::BadRequest("header X-Store-ID inválido: UUID esperado".to_string())
        })?;
        Ok(Self { store_id })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;

    /// Monta `Parts` com ou sem o header, sem subir socket.
    fn parts_with(header: Option<&str>) -> Parts {
        let mut builder = Request::builder().uri("/api/v1/ping");
        if let Some(value) = header {
            builder = builder.header(STORE_ID_HEADER, value);
        }
        let request = builder.body(Body::empty()).unwrap();
        let (parts, _) = request.into_parts();
        parts
    }

    async fn extract(header: Option<&str>) -> Result<TenantContext, AppError> {
        let mut parts = parts_with(header);
        TenantContext::from_request_parts(&mut parts, &()).await
    }

    #[tokio::test]
    async fn header_valido_entrega_store_id() {
        let id = Uuid::new_v4();
        let ctx = extract(Some(&id.to_string())).await.unwrap();
        assert_eq!(ctx.store_id, id);
    }

    #[tokio::test]
    async fn header_ausente_rejeita_400() {
        let err = extract(None).await.unwrap_err();
        let res = err.into_response();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn header_nao_uuid_rejeita_400_sem_panic() {
        // Edge do DoD: texto livre em vez de UUID.
        for invalido in ["loja-do-joao", "", "123", "not-a-uuid-at-all"] {
            let err = extract(Some(invalido)).await.unwrap_err();
            let res = err.into_response();
            assert_eq!(res.status(), StatusCode::BAD_REQUEST, "{invalido}");
        }
    }

    #[tokio::test]
    async fn mensagem_nao_ecoa_valor_nem_panica() {
        let err = extract(Some("loja-do-joao")).await.unwrap_err();
        let res = err.into_response();
        let bytes = axum::body::to_bytes(res.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(json["code"], "BAD_REQUEST");
        assert!(json["message"].as_str().unwrap().contains("UUID"));
    }
}
