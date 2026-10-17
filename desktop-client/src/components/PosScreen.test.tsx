/**
 * Comportamento da PosScreen (US06/US07): grade do carrinho, remoção,
 * abertura do checkout e confirmação com troco após a venda.
 */
import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart, type CartItem } from "../stores/cart";
import { PosScreen } from "./PosScreen";

function seedCart(items: Array<Partial<CartItem>>) {
  useCart.setState({
    items: items.map((over, index) => ({
      key: `k${index}`,
      product_id: `p${index}`,
      barcode: null,
      name: "Arroz 5kg",
      unit_price_cents: 1000,
      quantity: 2,
      discount_cents: 0,
      fiscal: {
        ncm: null,
        cest: null,
        cfop: null,
        icms_origin: null,
        icms_rate: "18.00",
      },
      ...over,
    })),
  });
}

describe("PosScreen", () => {
  beforeEach(() => {
    useCart.setState({ items: [] });
    vi.unstubAllGlobals();
  });

  it("sem itens, orienta o bip e trava a finalização", () => {
    render(<PosScreen />);
    expect(
      screen.getByText(/Bipe um código de barras/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Finalizar venda/ }),
    ).toBeDisabled();
  });

  it("mostra a linha, o desconto e o total do carrinho", () => {
    seedCart([{ quantity: 3, discount_cents: 50 }]);
    render(<PosScreen />);

    expect(screen.getByRole("cell", { name: "Arroz 5kg" })).toBeInTheDocument();
    expect(screen.getByText(/3 itens/)).toBeInTheDocument();
    expect(screen.getAllByText("R$ 29,50").length).toBeGreaterThan(0);
    expect(
      screen.getByRole("button", { name: /Finalizar venda/ }),
    ).toBeEnabled();
  });

  it("remove a linha e volta ao estado vazio", async () => {
    const user = userEvent.setup();
    seedCart([{}]);
    render(<PosScreen />);

    await user.click(screen.getByRole("button", { name: "Remover Arroz 5kg" }));
    expect(
      screen.getByText(/Bipe um código de barras/),
    ).toBeInTheDocument();
  });

  it("finalizar abre o modal de fechamento de venda", async () => {
    const user = userEvent.setup();
    seedCart([{}]);
    render(<PosScreen />);

    await user.click(screen.getByRole("button", { name: /Finalizar venda/ }));
    expect(
      screen.getByRole("dialog", { name: "Fechamento de venda" }),
    ).toBeInTheDocument();
  });

  it("venda concluída mostra confirmação com troco", async () => {
    const user = userEvent.setup();
    const fetchSpy = vi.fn().mockResolvedValue({
      ok: true,
      json: () =>
        Promise.resolve({
          id: "sale-1",
          status: "closed",
          subtotal: "20.00",
          discount: "0.00",
          total: "20.00",
          change_amount: "10.00",
          created_at: "2026-10-17T12:00:00Z",
        }),
    });
    vi.stubGlobal("fetch", fetchSpy);
    seedCart([{}]);
    render(<PosScreen />);

    await user.click(screen.getByRole("button", { name: /Finalizar venda/ }));
    await user.type(screen.getByLabelText(/Valor recebido/), "3000");
    await user.click(screen.getByRole("button", { name: /Adicionar pagamento/ }));
    // Há dois botões "Finalizar venda" (tela e modal): confirma o do modal.
    const dialog = screen.getByRole("dialog", { name: "Fechamento de venda" });
    await user.click(
      within(dialog).getByRole("button", { name: /Finalizar venda/ }),
    );

    const status = await screen.findByRole("status");
    expect(status).toHaveTextContent(/Venda sale-1 concluída/);
    expect(status).toHaveTextContent(/troco R\$ 10,00/);
    expect(
      screen.queryByRole("dialog", { name: "Fechamento de venda" }),
    ).not.toBeInTheDocument();
    expect(useCart.getState().items).toEqual([]);
  });
});
