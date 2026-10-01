/**
 * Login do operador (US02).
 *
 * Pede loja (UUID), email e senha; autentica na Cloud API e entrega a sessão
 * ao store (que persiste no cofre via IPC). Erro de credencial vira mensagem,
 * nunca detalhe interno.
 */

import { useState } from "react";
import { useSession } from "../stores/session";

export function LoginForm() {
  const login = useSession((s) => s.login);
  const [storeId, setStoreId] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function submit() {
    setError(null);
    if (storeId.trim() === "" || email.trim() === "" || password === "") {
      setError("Preencha loja, email e senha.");
      return;
    }
    setBusy(true);
    try {
      await login(storeId.trim(), email.trim(), password);
    } catch {
      setError("Loja, email ou senha inválidos.");
    } finally {
      setBusy(false);
    }
  }

  return (
    <section aria-label="Login do operador" className="card">
      <h1>PDV Retail — Entrar</h1>
      <label>
        ID da loja (UUID)
        <input
          value={storeId}
          onChange={(e) => setStoreId(e.target.value)}
          placeholder="22222222-2222-2222-2222-222222222222"
          autoComplete="off"
        />
      </label>
      <label>
        Email
        <input
          value={email}
          onChange={(e) => setEmail(e.target.value)}
          autoComplete="username"
        />
      </label>
      <label>
        Senha
        <input
          type="password"
          value={password}
          onChange={(e) => setPassword(e.target.value)}
          autoComplete="current-password"
          onKeyDown={(e) => {
            if (e.key === "Enter") void submit();
          }}
        />
      </label>
      {error && <p role="alert">{error}</p>}
      <button disabled={busy} className="primary" onClick={() => void submit()}>
        {busy ? "Entrando…" : "Entrar"}
      </button>
    </section>
  );
}
