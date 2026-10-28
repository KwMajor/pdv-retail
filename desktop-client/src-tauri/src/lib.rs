//! Núcleo do PDV desktop (US02 Task 2.4).
//!
//! - [`vault`]: guarda do JWT no cofre nativo do SO (sempre compilado e testado).
//! - `commands`: pontes IPC `invoke` (só com a feature `tauri-app`).

pub mod vault;

#[cfg(feature = "tauri-app")]
pub mod commands;

/// Monta a janela e registra os commands do cofre.
///
/// Vive na lib (mesmo crate dos commands) porque `generate_handler!` resolve
/// os macros `__cmd__*` gerados pelo `#[tauri::command]` no crate de origem.
#[cfg(feature = "tauri-app")]
pub fn run() -> Result<(), tauri::Error> {
    use commands::{clear_session_token, load_session_token, save_session_token, VaultState};

    tauri::Builder::default()
        .manage(VaultState::default())
        .invoke_handler(tauri::generate_handler![
            save_session_token,
            load_session_token,
            clear_session_token,
        ])
        .run(tauri::generate_context!())
}
