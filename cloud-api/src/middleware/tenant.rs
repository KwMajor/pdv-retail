//! `TenantContext`: extractor Axum que injeta identidade do tenant nas rotas.
//!
//! - US02 (atual): lê `Authorization: Bearer <JWT>`, valida assinatura (HMAC)
//!   e expiração, injeta `store_id` + `user_id` + `role`. Handlers não parseiam nada.
//! - Qualquer falha (ausente, adulterado, expirado, papel desconhecido) →
//!   `401 Unauthorized` imediato, antes do handler. Sem `unwrap`/`panic`,
//!   sem vazar detalhe em log ou resposta.
//!
//! `StoreHint` é o ÚNICO resquício do `X-Store-ID`: dica NÃO autenticada usada
//! só pelo `/auth/login` para localizar a loja. Todo o resto exige JWT.

use axum::{extract::FromRequestParts, http::request::Parts};
use uuid::Uuid;

use crate::AppState;
use crate::errors::AppError;
use crate::models::UserRole;

/// Header de dica de tenant (só login). Nunca é prova de identidade.
pub const STORE_ID_HEADER: &str = "x-store-id";

/// Identidade autenticada do funcionário, pronta para repositórios e RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TenantContext {
    pub store_id: Uuid,
    pub user_id: Uuid,
    pub role: UserRole,
}

impl TenantContext {
    /// RBAC (OWASP A01, US02 Task 2.3): o papel do JWT precisa estar na lista
    /// exigida pela rota. Fora dela → `403`, sem revelar nada além do código.
    pub fn require_roles(&self, allowed: &[UserRole]) -> Result<(), AppError> {
        if allowed.contains(&self.role) {
            Ok(())
        } else {
            Err(AppError::Forbidden)
        }
    }

    /// Atalho: gestão (`admin`/`manager`) — retaguarda e cadastros.
    pub fn require_manager(&self) -> Result<(), AppError> {
        self.require_roles(&[UserRole::Admin, UserRole::Manager])
    }
}

#[async_trait::async_trait]
impl FromRequestParts<AppState> for TenantContext {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let raw = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .ok_or(AppError::Unauthorized)?;
        let text = raw.to_str().map_err(|_| AppError::Unauthorized)?;
        let token = text.strip_prefix("Bearer ").ok_or(AppError::Unauthorized)?;
        if token.is_empty() {
            return Err(AppError::Unauthorized);
        }
        // Assinatura e exp validados aqui: adulteração/expiração caem neste erro.
        let claims = state
            .jwt
            .validate(token)
            .map_err(|_| AppError::Unauthorized)?;
        let role = match claims.role.as_str() {
            "admin" => UserRole::Admin,
            "manager" => UserRole::Manager,
            "cashier" => UserRole::Cashier,
            _ => return Err(AppError::Unauthorized),
        };
        Ok(Self {
            store_id: claims.store_id,
            user_id: claims.sub,
            role,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::response::IntoResponse;

    use crate::jwt::JwtKeys;

    /// Segredo só dos testes (nunca em prod).
    fn keys() -> JwtKeys {
        JwtKeys::from_secret("test-only-secret-com-mais-de-32-chars").unwrap()
    }

    fn state() -> AppState {
        AppState {
            pool: None,
            jwt: keys(),
            expose_docs: false,
        }
    }

    fn parts_with(headers: &[(&str, &str)]) -> Parts {
        let mut builder = Request::builder().uri("/api/v1/ping");
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let (parts, _) = builder.body(Body::empty()).unwrap().into_parts();
        parts
    }

    async fn extract(headers: &[(&str, &str)]) -> Result<TenantContext, AppError> {
        let mut parts = parts_with(headers);
        TenantContext::from_request_parts(&mut parts, &state()).await
    }

    fn status_of(err: AppError) -> StatusCode {
        err.into_response().status()
    }

    #[tokio::test]
    async fn bearer_valido_entrega_identidade() {
        let keys = keys();
        let user = Uuid::new_v4();
        let store = Uuid::new_v4();
        let (token, _) = keys.issue(user, store, UserRole::Manager).unwrap();
        let auth = format!("Bearer {token}");
        let ctx = extract(&[("authorization", &auth)]).await.unwrap();
        assert_eq!(
            (ctx.store_id, ctx.user_id, ctx.role),
            (store, user, UserRole::Manager)
        );
    }

    #[tokio::test]
    async fn sem_bearer_rejeita_401() {
        assert_eq!(
            status_of(extract(&[]).await.unwrap_err()),
            StatusCode::UNAUTHORIZED
        );
        let bad = extract(&[("authorization", "Token abc")])
            .await
            .unwrap_err();
        assert_eq!(status_of(bad), StatusCode::UNAUTHORIZED);
        let vazio = extract(&[("authorization", "Bearer ")]).await.unwrap_err();
        assert_eq!(status_of(vazio), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn payload_adulterado_rejeita_401() {
        let keys = keys();
        let (mut token, _) = keys
            .issue(Uuid::new_v4(), Uuid::new_v4(), UserRole::Cashier)
            .unwrap();
        // Vira um byte do segmento de payload: assinatura deixa de conferir.
        let dot = token.find('.').unwrap();
        let byte = token.as_bytes()[dot + 1];
        token.replace_range(dot + 1..dot + 2, if byte == b'A' { "B" } else { "A" });
        let auth = format!("Bearer {token}");
        assert_eq!(
            status_of(extract(&[("authorization", &auth)]).await.unwrap_err()),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn expirado_rejeita_401() {
        use chrono::Utc;
        let keys = keys();
        let (token, _) = keys
            .issue_at(
                Uuid::new_v4(),
                Uuid::new_v4(),
                UserRole::Admin,
                Utc::now().timestamp() - 3600,
            )
            .unwrap();
        let auth = format!("Bearer {token}");
        assert_eq!(
            status_of(extract(&[("authorization", &auth)]).await.unwrap_err()),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn papel_desconhecido_rejeita_401() {
        // Assinatura válida, mas `role` fora da lista: o extractor barra.
        use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
        let now = chrono::Utc::now().timestamp();
        let claims = crate::jwt::Claims {
            sub: Uuid::new_v4(),
            store_id: Uuid::new_v4(),
            role: "dono".to_string(),
            exp: now + 3600,
            iat: now,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"test-only-secret-com-mais-de-32-chars"),
        )
        .unwrap();
        let auth = format!("Bearer {token}");
        assert_eq!(
            status_of(extract(&[("authorization", &auth)]).await.unwrap_err()),
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test]
    async fn caixa_nao_passa_no_require_manager() {
        let ctx = TenantContext {
            store_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            role: UserRole::Cashier,
        };
        assert_eq!(
            status_of(ctx.require_manager().unwrap_err()),
            StatusCode::FORBIDDEN
        );
        let gerente = TenantContext {
            role: UserRole::Manager,
            ..ctx
        };
        assert!(gerente.require_manager().is_ok());
    }
}
