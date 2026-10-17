/**
 * Checkout do PDV (US07 Task 7.2, base para US08).
 *
 * Tudo aqui é PURO e testável: centavos inteiros de ponta a ponta, sem
 * float; strings decimais "x.xx" na saída. O PREÇO nunca vai no payload —
 * o backend re-congela o catálogo na transação (autoridade). Estas funções
 * são UX; o `POST /api/v1/sales` revalida toda a matemática.
 */

import type { CartItem } from "../stores/cart";
import { formatBRLFromCents } from "./masks";

export type PaymentMethod = "cash" | "pix" | "credit" | "debit";

export const PAYMENT_METHODS: ReadonlyArray<{
  id: PaymentMethod;
  label: string;
}> = [
  { id: "cash", label: "Dinheiro" },
  { id: "pix", label: "PIX" },
  { id: "credit", label: "Crédito" },
  { id: "debit", label: "Débito" },
];

export function paymentMethodLabel(method: PaymentMethod): string {
  return PAYMENT_METHODS.find((m) => m.id === method)?.label ?? method;
}

export interface DraftPayment {
  /** Chave da linha (React `key` e remoção). */
  key: string;
  method: PaymentMethod;
  /** Valor aplicado à dívida (centavos, ≤ falta no momento do "Adicionar"). */
  amount_cents: number;
  /** Entregue em dinheiro (≥ amount; a sobra vira troco no backend). */
  tendered_cents: number | null;
}

/** Centavos → string decimal exata da API: 2799 → "27.99" (sem float). */
export function centsToDecimalString(cents: number): string {
  const safe = Math.max(0, Math.trunc(cents));
  return `${Math.floor(safe / 100)}.${String(safe % 100).padStart(2, "0")}`;
}

/** R$ legível para a tela: 2799 → "R$ 27,99". */
export function formatCents(cents: number): string {
  return formatBRLFromCents(String(Math.max(0, Math.trunc(cents))));
}

export function paidCents(payments: DraftPayment[]): number {
  return payments.reduce((sum, p) => sum + p.amount_cents, 0);
}

/** Falta pagar (pode ser ≤ 0 — a UI trava o mínimo em R$ 0,00). */
export function remainingCents(
  totalCents: number,
  payments: DraftPayment[],
): number {
  return totalCents - paidCents(payments);
}

/** Troco = Σ(entregue − aplicado) do dinheiro; nunca negativo. */
export function changeCents(payments: DraftPayment[]): number {
  return payments.reduce(
    (sum, p) =>
      p.tendered_cents === null
        ? sum
        : sum + Math.max(0, p.tendered_cents - p.amount_cents),
    0,
  );
}

export type PlanPaymentResult =
  | { ok: true; payment: DraftPayment }
  | { ok: false; error: string };

/**
 * Converte "método + valor digitado" numa linha de pagamento:
 * - valor ≤ 0 ou não-inteiro é rejeitado (máscara já bloqueia letras);
 * - venda já quitada não aceita mais linha;
 * - fora do dinheiro, valor acima da falta é rejeitado (não há troco em
 *   PIX/cartão — o backend barraria com 422);
 * - dinheiro aceita valor acima da falta: aplica só o que falta e guarda o
 *   entregue cheio (`tendered_amount`) para o troco do backend.
 */
export function planPayment(
  totalCents: number,
  payments: DraftPayment[],
  method: PaymentMethod,
  enteredCents: number,
  key: string,
): PlanPaymentResult {
  if (!Number.isInteger(enteredCents) || enteredCents <= 0) {
    return { ok: false, error: "Informe um valor maior que zero." };
  }
  const remaining = remainingCents(totalCents, payments);
  if (remaining <= 0) {
    return { ok: false, error: "Venda já está totalmente paga." };
  }
  if (method !== "cash" && enteredCents > remaining) {
    return {
      ok: false,
      error: "Valor maior que o falta pagar — para receber troco use Dinheiro.",
    };
  }
  const amount = method === "cash" ? Math.min(enteredCents, remaining) : enteredCents;
  return {
    ok: true,
    payment: {
      key,
      method,
      amount_cents: amount,
      tendered_cents: method === "cash" ? enteredCents : null,
    },
  };
}

export interface SalePayloadItem {
  product_id: string;
  quantity: number;
  discount: string;
}

export interface SalePayloadPayment {
  method: PaymentMethod;
  amount: string;
  tendered_amount?: string;
}

export interface CreateSalePayload {
  items: SalePayloadItem[];
  payments: SalePayloadPayment[];
}

/**
 * Monta o corpo do `POST /api/v1/sales`: linha do carrinho → `discount`
 * da linha; pagamento → `amount`/`tendered_amount` decimais. Nenhum preço
 * unitário é enviado (o servidor usa o snapshot do catálogo dele).
 */
export function buildCreateSalePayload(
  items: CartItem[],
  payments: DraftPayment[],
): CreateSalePayload {
  return {
    items: items.map((item) => ({
      product_id: item.product_id,
      quantity: item.quantity,
      discount: centsToDecimalString(item.discount_cents),
    })),
    payments: payments.map((p) => ({
      method: p.method,
      amount: centsToDecimalString(p.amount_cents),
      ...(p.tendered_cents !== null
        ? { tendered_amount: centsToDecimalString(p.tendered_cents) }
        : {}),
    })),
  };
}
