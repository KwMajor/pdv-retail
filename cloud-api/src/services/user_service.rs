//! Criação de usuários (US02 Task 2.1).
//!
//! - O `store_id` vem SEMPRE do chamador autenticado (`TenantContext`, que na
//!   US02 passará a sair do JWT). O payload não tem campo `store_id`: é
//!   estruturalmente impossível forçar outra loja.
//! - A senha em texto plano existe só neste escopo: hash Argon2id com salt
//!   aleatório imediato, antes de qualquer persistência. Nunca logar.
//! - Papel já chega tipado (`UserRole`): valor fora da lista nem passa da rota.

use argon2::{
    Argon2,
    password_hash::{PasswordHasher, SaltString},
};
use rand_core::OsRng;
use uuid::Uuid;

use crate::models::{User, UserRole};
use crate::repositories::{NewUser, PgUserRepository, UserRepository};

/// Senha de acesso (não é o PIN numérico do gerente, que tem fluxo próprio).
const PASSWORD_MIN_CHARS: usize = 8;
const PASSWORD_MAX_CHARS: usize = 128;

pub struct CreateUserInput {
    pub name: String,
    pub email: String,
    /// Texto plano transitório — jamais persistido ou logado.
    pub password: String,
    pub role: UserRole,
}

#[derive(Debug, thiserror::Error)]
pub enum CreateUserError {
    #[error("{0}")]
    Invalid(String),
    #[error("email já cadastrado nesta loja")]
    EmailTaken,
    /// Guarda só o detalhe para log; a resposta pública é sempre genérica.
    #[error("falha ao proteger credencial")]
    Hash(String),
    #[error("erro de banco de dados")]
    Db(#[from] sqlx::Error),
}

/// Cria o usuário na loja do criador. Retorna o `User` persistido (o controller
/// remove os hashes antes de responder).
pub async fn create_user(
    repo: &PgUserRepository,
    store_id: Uuid,
    input: CreateUserInput,
) -> Result<User, CreateUserError> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        return Err(CreateUserError::Invalid("nome é obrigatório".to_string()));
    }
    let email = input.email.trim().to_lowercase();
    if email.is_empty() || !email.contains('@') {
        return Err(CreateUserError::Invalid("email inválido".to_string()));
    }
    let password_chars = input.password.chars().count();
    if password_chars < PASSWORD_MIN_CHARS {
        return Err(CreateUserError::Invalid(
            "senha deve ter ao menos 8 caracteres".to_string(),
        ));
    }
    if password_chars > PASSWORD_MAX_CHARS {
        return Err(CreateUserError::Invalid(
            "senha deve ter no máximo 128 caracteres".to_string(),
        ));
    }

    // Checagem amigável antes de inserir; o UNIQUE do banco é o backstop
    // contra condição de corrida (mapeado para EmailTaken abaixo).
    if repo
        .find_by_email(store_id, &email)
        .await?
        .is_some()
    {
        return Err(CreateUserError::EmailTaken);
    }

    let password_hash =
        hash_password(&input.password).map_err(|e| CreateUserError::Hash(e.to_string()))?;

    repo.create(NewUser {
        store_id,
        name,
        email,
        password_hash,
        role: input.role.as_str().to_string(),
    })
    .await
    .map_err(|e| match &e {
        sqlx::Error::Database(db) if db.is_unique_violation() => CreateUserError::EmailTaken,
        _ => CreateUserError::Db(e),
    })
}

/// Argon2id padrão (OWASP A02) com salt aleatório por credencial.
pub(crate) fn hash_password(plain: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(plain.as_bytes(), &salt)?
        .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use argon2::password_hash::{PasswordHash, PasswordVerifier};

    #[test]
    fn hash_usa_salt_aleatorio_e_verifica() {
        let a = hash_password("segredo-123").unwrap();
        let b = hash_password("segredo-123").unwrap();
        // Salt embutido: mesmo plaintext gera hashes distintos (irreversível).
        assert_ne!(a, b);
        assert!(a.starts_with("$argon2id$"));
        let parsed = PasswordHash::new(&a).unwrap();
        assert!(Argon2::default().verify_password(b"segredo-123", &parsed).is_ok());
        assert!(Argon2::default().verify_password(b"errada", &parsed).is_err());
    }
}
