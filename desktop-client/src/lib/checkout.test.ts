/**
 * Validadores puros do checkout (US07 Task 7.2): centavos ↔ decimal,
 * regras de troco por método e montagem do payload sem preço.
 */
import { describe, expect, it } from "vitest";
import type { CartItem } from "../stores/cart";
import {
  PAYMENT_METHODS,
  buildCreateSalePayload,
  centsToDecimalString,
  changeCents,
  paidCents,
  paymentMethodLabel,
  planPayment,
  remainingCents,
  type DraftPayment,
} from "./checkout";

const item = (over: Partial<CartItem> = {}): CartItem => ({
  key: "k1",
  product_id: "p1",
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
});

const cash = (amount: number, tendered: number): DraftPayment => ({
  key: `cash-${amount}-${tendered}`,
  method: "cash",
  amount_cents: amount,
  tendered_cents: tendered,
});

const pix = (amount: number): DraftPayment => ({
  key: `pix-${amount}`,
  method: "pix",
  amount_cents: amount,
  tendered_cents: null,
});

describe("centsToDecimalString", () => {
  it("converte centavos em decimal com 2 casas, sem float", () => {
    expect(centsToDecimalString(2799)).toBe("27.99");
    expect(centsToDecimalString(5)).toBe("0.05");
    expect(centsToDecimalString(100)).toBe("1.00");
    expect(centsToDecimalString(0)).toBe("0.00");
  });

  it("nunca deixa valor negativo vazar para a API", () => {
    expect(centsToDecimalString(-10)).toBe("0.00");
  });
});

describe("métodos de pagamento", () => {
  it("expõe os quatro métodos do enum do backend", () => {
    expect(PAYMENT_METHODS.map((m) => m.id)).toEqual([
      "cash",
      "pix",
      "credit",
      "debit",
    ]);
    expect(paymentMethodLabel("cash")).toBe("Dinheiro");
    expect(paymentMethodLabel("credit")).toBe("Crédito");
  });
});

describe("planPayment", () => {
  it("rejeita valor zero, negativo ou fracionado", () => {
    expect(planPayment(2000, [], "cash", 0, "k").ok).toBe(false);
    expect(planPayment(2000, [], "cash", -5, "k").ok).toBe(false);
    expect(planPayment(2000, [], "cash", 10.5, "k").ok).toBe(false);
  });

  it("rejeita continuação quando a venda já está paga", () => {
    const r = planPayment(2000, [pix(2000)], "cash", 100, "k");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/totalmente paga/);
  });

  it("PIX/Crédito/Débito não aceitam valor acima da falta (sem troco)", () => {
    const r = planPayment(2000, [], "pix", 5000, "k");
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/use Dinheiro/);
  });

  it("PIX aplica o valor exato e não carrega tendered", () => {
    const r = planPayment(2000, [], "pix", 2000, "k");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.payment.amount_cents).toBe(2000);
      expect(r.payment.tendered_cents).toBeNull();
    }
  });

  it("dinheiro acima da falta aplica só a falta e guarda o entregue", () => {
    const r = planPayment(2000, [], "cash", 5000, "k");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.payment.amount_cents).toBe(2000);
      expect(r.payment.tendered_cents).toBe(5000);
    }
  });

  it("dinheiro exato zera a falta sem troco", () => {
    const r = planPayment(2000, [], "cash", 2000, "k");
    expect(r.ok).toBe(true);
    if (r.ok) {
      expect(r.payment.amount_cents).toBe(2000);
      expect(r.payment.tendered_cents).toBe(2000);
    }
  });
});

describe("matemática dos pagamentos", () => {
  it("soma aplicado, falta e troco (Σ sobras do dinheiro)", () => {
    const payments = [pix(400), cash(1600, 2000)];
    expect(paidCents(payments)).toBe(2000);
    expect(remainingCents(2000, payments)).toBe(0);
    expect(changeCents(payments)).toBe(400);
  });

  it("troco nunca fica negativo", () => {
    expect(changeCents([cash(1000, 500)])).toBe(0);
    expect(changeCents([pix(1000)])).toBe(0);
  });
});

describe("buildCreateSalePayload", () => {
  it("envia item sem preço (backend re-congela) e desconto da linha", () => {
    const payload = buildCreateSalePayload([item({ discount_cents: 50 })], []);
    expect(payload.items).toEqual([
      { product_id: "p1", quantity: 2, discount: "0.50" },
    ]);
  });

  it("inclui tendered_amount só no dinheiro", () => {
    const payload = buildCreateSalePayload(
      [item()],
      [pix(400), cash(1600, 2000)],
    );
    expect(payload.payments).toEqual([
      { method: "pix", amount: "4.00" },
      { method: "cash", amount: "16.00", tendered_amount: "20.00" },
    ]);
  });
});
