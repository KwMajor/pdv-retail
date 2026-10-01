/**
 * Comportamento do ProductForm (US03 Task 3.3): erro visível, submit
 * bloqueado com dado inválido, payload correto no sucesso.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProductForm } from "./ProductForm";

async function fillValid(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByLabelText(/Nome/), "Arroz T1 5kg");
  await user.type(screen.getByLabelText(/^SKU/), "ARROZ-T1-5KG");
  await user.type(screen.getByLabelText(/Preço de venda/), "2799");
  await user.click(screen.getByRole("tab", { name: "Fiscal" }));
  await user.type(screen.getByLabelText(/^NCM/), "10063021");
  await user.type(screen.getByLabelText(/Alíquota/), "18");
}

describe("ProductForm", () => {
  beforeEach(() => {
    vi.unstubAllGlobals();
  });

  it("alterna entre abas Geral e Fiscal", async () => {
    const user = userEvent.setup();
    render(<ProductForm onCreated={() => {}} />);
    expect(screen.getByLabelText(/Nome/)).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Fiscal" }));
    expect(screen.getByLabelText(/^NCM/)).toBeInTheDocument();
  });

  it("NCM inválido mostra erro e não chama a API", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn().mockRejectedValue(new Error("não deve chamar"));
    vi.stubGlobal("fetch", fetchSpy);
    render(<ProductForm onCreated={() => {}} />);

    await user.click(screen.getByRole("tab", { name: "Fiscal" }));
    await user.type(screen.getByLabelText(/^NCM/), "123");
    await user.click(screen.getByRole("button", { name: /Salvar/ }));
    expect(await screen.findByText(/8 dígitos/)).toBeInTheDocument();
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("dados válidos habilitam e enviam o payload", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve({ id: "1" }),
    });
    vi.stubGlobal("fetch", fetchSpy);
    const onCreated = vi.fn();
    render(<ProductForm onCreated={onCreated} />);

    await fillValid(user);
    const save = screen.getByRole("button", { name: /Salvar/ });
    await waitFor(() => expect(save).toBeEnabled());
    await user.click(save);

    await waitFor(() => expect(onCreated).toHaveBeenCalled());
    const [, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as Record<string, unknown>;
    expect(body).toMatchObject({
      name: "Arroz T1 5kg",
      sku: "ARROZ-T1-5KG",
      ncm: "10063021",
      price: 27.99,
    });
  });
});
