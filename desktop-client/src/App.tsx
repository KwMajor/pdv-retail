/**
 * Shell mínimo do PDV (US02 Task 2.3).
 *
 * Navegação com cadeado visual por papel: "Vendas" para todos (frente de
 * caixa US06/US07), "Produtos" e "Relatórios" só gestão, "Configurações"
 * só admin. Relatórios chegam na US17; aqui o contrato de RBAC já vale.
 */

import { useEffect, useState } from "react";
import { useSession } from "./stores/session";
import { LoginForm } from "./components/LoginForm";
import { PosScreen } from "./components/PosScreen";
import { RoleGate } from "./components/RoleGate";
import { ProductForm } from "./components/ProductForm";
import { ADMIN_ROLES, MANAGERIAL_ROLES } from "./auth/roles";

type Screen = "vendas" | "produtos" | "relatorios" | "config";

export function App() {
  const { user, ready, restore, logout } = useSession();
  const [screen, setScreen] = useState<Screen>("vendas");

  useEffect(() => {
    void restore();
  }, [restore]);

  if (!ready) return <p>Carregando sessão…</p>;
  if (!user) return <LoginForm />;

  return (
    <div>
      <header>
        <nav>
          <button onClick={() => setScreen("vendas")}>Vendas</button>
          <RoleGate allowed={MANAGERIAL_ROLES}>
            <button onClick={() => setScreen("produtos")}>Produtos</button>
          </RoleGate>
          <RoleGate allowed={MANAGERIAL_ROLES}>
            <button onClick={() => setScreen("relatorios")}>Relatórios</button>
          </RoleGate>
          <RoleGate allowed={ADMIN_ROLES}>
            <button onClick={() => setScreen("config")}>Configurações</button>
          </RoleGate>
        </nav>
        <span>
          {user.name} ({user.role})
        </span>
        <button
          onClick={() => {
            void logout();
          }}
        >
          Sair
        </button>
      </header>
      <main>
        {screen === "vendas" && <PosScreen />}
        {screen === "produtos" && (
          <RoleGate allowed={MANAGERIAL_ROLES}>
            <ProductForm onCreated={() => setScreen("produtos")} />
          </RoleGate>
        )}
        {screen === "relatorios" && (
          <RoleGate allowed={MANAGERIAL_ROLES}>
            <p>Retaguarda gerencial (US17).</p>
          </RoleGate>
        )}
        {screen === "config" && (
          <RoleGate allowed={ADMIN_ROLES}>
            <p>Configurações da loja.</p>
          </RoleGate>
        )}
      </main>
    </div>
  );
}
