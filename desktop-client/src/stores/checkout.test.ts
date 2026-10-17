/**
 * Checkout (US08 Task 8.1): troco em tempo real derivado no store — nunca
 * armazenado, nunca negativo; estado só em memória. Sem DOM, sem API.
 */
import { beforeEach, describe, expect, it } from "vitest";
import { entryCentsOf, selectLiveChange, useCheckout } from "./checkout";

beforeEach(() => {
  useCheckout.getState().reset();
});

describe("setEntryDigits", () => {
  it("máscara bloqueante: só dígitos e no máximo 12", () => {
    useCheckout.getState().setEntryDigits("12a3.45");
    expect(useCheckout.getState().entryDigits).toBe("12345");
    useCheckout.getState().setEntryDigits("1".repeat(20));
    expect(useCheckout.getState().entryDigits).toBe("1".repeat(12));
  });

  it("entryCentsOf trata vazio como zero", () => {
    expect(entryCentsOf({ entryDigits: "" })).toBe(0);
    useCheckout.getState().setEntryDigits("123");
    expect(entryCentsOf(useCheckout.getState())).toBe(123);
  });
});

describe("addPayment", () => {
  it("dinheiro acima da falta aplica só a falta, guarda o entregue e limpa o input", () => {
    useCheckout.getState().setEntryDigits("5000");
    expect(useCheckout.getState().addPayment(2000)).toBeNull();
    const { payments, entryDigits } = useCheckout.getState();
    expect(payments).toHaveLength(1);
    expect(payments[0]).toMatchObject({
      method: "cash",
      amount_cents: 2000,
      tendered_cents: 5000,
    });
    expect(entryDigits).toBe("");
  });

  it("devolve o erro da validação pura e não persiste nada", () => {
    useCheckout.getState().setMethod("pix");
    useCheckout.getState().setEntryDigits("5000");
    expect(useCheckout.getState().addPayment(2000)).toMatch(/use Dinheiro/);
    expect(useCheckout.getState().payments).toHaveLength(0);
    expect(useCheckout.getState().entryDigits).toBe("5000");
  });

  it("venda já paga não aceita nova linha", () => {
    useCheckout.getState().setEntryDigits("2000");
    expect(useCheckout.getState().addPayment(2000)).toBeNull();
    useCheckout.getState().setEntryDigits("100");
    expect(useCheckout.getState().addPayment(2000)).toMatch(/totalmente paga/);
  });
});

describe("removePayment", () => {
  it("remove a linha e o derivado se recompõe", () => {
    useCheckout.getState().setEntryDigits("2000");
    useCheckout.getState().addPayment(2000);
    const key = useCheckout.getState().payments[0].key;
    useCheckout.getState().removePayment(key);
    expect(useCheckout.getState().payments).toHaveLength(0);
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(0);
  });
});

describe("selectLiveChange (DoD US08)", () => {
  it("falta 20 + digitar 50 em dinheiro → troco 30 em tempo real, sem adicionar", () => {
    useCheckout.getState().setEntryDigits("5000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(3000);
    // Projeção pura: nada foi persistido ainda.
    expect(useCheckout.getState().payments).toHaveLength(0);
  });

  it("valor exato → R$ 0,00; valor menor → R$ 0,00 (nunca negativo)", () => {
    useCheckout.getState().setEntryDigits("2000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(0);
    useCheckout.getState().setEntryDigits("1000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(0);
  });

  it("fora do dinheiro não há troco (PIX/cartão não dão sobra)", () => {
    useCheckout.getState().setMethod("pix");
    useCheckout.getState().setEntryDigits("5000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(0);
  });

  it("soma o troco acumulado das linhas com a projeção pendente", () => {
    // PIX R$ 4,00 marcado, falta R$ 16,00, digitando R$ 50,00 em dinheiro.
    useCheckout.getState().setMethod("pix");
    useCheckout.getState().setEntryDigits("400");
    useCheckout.getState().addPayment(2000);
    useCheckout.getState().setMethod("cash");
    useCheckout.getState().setEntryDigits("5000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(3400);
  });

  it("com a venda já paga, digitar a mais não inventa troco", () => {
    useCheckout.getState().setEntryDigits("2000");
    useCheckout.getState().addPayment(2000);
    useCheckout.getState().setEntryDigits("5000");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(0);
  });

  it("troco acumulado sobrevive à troca de método (operador precisa vê-lo)", () => {
    useCheckout.getState().setEntryDigits("5000");
    useCheckout.getState().addPayment(2000); // troco R$ 30,00 acumulado
    useCheckout.getState().setMethod("pix");
    expect(selectLiveChange(2000, useCheckout.getState())).toBe(3000);
  });
});

describe("reset", () => {
  it("volta ao estado inicial (dinheiro, sem linhas e sem entrada)", () => {
    useCheckout.getState().setMethod("debit");
    useCheckout.getState().setEntryDigits("5000");
    useCheckout.getState().addPayment(2000);
    useCheckout.getState().reset();
    expect(useCheckout.getState()).toMatchObject({
      method: "cash",
      entryDigits: "",
      payments: [],
    });
  });
});
