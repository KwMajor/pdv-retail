/**
 * Modal de checkout com pagamentos fracionados (US07 Task 7.2).
 *
 * O operador adiciona pagamentos (Dinheiro/PIX/Crédito/Débito) até o
 * "Falta Pagar" zerar; "Finalizar" só habilita então. O valor é digitado
 * com máscara BRL bloqueante (só dígitos viram centavos) e validado
 * localmente — o backend revalida tudo e é a autoridade (422/404).
 * O troco em tempo real "em destaque" é a US08; aqui o dinheiro acima da
 * falta já vai como `tendered_amount` cheio.
 */

import { useState } from "react";
import {
  PAYMENT_METHODS,
  buildCreateSalePayload,
  formatCents,
  paidCents,
  paymentMethodLabel,
  planPayment,
  remainingCents,
  type DraftPayment,
  type PaymentMethod,
} from "../lib/checkout";
import { formatBRLFromCents, onlyDigits } from "../lib/masks";
import { createSale, type Sale } from "../lib/sales";
import { selectTotals, useCart } from "../stores/cart";

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

  const [method, setMethod] = useState<PaymentMethod>("cash");
  const [valueCents, setValueCents] = useState("");
  const [payments, setPayments] = useState<DraftPayment[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [serverError, setServerError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const paid = paidCents(payments);
  const remaining = remainingCents(totals.total_cents, payments);

  function addPayment() {
    const result = planPayment(
      totals.total_cents,
      payments,
      method,
      valueCents === "" ? 0 : Number(valueCents),
      crypto.randomUUID(),
    );
    if (!result.ok) {
      setError(result.error);
      return;
    }
    setPayments((current) => [...current, result.payment]);
    setValueCents("");
    setError(null);
  }

  function removePayment(key: string) {
    setPayments((current) => current.filter((p) => p.key !== key));
    setError(null);
  }

  async function submit() {
    setSubmitting(true);
    setServerError(null);
    try {
      const sale = await createSale(buildCreateSalePayload(items, payments));
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
            aria-pressed={method === m.id}
            className={method === m.id ? "primary" : undefined}
            onClick={() => setMethod(m.id)}
          >
            {m.label}
          </button>
        ))}
      </fieldset>

      <label>
        Valor recebido
        <input
          value={valueCents === "" ? "" : formatBRLFromCents(valueCents)}
          inputMode="numeric"
          placeholder="R$ 0,00"
          onChange={(e) => setValueCents(onlyDigits(e.target.value).slice(0, 12))}
        />
      </label>
      <button
        type="button"
        onClick={() => {
          setError(null);
          addPayment();
        }}
      >
        Adicionar pagamento
      </button>
      {error && <p role="alert">{error}</p>}

      {payments.length > 0 && (
        <ul aria-label="Pagamentos adicionados">
          {payments.map((p) => (
            <li key={p.key}>
              {paymentMethodLabel(p.method)} — {formatCents(p.amount_cents)}
              {p.tendered_cents !== null && p.tendered_cents > p.amount_cents
                ? ` (entregue ${formatCents(p.tendered_cents)})`
                : ""}{" "}
              <button
                type="button"
                aria-label={`Remover pagamento em ${paymentMethodLabel(p.method)}`}
                onClick={() => removePayment(p.key)}
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
        disabled={submitting || payments.length === 0 || remaining > 0}
        onClick={() => {
          void submit();
        }}
      >
        {submitting ? "Finalizando…" : "Finalizar venda"}
      </button>
      <button type="button" disabled={submitting} onClick={onClose}>
        Cancelar
      </button>
    </div>
  );
}
