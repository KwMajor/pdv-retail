/**
 * RoleGate (US02 Task 2.3): fora do papel = fora do DOM (não `display:none`).
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MANAGERIAL_ROLES } from "../auth/roles";
import { useSession } from "../stores/session";
import { RoleGate } from "./RoleGate";

describe("RoleGate", () => {
  it("caixa não vê conteúdo de gestão (nem no DOM)", () => {
    useSession.setState({
      user: {
        id: "u",
        store_id: "s",
        name: "Caixa",
        email: "c@loja",
        role: "cashier",
      },
      ready: true,
    });
    const { container } = render(
      <RoleGate allowed={MANAGERIAL_ROLES}>
        <button>Relatórios</button>
      </RoleGate>,
    );
    expect(screen.queryByRole("button")).toBeNull();
    expect(container.textContent).not.toContain("Relatórios");
  });

  it("gerente vê o conteúdo", () => {
    useSession.setState({
      user: {
        id: "u",
        store_id: "s",
        name: "Gerente",
        email: "g@loja",
        role: "manager",
      },
      ready: true,
    });
    render(
      <RoleGate allowed={MANAGERIAL_ROLES}>
        <button>Relatórios</button>
      </RoleGate>,
    );
    expect(screen.getByRole("button", { name: "Relatórios" })).toBeInTheDocument();
  });
});
