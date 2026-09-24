/**
 * Cliente HTTP da Cloud API (US02 Task 2.4).
 *
 * O JWT vive SÓ na memória deste módulo (variável de módulo, nunca em
 * armazenamento web do navegador em texto plano — há um teste automatizado
 * que proíbe esses armazenamentos em todo `src/`). A sessão é restaurada
 * no boot via IPC com o cofre do SO (ver `stores/session.ts`).
 */

const API_BASE = import.meta.env.VITE_API_URL ?? "http://localhost:3000";

let inMemoryToken: string | null = null;

/** Chamado pelo boot (token vindo do cofre) e pelo login. Só memória. */
export function setApiToken(token: string | null): void {
  inMemoryToken = token;
}

export function getApiToken(): string | null {
  return inMemoryToken;
}

export async function api<T>(path: string, init: RequestInit = {}): Promise<T> {
  const headers = new Headers(init.headers);
  headers.set("Content-Type", "application/json");
  if (inMemoryToken) {
    headers.set("Authorization", `Bearer ${inMemoryToken}`);
  }
  const res = await fetch(`${API_BASE}${path}`, { ...init, headers });
  if (res.status === 401) {
    // Sessão inválida/expirada: derruba o estado em memória (o cofre é
    // limpo pelo fluxo de logout; aqui só soltamos a referência).
    inMemoryToken = null;
  }
  if (!res.ok) {
    throw new Error(`API ${res.status} em ${path}`);
  }
  return (await res.json()) as T;
}
