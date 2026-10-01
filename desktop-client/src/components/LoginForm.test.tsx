/**
 * LoginForm: validação local antes do fetch + erro genérico no 401.
 */
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { LoginForm } from "./LoginForm";

describe("LoginForm", () => {
  it("campos vazios mostram erro sem chamar a API", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    render(<LoginForm />);
    await user.click(screen.getByRole("button", { name: "Entrar" }));
    expect(screen.getByRole("alert")).toHaveTextContent(/Preencha/);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("credencial recusada mostra erro genérico", async () => {
    const user = userEvent.setup();
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue({ ok: false, status: 401, json: () => Promise.resolve({}) }),
    );
    render(<LoginForm />);
    await user.type(screen.getByLabelText(/ID da loja/), "22222222-2222-2222-2222-222222222222");
    await user.type(screen.getByLabelText(/Email/), "c@loja");
    await user.type(screen.getByLabelText(/Senha/), "errada123");
    await user.click(screen.getByRole("button", { name: "Entrar" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(/inválidos/);
  });
});
