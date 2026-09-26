//! `StoreHint`: dica de loja NÃO autenticada, usada SÓ pelo `/auth/login`
//! para localizar o tenant. Todo o resto exige JWT (`TenantContext`).

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::middleware::tenant::STORE_ID_HEADER;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreHint {
    pub store_id: Uuid,
}

#[async_trait::async_trait]
impl FromRequestParts<AppState> for StoreHint {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        _state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(STORE_ID_HEADER)
            .ok_or_else(|| AppError::BadRequest("header X-Store-ID ausente".to_string()))?;
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
    use axum::http::Request;

    use crate::jwt::JwtKeys;

    fn state() -> AppState {
        AppState {
            pool: None,
            jwt: JwtKeys::from_secret("test-only-secret-com-mais-de-32-chars").unwrap(),
            expose_docs: false,
        }
    }

    fn parts_with(headers: &[(&str, &str)]) -> Parts {
        let mut builder = Request::builder().uri("/api/v1/auth/login");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let (parts, _) = builder.body(Body::empty()).unwrap().into_parts();
        parts
    }

    #[tokio::test]
    async fn store_hint_so_para_login() {
        let id = Uuid::new_v4().to_string();
        let mut parts = parts_with(&[(STORE_ID_HEADER, &id)]);
        let hint = StoreHint::from_request_parts(&mut parts, &state())
            .await
            .unwrap();
        assert_eq!(hint.store_id.to_string(), id);
        let mut sem = parts_with(&[]);
        assert!(
            StoreHint::from_request_parts(&mut sem, &state())
                .await
                .is_err()
        );
    }
}
