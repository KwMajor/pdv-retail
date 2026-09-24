//! Pontes IPC do cofre (US02 Task 2.4). Feature `tauri-app`.
//!
//! O React CHAMA estes commands via `invoke` — nunca `localStorage`:
//! - após o login: `save_session_token` (guarda no cofre do SO);
//! - na abertura: `load_session_token` (restaura a sessão em memória);
//! - no logout: `clear_session_token`.
//! Erros viram `Result<T, String>` amigável (sem token, sem detalhe interno).

use tauri::State;

use crate::vault::{KeyringVault, TokenVault, VaultError};

/// Cofre compartilhado (uma sessão por máquina).
pub struct VaultState(pub KeyringVault);

impl Default for VaultState {
    fn default() -> Self {
        Self(KeyringVault::session())
    }
}

fn friendly(error: VaultError) -> String {
    match error {
        VaultError::NotFound => "sessão não encontrada".to_string(),
        // Indisponível ou backend: mensagem única, sem vazar causa/token.
        VaultError::Unavailable | VaultError::Backend => {
            "cofre de credenciais indisponível".to_string()
        }
    }
}

#[tauri::command]
pub fn save_session_token(state: State<VaultState>, token: String) -> Result<(), String> {
    state.0.save(&token).map_err(friendly)
}

#[tauri::command]
pub fn load_session_token(state: State<VaultState>) -> Result<Option<String>, String> {
    match state.0.load() {
        Ok(token) => Ok(Some(token)),
        // Sem sessão salva = boot sem login anterior (não é erro).
        Err(VaultError::NotFound) => Ok(None),
        Err(e) => Err(friendly(e)),
    }
}

#[tauri::command]
pub fn clear_session_token(state: State<VaultState>) -> Result<(), String> {
    state.0.clear().map_err(friendly)
}
