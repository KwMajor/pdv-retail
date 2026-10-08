/**
 * Carrinho (US06 Task 6.1): store pura em memória, centavos inteiros,
 * totais derivados. Sem DOM, sem API — roda em jsdom e node.
 */
import { beforeEach, describe, expect, it } from "vitest";
import type { Product } from "../lib/products";
import { EMPTY_TOTALS, selectTotals, useCart } from "./cart";

function product(over: Partial<Product> = {}): Product {
  return {
    id: "11111111-1111-1111-1111-111111111111",
    store_id: "22222222-2222-2222-2222-222222222222",
    sku: "ARROZ-T1",
    barcode: "7891234567890",
    name: "Arroz T1 5kg",
    price: "10.00",
    cost: "7.00",
    ncm: "10063021",
    cest: null,
    cfop: "5102",
    icms_origin: "0",
    icms_rate: "18.00",
    is_active: true,
    ...over,
  };
}

beforeEach(() => {
  useCart.getState().clearCart();
});

describe("addItem", () => {
  it("adiciona 2 unidades de R$ 10 e o total reflete R$ 20", () => {
    useCart.getState().addItem(product(), 2);
    const { items } = useCart.getState();
    expect(items).toHaveLength(1);
    expect(items[0].quantity).toBe(2);
    expect(items[0].unit_price_cents).toBe(1000);
    expect(selectTotals(items).total_cents).toBe(2000);
  });

  it("bipar o mesmo produto soma na linha (sem duplicar)", () => {
    const api = useCart.getState();
    api.addItem(product());
    api.addItem(product(), 2);
    const { items } = useCart.getState();
    expect(items).toHaveLength(1);
    expect(items[0].quantity).toBe(3);
  });

  it("congela o fiscal no momento do bip", () => {
    const p = product({ ncm: "10063021", icms_rate: "18.00" });
    useCart.getState().addItem(p);
    // Catálogo muda depois — a linha mantém a foto antiga.
    p.ncm = "99999999";
    p.price = "99.99";
    const { items } = useCart.getState();
    expect(items[0].fiscal.ncm).toBe("10063021");
    expect(items[0].unit_price_cents).toBe(1000);
  });

  it("ignora quantidade inválida e preço inválido", () => {
    const api = useCart.getState();
    api.addItem(product(), 0);
    api.addItem(product(), 1.5);
    api.addItem(product({ price: "dez reais" }));
    api.addItem(product({ price: "10.999" }));
    expect(useCart.getState().items).toHaveLength(0);
  });
});

describe("applyDiscount", () => {
  it("R$ 5 de desconto no item de R$ 20 → total R$ 15", () => {
    const api = useCart.getState();
    api.addItem(product(), 2);
    const key = useCart.getState().items[0].key;
    api.applyDiscount(key, 500);
    const { items } = useCart.getState();
    expect(items[0].discount_cents).toBe(500);
    expect(selectTotals(items)).toEqual({
      lines: 1,
      quantity: 2,
      gross_cents: 2000,
      discount_cents: 500,
      total_cents: 1500,
    });
  });

  it("trava desconto em 0..bruto da linha", () => {
    const api = useCart.getState();
    api.addItem(product()); // R$ 10,00
    const key = useCart.getState().items[0].key;
    api.applyDiscount(key, -300);
    expect(useCart.getState().items[0].discount_cents).toBe(0);
    api.applyDiscount(key, 99999);
    expect(useCart.getState().items[0].discount_cents).toBe(1000);
  });

  it("bipar após desconto abre linha nova (desconto travado)", () => {
    const api = useCart.getState();
    api.addItem(product());
    const key = useCart.getState().items[0].key;
    api.applyDiscount(key, 100);
    api.addItem(product());
    const { items } = useCart.getState();
    expect(items).toHaveLength(2);
    expect(selectTotals(items).total_cents).toBe(1900);
  });
});

describe("removeItem / clearCart / selectTotals", () => {
  it("remove por chave e limpa tudo", () => {
    const api = useCart.getState();
    api.addItem(product());
    api.addItem(product({ id: "33333333-3333-3333-3333-333333333333" }));
    const [first] = useCart.getState().items;
    api.removeItem(first.key);
    expect(useCart.getState().items).toHaveLength(1);
    api.clearCart();
    expect(useCart.getState().items).toHaveLength(0);
    expect(selectTotals([])).toEqual(EMPTY_TOTALS);
  });

  it("removeItem com chave inexistente não mexe em nada", () => {
    useCart.getState().addItem(product());
    useCart.getState().removeItem("chave-que-nao-existe");
    expect(useCart.getState().items).toHaveLength(1);
  });

  it("soma múltiplas linhas sem float", () => {
    const api = useCart.getState();
    api.addItem(product({ price: "0.10" }));
    api.addItem(product({ id: "44444444-4444-4444-4444-444444444444", price: "0.20" }));
    // 0.1 + 0.2 em float daria 0.30000000000000004 — em centavos dá 30.
    expect(selectTotals(useCart.getState().items).total_cents).toBe(30);
  });
});
