//! Núcleo do PDV desktop (US02 Task 2.4).
//!
//! - [`vault`]: guarda do JWT no cofre nativo do SO (sempre compilado e testado).
//! - `commands`: pontes IPC `invoke` (só com a feature `tauri-app`).

pub mod vault;

#[cfg(feature = "tauri-app")]
pub mod commands;
