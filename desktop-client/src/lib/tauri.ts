/**
 * Ponte IPC com o cofre (US02 Task 2.4) que não quebra fora do Tauri.
 *
 * No navegador (`npm run dev`) não há bridge IPC: `invoke` rejeita e o
 * `restore()` travava na tela de loading. Estas funções detectam o runtime:
 * - Tauri: cofre do SO (produção, DoD).
 * - Web: sem persistência (sessão só em memória até recarregar).
 */

import { invoke } from "@tauri-apps/api/core";

/** `true` dentro da janela Tauri (bridge IPC presente). */
export function isTauri(): boolean {
  return (
    typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
  );
}

/** Guarda no cofre; fora do Tauri é no-op (memória já tem o token). */
export async function vaultSave(token: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("save_session_token", { token });
}

/** Lê do cofre; fora do Tauri devolve `null` (sem sessão anterior). */
export async function vaultLoad(): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("load_session_token");
}

/** Limpa o cofre; fora do Tauri é no-op. */
export async function vaultClear(): Promise<void> {
  if (!isTauri()) return;
  await invoke("clear_session_token");
}
