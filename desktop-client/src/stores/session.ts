/**
 * Sessão do operador (US02 Task 2.4).
 *
 * Zustand em MEMÓRIA + IPC com o motor Rust (nunca armazenamento web):
 * - `login()`: autentica na Cloud API e envia o JWT ao cofre via `invoke`
 *   IMEDIATAMENTE (DoD) — o token nunca toca disco do navegador.
 * - `restore()`: na abertura do app, recupera o token do cofre e o mantém
 *   só em memória para o cliente HTTP montar o `Authorization`.
 * - `logout()`: limpa memória E cofre.
 */

import { create } from "zustand";
import { api, setApiToken } from "../lib/api";
import { vaultClear, vaultLoad, vaultSave } from "../lib/tauri";

export interface SessionUser {
  id: string;
  store_id: string;
  name: string;
  email: string;
  role: string;
}

interface SessionState {
  user: SessionUser | null;
  ready: boolean;
  login: (storeId: string, email: string, password: string) => Promise<void>;
  restore: () => Promise<void>;
  logout: () => Promise<void>;
}

interface LoginResponse {
  token: string;
  token_type: string;
  expires_in: number;
  user: SessionUser;
}

export const useSession = create<SessionState>()((set) => ({
  user: null,
  ready: false,

  login: async (storeId, email, password) => {
    const res = await api<LoginResponse>("/api/v1/auth/login", {
      method: "POST",
      headers: { "X-Store-ID": storeId },
      body: JSON.stringify({ email, password }),
    });
    // Imediatamente ao cofre do SO via IPC (DoD) — e só à memória local.
    // Fora do Tauri (web dev) o cofre é no-op: sessão vive só em memória.
    await vaultSave(res.token);
    setApiToken(res.token);
    set({ user: res.user, ready: true });
  },

  restore: async () => {
    // Abertura do app: token silencioso do cofre → memória (Happy Path DoD).
    // Bloco à prova de web: qualquer falha cai para "sem sessão", nunca trava.
    try {
      const token = await vaultLoad();
      if (token) {
        setApiToken(token);
        // A identidade é revalidada contra a API (o JWT pode ter expirado).
        try {
          const me = await api<SessionUser>("/api/v1/me");
          set({ user: me, ready: true });
          return;
        } catch {
          await vaultClear();
          setApiToken(null);
        }
      }
    } catch {
      setApiToken(null);
    }
    set({ user: null, ready: true });
  },

  logout: async () => {
    await vaultClear();
    setApiToken(null);
    set({ user: null });
  },
}));
