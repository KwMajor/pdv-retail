//! Cofre do token JWT (US02 Task 2.4).
//!
//! - Produção: [`KeyringVault`] — Credential Manager (Windows), Keychain
//!   (macOS) ou Secret Service (Linux) via crate `keyring`. Uma sessão ativa
//!   por máquina (o PDV tem um operador por vez): entrada única `session`.
//! - Testes/dev sem cofre: [`MemoryVault`] (nunca em produção).
//! - Regras: sem `unwrap`/`panic`; o token NUNCA aparece em erro ou log;
//!   `clear` é idempotente (entrada ausente = sucesso).

use std::collections::HashMap;
use std::sync::Mutex;

use thiserror::Error;

/// Serviço (namespace) e entrada usados no cofre do SO.
pub const VAULT_SERVICE: &str = "pdv-retail";
pub const VAULT_USER: &str = "pdv-session";

#[derive(Debug, Error)]
pub enum VaultError {
    /// Cofre indisponível (ex: Secret Service fora do ar). Sem detalhe do token.
    #[error("cofre de credenciais indisponível")]
    Unavailable,
    /// Entrada ausente — só retornado por `load` (em `clear` vira sucesso).
    #[error("sessão não encontrada no cofre")]
    NotFound,
    #[error("falha ao acessar o cofre de credenciais")]
    Backend,
}

impl From<keyring::Error> for VaultError {
    fn from(e: keyring::Error) -> Self {
        match e {
            keyring::Error::NoEntry => VaultError::NotFound,
            // Plataforma sem cofre, DBus fora, permissão negada etc.
            _ => VaultError::Unavailable,
        }
    }
}

/// Contrato do cofre. `save` sobrescreve; `load` devolve `NotFound` se vazio.
pub trait TokenVault: Send + Sync {
    fn save(&self, token: &str) -> Result<(), VaultError>;
    fn load(&self) -> Result<String, VaultError>;
    fn clear(&self) -> Result<(), VaultError>;
}

/// Cofre nativo do SO (produção). Por padrão usa a entrada única `session`
/// (um operador por máquina); testes usam entrada isolada via [`KeyringVault::isolated`].
#[derive(Debug, Clone, Default)]
pub struct KeyringVault {
    user: String,
}

impl KeyringVault {
    /// Entrada de produção (`pdv-session`).
    pub fn session() -> Self {
        Self { user: VAULT_USER.to_string() }
    }

    /// Entrada isolada (testes): nunca toca a sessão real do operador.
    pub fn isolated(tag: &str) -> Self {
        Self { user: format!("{VAULT_USER}-test-{tag}") }
    }

    fn entry(&self) -> Result<keyring::Entry, VaultError> {
        // `Entry::new` pode falhar (plataforma sem backend): sem unwrap.
        keyring::Entry::new(VAULT_SERVICE, &self.user).map_err(|_| VaultError::Unavailable)
    }
}

impl TokenVault for KeyringVault {
    fn save(&self, token: &str) -> Result<(), VaultError> {
        self.entry()?.set_password(token).map_err(|_| VaultError::Backend)
    }

    fn load(&self) -> Result<String, VaultError> {
        self.entry()?.get_password().map_err(VaultError::from)
    }

    fn clear(&self) -> Result<(), VaultError> {
        match self.entry()?.delete_credential() {
            Ok(()) => Ok(()),
            // Idempotente: limpar sessão já vazia é sucesso.
            Err(keyring::Error::NoEntry) => Ok(()),
            Err(_) => Err(VaultError::Backend),
        }
    }
}

/// Cofre em memória (testes e dev sem SO com cofre). NUNCA em produção.
#[derive(Debug, Default)]
pub struct MemoryVault {
    inner: Mutex<HashMap<String, String>>,
}

impl MemoryVault {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenVault for MemoryVault {
    fn save(&self, token: &str) -> Result<(), VaultError> {
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(VAULT_USER.to_string(), token.to_string());
            Ok(())
        } else {
            Err(VaultError::Backend)
        }
    }

    fn load(&self) -> Result<String, VaultError> {
        if let Ok(guard) = self.inner.lock() {
            guard.get(VAULT_USER).cloned().ok_or(VaultError::NotFound)
        } else {
            Err(VaultError::Backend)
        }
    }

    fn clear(&self) -> Result<(), VaultError> {
        if let Ok(mut guard) = self.inner.lock() {
            guard.remove(VAULT_USER);
            Ok(())
        } else {
            Err(VaultError::Backend)
        }
    }
}

/// Sonda se há cofre funcional (para pular o teste live onde não houver).
#[cfg(test)]
fn secret_service_available() -> bool {
    let probe = format!("{}-probe", VAULT_USER);
    let entry = match keyring::Entry::new(VAULT_SERVICE, &probe) {
        Ok(e) => e,
        Err(_) => return false,
    };
    if entry.set_password("probe").is_err() {
        return false;
    }
    let ok = entry.get_password().map(|v| v == "probe").unwrap_or(false);
    let _ = entry.delete_credential();
    ok
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contrato válido para qualquer backend: save→load, sobrescrita, clear.
    fn assert_contract(vault: &impl TokenVault) {
        vault.clear().unwrap();
        assert!(matches!(vault.load(), Err(VaultError::NotFound)));
        vault.save("jwt-1").unwrap();
        assert_eq!(vault.load().unwrap(), "jwt-1");
        vault.save("jwt-2").unwrap();
        assert_eq!(vault.load().unwrap(), "jwt-2");
        vault.clear().unwrap();
        assert!(matches!(vault.load(), Err(VaultError::NotFound)));
        // Idempotente: limpar duas vezes não falha.
        vault.clear().unwrap();
    }

    #[test]
    fn memory_vault_cumpre_contrato() {
        assert_contract(&MemoryVault::new());
    }

    #[test]
    fn erros_nunca_carregam_o_token() {
        let err = VaultError::NotFound.to_string();
        assert!(!err.contains("jwt"));
        // `save` vazio não quebra o contrato (token vazio = sessão inválida,
        // recusada pelo JWT; o cofre só guarda).
        let vault = MemoryVault::new();
        vault.save("").unwrap();
        assert_eq!(vault.load().unwrap(), "");
    }

    /// Happy Path real (DoD): save→load→clear no cofre nativo do SO, em
    /// entrada isolada (nunca toca o `session` do operador). Pula com aviso
    /// onde não houver Secret Service.
    #[test]
    fn keyring_roundtrip_no_cofre_nativo() {
        if !secret_service_available() {
            eprintln!("SKIP: sem Secret Service neste ambiente");
            return;
        }
        let vault =
            KeyringVault::isolated(&format!("rt-{}", std::process::id()));
        assert_contract(&vault);
        vault.clear().unwrap();
    }

    #[test]
    fn keyring_clear_idempotente() {
        if !secret_service_available() {
            eprintln!("SKIP: sem Secret Service neste ambiente");
            return;
        }
        let vault = KeyringVault::isolated(&format!("cl-{}", std::process::id()));
        vault.clear().unwrap();
        vault.clear().unwrap();
    }
}
