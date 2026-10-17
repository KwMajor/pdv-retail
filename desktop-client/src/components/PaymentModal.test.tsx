/**
 * Comportamento do PaymentModal (US07 Task 7.2): resumo Total/Pago/Falta,
 * botões rápidos, máscara BRL, trava da finalização e payload/erros.
 */
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart, type CartItem } from "../stores/cart";
import { PaymentModal } from "./PaymentModal";

const SALE = {
  id: "sale-1",
  status: "closed",
  subtotal: "20.00",
  discount: "0.00",
  total: "20.00",
  change_amount: "30.00",
  created_at: "2026-10-17T12:00:00Z",
};

function seedCart(quantity = 2) {
  const item: CartItem = {
    key: "k1",
    product_id: "p1",
    barcode: null,
    name: "Arroz 5kg",
    unit_price_cents: 1000,
    quantity,
    discount_cents: 0,
    fiscal: {
      ncm: null,
      cest: null,
      cfop: null,
      icms_origin: null,
      icms_rate: "18.00",
    },
  };
  useCart.setState({ items: [item] });
}

function amountOf(label: string): string {
  const dt = screen.getByText(label, { selector: "dt" });
  const dd = dt.parentElement?.querySelector("dd")?.textContent ?? "";
  return dd.replace(/\u00a0/g, " ");
}

function renderModal() {
  const onSuccess = vi.fn();
  const onClose = vi.fn();
  render(<PaymentModal onClose={onClose} onSuccess={onSuccess} />);
  return { onSuccess, onClose };
}

describe("PaymentModal", () => {
  beforeEach(() => {
    useCart.setState({ items: [] });
    vi.unstubAllGlobals();
  });

  it("mostra o resumo e trava a finalização sem pagamento", () => {
    seedCart();
    renderModal();
    expect(amountOf("Valor Total")).toBe("R$ 20,00");
    expect(amountOf("Pago até agora")).toBe("R$ 0,00");
    expect(amountOf("Falta Pagar")).toBe("R$ 20,00");
    expect(screen.getByRole("button", { name: /Finalizar venda/ })).toBeDisabled();
  });

  it("adiciona PIX parcial e recalcula a falta", async () => {
    const user = userEvent.setup();
    seedCart();
    renderModal();

    await user.click(screen.getByRole("button", { name: "PIX" }));
    await user.type(screen.getByLabelText(/Valor recebido/), "400");
    await user.click(screen.getByRole("button", { name: /Adicionar pagamento/ }));

    expect(amountOf("Pago até agora")).toBe("R$ 4,00");
    expect(amountOf("Falta Pagar")).toBe("R$ 16,00");
    expect(screen.getByText(/PIX — R\$ 4,00/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Finalizar venda/ })).toBeDisabled();
  });

  it("dinheiro acima da falta vira troco, libera a venda e envia o payload sem preço", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () => Promise.resolve(SALE),
    });
    vi.stubGlobal("fetch", fetchSpy);
    seedCart();
    const { onSuccess } = renderModal();

    await user.type(screen.getByLabelText(/Valor recebido/), "5000");
    await user.click(screen.getByRole("button", { name: /Adicionar pagamento/ }));
    expect(amountOf("Falta Pagar")).toBe("R$ 0,00");
    expect(screen.getByText(/entregue R\$ 50,00/)).toBeInTheDocument();

    const finish = screen.getByRole("button", { name: /Finalizar venda/ });
    await waitFor(() => expect(finish).toBeEnabled());
    await user.click(finish);

    await waitFor(() => expect(onSuccess).toHaveBeenCalledWith(SALE));
    expect(useCart.getState().items).toEqual([]);
    const [path, init] = fetchSpy.mock.calls[0] as [string, RequestInit];
    expect(path).toBe("http://localhost:3000/api/v1/sales");
    expect(JSON.parse(init.body as string)).toEqual({
      items: [{ product_id: "p1", quantity: 2, discount: "0.00" }],
      payments: [{ method: "cash", amount: "20.00", tendered_amount: "50.00" }],
    });
  });

  it("recusa valor acima da falta fora do dinheiro", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);
    seedCart();
    renderModal();

    await user.click(screen.getByRole("button", { name: "PIX" }));
    await user.type(screen.getByLabelText(/Valor recebido/), "5000");
    await user.click(screen.getByRole("button", { name: /Adicionar pagamento/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent(/use Dinheiro/);
    expect(amountOf("Falta Pagar")).toBe("R$ 20,00");
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("mostra o erro do backend e não limpa o carrinho", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn().mockResolvedValue({
      ok: false,
      status: 422,
      json: () =>
        Promise.resolve({
          code: "UNPROCESSABLE",
          message: "estoque insuficiente para Arroz 5kg",
        }),
    });
    vi.stubGlobal("fetch", fetchSpy);
    seedCart();
    const { onSuccess } = renderModal();

    await user.type(screen.getByLabelText(/Valor recebido/), "2000");
    await user.click(screen.getByRole("button", { name: /Adicionar pagamento/ }));
    await user.click(screen.getByRole("button", { name: /Finalizar venda/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      "estoque insuficiente para Arroz 5kg",
    );
    expect(onSuccess).not.toHaveBeenCalled();
    expect(useCart.getState().items).toHaveLength(1);
  });

  it("máscara BRL bloqueia letras e formata os dígitos", async () => {
    const user = userEvent.setup();
    seedCart();
    renderModal();

    const input = screen.getByLabelText(/Valor recebido/);
    await user.type(input, "abc400");
    // Intl usa NBSP entre "R$" e o valor; normaliza antes de comparar.
    expect((input as HTMLInputElement).value.replace(/\u00a0/g, " ")).toBe(
      "R$ 4,00",
    );
  });
});
