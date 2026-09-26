//! Login (US02 Task 2.2).
//!
//! Anti-enumeração: email inexistente, usuário inativo e senha errada devolvem
//! o MESMO erro genérico — e o caminho "usuário não existe" também paga o
//! custo de uma verificação Argon2 (hash dummy), equalizando o tempo.

use std::sync::OnceLock;
use uuid::Uuid;

use crate::auth::JwtKeys;
use crate::models::User;
use crate::repositories::{PgUserRepository, UserRepository};
use argon2::password_hash::{PasswordHash, PasswordVerifier};

use super::user_service::hash_password;

pub struct LoginInput {
    pub email: String,
    pub password: String,
}

pub struct LoginOutput {
    pub token: String,
    pub expires_at: i64,
    pub user: User,
}

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    /// Cobre todos os casos (não revela se o email existe).
    #[error("credenciais inválidas")]
    Invalid,
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
}

pub async fn login(
    repo: &PgUserRepository,
    keys: &JwtKeys,
    store_id: Uuid,
    input: LoginInput,
) -> Result<LoginOutput, LoginError> {
    let email = input.email.trim().to_lowercase();
    // Teto igual ao do cadastro: acima disso nunca é credencial válida.
    // Cai no erro genérico (sem oráculo) e nem chega ao Argon2.
    if input.password.chars().count() > super::user_service::PASSWORD_MAX_CHARS {
        return Err(LoginError::Invalid);
    }
    let found = repo.find_by_email(store_id, &email).await?;

    let user = match found {
        Some(u) if u.is_active => u,
        _ => {
            // Custo Argon2 também no "não existe/inativo": timing não entrega nada.
            verify_against_dummy(&input.password);
            return Err(LoginError::Invalid);
        }
    };

    if !check_hash(&user.password_hash, &input.password) {
        return Err(LoginError::Invalid);
    }

    let role = match user.role.as_str() {
        "admin" => crate::models::UserRole::Admin,
        "manager" => crate::models::UserRole::Manager,
        _ => crate::models::UserRole::Cashier,
    };
    let (token, expires_at) = keys
        .issue(user.id, user.store_id, role)
        .map_err(|_| LoginError::Invalid)?;

    Ok(LoginOutput {
        token,
        expires_at,
        user,
    })
}

fn check_hash(hash: &str, plain: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .map(|parsed| {
            argon2::Argon2::default()
                .verify_password(plain.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// Hash dummy estável por processo: equaliza o tempo do caminho negativo.
fn verify_against_dummy(plain: &str) {
    static DUMMY: OnceLock<String> = OnceLock::new();
    let hash = DUMMY
        .get_or_init(|| hash_password("dummy-credential-para-equalizar-tempo").unwrap_or_default());
    if !hash.is_empty() {
        let _ = check_hash(hash, plain);
    }
}
