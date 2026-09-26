//! JWT da Cloud API (US02 Task 2.2).
//!
//! - Emissão no login (HMAC-SHA256). Claims obrigatórios: `sub` (user_id),
//!   `store_id`, `role`, `exp` (teto de 12h — cobre o turno mais longo).
//! - Validação no `TenantContext`: assinatura + expiração. Qualquer falha
//!   (token adulterado, expirado, segredo trocado) → `401`, sem detalhe.

use chrono::Utc;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::models::UserRole;

/// Teto do DoD: 12h. `expires_in` da resposta de login usa este valor.
pub const TOKEN_TTL_SECS: i64 = 12 * 3600;
/// Segredo HMAC com menos que isso é recusado no boot (força bruta).
pub const MIN_SECRET_LEN: usize = 32;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// `user_id` do funcionário.
    pub sub: Uuid,
    pub store_id: Uuid,
    /// Minúsculo (`admin`|`manager`|`cashier`); parse p/ `UserRole` na validação.
    pub role: String,
    pub exp: i64,
    pub iat: i64,
}

/// Sem `Debug` de propósito: chaves nunca aparecem em log.
#[derive(Clone)]
pub struct JwtKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl JwtKeys {
    pub fn from_secret(secret: &str) -> Result<Self, String> {
        if secret.len() < MIN_SECRET_LEN {
            return Err("JWT_SECRET deve ter ao menos 32 caracteres".to_string());
        }
        Ok(Self {
            encoding: EncodingKey::from_secret(secret.as_bytes()),
            decoding: DecodingKey::from_secret(secret.as_bytes()),
        })
    }

    /// Emite o token. Retorna `(token, exp_unix)`.
    pub fn issue(
        &self,
        user_id: Uuid,
        store_id: Uuid,
        role: UserRole,
    ) -> Result<(String, i64), jsonwebtoken::errors::Error> {
        let now = Utc::now().timestamp();
        self.issue_at(user_id, store_id, role, now + TOKEN_TTL_SECS)
    }

    /// Emissão com `exp` explícito — costura dos testes de expiração.
    pub fn issue_at(
        &self,
        user_id: Uuid,
        store_id: Uuid,
        role: UserRole,
        exp: i64,
    ) -> Result<(String, i64), jsonwebtoken::errors::Error> {
        let claims = Claims {
            sub: user_id,
            store_id,
            role: role.as_str().to_string(),
            exp,
            iat: Utc::now().timestamp(),
        };
        encode(&Header::new(Algorithm::HS256), &claims, &self.encoding).map(|t| (t, exp))
    }

    /// Decodifica e valida assinatura + expiração (HS256, `exp` obrigatório).
    /// `leeway` de 30s: tolerância a relógio entre o hardware do PDV e a API.
    pub fn validate(&self, token: &str) -> Result<Claims, jsonwebtoken::errors::Error> {
        let mut validation = Validation::new(Algorithm::HS256);
        validation.leeway = 30;
        decode::<Claims>(token, &self.decoding, &validation).map(|data| data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Segredo só dos testes (nunca em prod).
    fn keys() -> JwtKeys {
        JwtKeys::from_secret("test-only-secret-com-mais-de-32-chars").unwrap()
    }

    #[test]
    fn segredo_curto_recusado() {
        assert!(JwtKeys::from_secret("curto").is_err());
        assert!(keys().validate("x").is_err());
    }

    #[test]
    fn roundtrip_preserva_claims() {
        let keys = keys();
        let (token, exp) = keys
            .issue(Uuid::new_v4(), Uuid::new_v4(), UserRole::Cashier)
            .unwrap();
        let claims = keys.validate(&token).unwrap();
        assert_eq!(claims.role, "cashier");
        assert_eq!(claims.exp, exp);
    }

    #[test]
    fn segredo_trocado_invalida_assinatura() {
        let a = keys();
        let b = JwtKeys::from_secret("outro-segredo-valido-com-32-chars-xy").unwrap();
        let (token, _) = a
            .issue(Uuid::new_v4(), Uuid::new_v4(), UserRole::Manager)
            .unwrap();
        assert!(b.validate(&token).is_err());
    }

    #[test]
    fn expirado_rejeitado() {
        let keys = keys();
        // Bem além do leeway de 30s: determinístico.
        let past = Utc::now().timestamp() - 3600;
        let (token, _) = keys
            .issue_at(Uuid::new_v4(), Uuid::new_v4(), UserRole::Admin, past)
            .unwrap();
        let err = keys.validate(&token).unwrap_err();
        assert!(matches!(
            err.kind(),
            jsonwebtoken::errors::ErrorKind::ExpiredSignature
        ));
    }
}
