/**
 * Fechamento de venda (US08 Task 8.1).
 *
 * Estado do checkout em MEMÓRIA (Zustand, nunca localStorage):
 * - `method` + `entryDigits` = pagamento que o operador está marcando agora;
 * - `payments` = linhas já confirmadas (autoridade final é o backend, US07);
 * - o TROCO é DERIVADO (`selectLiveChange`), nunca armazenado: acumulado
 *   das linhas de dinheiro + projeção do valor sendo digitado, com piso
 *   R$ 0,00 (troco negativo é impossível, por construção).
 */

import { create } from "zustand";
import {
  changeCents,
  planPayment,
  remainingCents,
  type DraftPayment,
  type PaymentMethod,
} from "../lib/checkout";

export interface CheckoutState {
  method: PaymentMethod;
  /** Dígitos crus do input (a máscara BRL é aplicada no render). */
  entryDigits: string;
  payments: DraftPayment[];
  setMethod: (method: PaymentMethod) => void;
  /** Máscara bloqueante: só dígitos, teto de 12 (R$ 9.999.999.999,99). */
  setEntryDigits: (digits: string) => void;
  /** Marca o pagamento digitado; devolve o erro p/ UI ou null no sucesso. */
  addPayment: (totalCents: number) => string | null;
  removePayment: (key: string) => void;
  reset: () => void;
}

/** Valor digitado em centavos ("" = 0; os dígitos já vêm validados). */
export function entryCentsOf(
  state: Pick<CheckoutState, "entryDigits">,
): number {
  return state.entryDigits === "" ? 0 : Number(state.entryDigits);
}

export const useCheckout = create<CheckoutState>()((set, get) => ({
  method: "cash",
  entryDigits: "",
  payments: [],

  setMethod: (method) => set({ method }),

  setEntryDigits: (digits) =>
    set({ entryDigits: digits.replace(/\D/g, "").slice(0, 12) }),

  addPayment: (totalCents) => {
    const { method, payments } = get();
    const result = planPayment(
      totalCents,
      payments,
      method,
      entryCentsOf(get()),
      crypto.randomUUID(),
    );
    if (!result.ok) return result.error;
    set({ payments: [...payments, result.payment], entryDigits: "" });
    return null;
  },

  removePayment: (key) =>
    set((state) => ({ payments: state.payments.filter((p) => p.key !== key) })),

  reset: () => set({ method: "cash", entryDigits: "", payments: [] }),
}));

/**
 * Troco vivo para a tela: acumulado das linhas de dinheiro + projeção do
 * valor sendo digitado (só no Dinheiro e só enquanto falta pagar — PIX/cartão
 * não têm troco e venda quitada não aceita linha nova).
 */
export function selectLiveChange(
  totalCents: number,
  state: Pick<CheckoutState, "method" | "entryDigits" | "payments">,
): number {
  const remaining = remainingCents(totalCents, state.payments);
  const pending =
    state.method === "cash" && remaining > 0
      ? Math.max(0, entryCentsOf(state) - remaining)
      : 0;
  return changeCents(state.payments) + pending;
}
