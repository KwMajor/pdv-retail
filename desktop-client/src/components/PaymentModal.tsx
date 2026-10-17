/**
 * Modal de checkout com pagamentos fracionados (US07 Task 7.2, US08 Task 8.1).
 *
 * O operador marca pagamentos (Dinheiro/PIX/Crédito/Débito) até o
 * "Falta Pagar" zerar; "Finalizar" (= confirmação do pagamento) só habilita
 * então. O valor é digitado com máscara BRL bloqueante e o TROCO aparece em
 * tempo real enquanto o operador digita no Dinheiro (piso R$ 0,00) — o
 * estado/derivação vivem no store (`stores/checkout`), o backend revalida
 * tudo e é a autoridade (422/404).
 */

import { useState } from "react";
import {
  PAYMENT_METHODS,
  buildCreateSalePayload,
  formatCents,
  paidCents,
  paymentMethodLabel,
  remainingCents,
} from "../lib/checkout";
import { formatBRLFromCents } from "../lib/masks";
import { createSale, type Sale } from "../lib/sales";
import { selectTotals, useCart } from "../stores/cart";
import { selectLiveChange, useCheckout } from "../stores/checkout";

export function PaymentModal({
  onClose,
  onSuccess,
}: {
  onClose: () => void;
  onSuccess: (sale: Sale) => void;
}) {
  const items = useCart((s) => s.items);
  const clearCart = useCart((s) => s.clearCart);
  const totals = selectTotals(items);
  const checkout = useCheckout();

  const [error, setError] = useState<string | null>(null);
  const [serverError, setServerError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const paid = paidCents(checkout.payments);
  const remaining = remainingCents(totals.total_cents, checkout.payments);
  const liveChange = selectLiveChange(totals.total_cents, checkout);
  // No Dinheiro o troco aparece sempre (inclusive R$ 0,00 no valor exato);
  // fora dele, só se já houver troco acumulado de alguma linha em dinheiro.
  const showChange = checkout.method === "cash" || liveChange > 0;

  async function submit() {
    setSubmitting(true);
    setServerError(null);
    try {
      const sale = await createSale(
        buildCreateSalePayload(items, checkout.payments),
      );
      checkout.reset();
      clearCart();
      onSuccess(sale);
    } catch (e) {
      setServerError(e instanceof Error ? e.message : "Falha ao fechar a venda.");
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Fechamento de venda"
      className="card"
    >
      <h2>Fechamento de venda</h2>
      <dl aria-label="Resumo da venda">
        <div>
          <dt>Valor Total</dt>
          <dd>{formatCents(totals.total_cents)}</dd>
        </div>
        <div>
          <dt>Pago até agora</dt>
          <dd>{formatCents(paid)}</dd>
        </div>
        <div>
          <dt>Falta Pagar</dt>
          {/* Piso R$ 0,00: excedente de dinheiro é troco, não "falta negativa". */}
          <dd>{formatCents(Math.max(0, remaining))}</dd>
        </div>
      </dl>

      <fieldset>
        <legend>Forma de pagamento</legend>
        {PAYMENT_METHODS.map((m) => (
          <button
            key={m.id}
            type="button"
            aria-pressed={checkout.method === m.id}
            className={checkout.method === m.id ? "primary" : undefined}
            onClick={() => checkout.setMethod(m.id)}
          >
            {m.label}
          </button>
        ))}
      </fieldset>

      <label>
        Valor recebido
        <input
          value={
            checkout.entryDigits === ""
              ? ""
              : formatBRLFromCents(checkout.entryDigits)
          }
          inputMode="numeric"
          placeholder="R$ 0,00"
          onChange={(e) => checkout.setEntryDigits(e.target.value)}
        />
      </label>

      {showChange && (
        <p className="change-highlight" role="status">
          Troco: {formatCents(liveChange)}
        </p>
      )}

      <button
        type="button"
        onClick={() => {
          setError(null);
          const addError = checkout.addPayment(totals.total_cents);
          if (addError) setError(addError);
        }}
      >
        Adicionar pagamento
      </button>
      {error && <p role="alert">{error}</p>}

      {checkout.payments.length > 0 && (
        <ul aria-label="Pagamentos adicionados">
          {checkout.payments.map((p) => (
            <li key={p.key}>
              {paymentMethodLabel(p.method)} — {formatCents(p.amount_cents)}
              {p.tendered_cents !== null && p.tendered_cents > p.amount_cents
                ? ` (entregue ${formatCents(p.tendered_cents)})`
                : ""}{" "}
              <button
                type="button"
                aria-label={`Remover pagamento em ${paymentMethodLabel(p.method)}`}
                onClick={() => {
                  checkout.removePayment(p.key);
                  setError(null);
                }}
              >
                Remover
              </button>
            </li>
          ))}
        </ul>
      )}

      {serverError && <p role="alert">{serverError}</p>}
      <button
        className="primary"
        disabled={
          submitting || checkout.payments.length === 0 || remaining > 0
        }
        onClick={() => {
          void submit();
        }}
      >
        {submitting ? "Finalizando…" : "Finalizar venda"}
      </button>
      <button
        type="button"
        disabled={submitting}
        onClick={() => {
          checkout.reset();
          onClose();
        }}
      >
        Cancelar
      </button>
    </div>
  );
}
