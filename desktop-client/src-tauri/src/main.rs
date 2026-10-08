//! Binário do PDV desktop (exige feature `tauri-app` + pré-requisitos do SO).
//!
//! Registra o [`VaultState`](pdv_desktop::commands::VaultState) e os commands
//! do cofre; o React acessa via `invoke('save_session_token', …)` etc.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use pdv_desktop::commands::{
    clear_session_token, load_session_token, save_session_token, VaultState,
};

fn main() {
    // Sem `expect` no boot: falha de inicialização vira mensagem + exit(1),
    // nunca panic (sem stack trace para o operador do caixa).
    if let Err(e) = tauri::Builder::default()
        .manage(VaultState::default())
        .invoke_handler(tauri::generate_handler![
            save_session_token,
            load_session_token,
            clear_session_token,
        ])
        .run(tauri::generate_context!())
    {
        eprintln!("falha ao iniciar o PDV desktop: {e}");
        std::process::exit(1);
    }
}
