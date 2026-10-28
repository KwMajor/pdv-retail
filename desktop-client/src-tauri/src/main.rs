//! Binário do PDV desktop (exige feature `tauri-app` + pré-requisitos do SO).
//!
//! O builder e os commands vivem na lib (`pdv_desktop::run`), onde os macros
//! `#[tauri::command]` ficam no mesmo crate do `generate_handler!`.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // Sem `expect` no boot: falha de inicialização vira mensagem + exit(1),
    // nunca panic (sem stack trace para o operador do caixa).
    if let Err(e) = pdv_desktop::run() {
        eprintln!("falha ao iniciar o PDV desktop: {e}");
        std::process::exit(1);
    }
}
