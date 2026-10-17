/**
 * Frente de caixa (US06 bip → US07 fechamento).
 *
 * Fluxo do operador: bipa o código de barras (o hook captura a rajada do
 * leitor e joga no carrinho), revisa a grade e "Finalizar venda" abre o
 * modal de pagamentos fracionados. O carrinho vive só em memória; a venda
 * só existe quando o backend confirma (autoridade).
 */

import { useState } from "react";
import { useScanToCart } from "../hooks/useBarcodeScanner";
import { formatCents } from "../lib/checkout";
import { decimalStringToCents } from "../lib/masks";
import type { Sale } from "../lib/sales";
import { selectTotals, useCart } from "../stores/cart";
import { PaymentModal } from "./PaymentModal";

export function PosScreen() {
  const items = useCart((s) => s.items);
  const removeItem = useCart((s) => s.removeItem);
  const totals = selectTotals(items);
  const { lastError, clearError } = useScanToCart();
  const [checkoutOpen, setCheckoutOpen] = useState(false);
  const [lastSale, setLastSale] = useState<Sale | null>(null);

  const lastChange = lastSale
    ? (decimalStringToCents(lastSale.change_amount) ?? 0)
    : 0;

  return (
    <section aria-label="Frente de caixa">
      {lastError && (
        <p role="alert">
          {lastError}{" "}
          <button type="button" onClick={clearError}>
            Dispensar
          </button>
        </p>
      )}

      {lastSale && (
        <p role="status">
          Venda {lastSale.id.slice(0, 8)} concluída — troco {formatCents(lastChange)}.
        </p>
      )}

      {items.length === 0 ? (
        <p>Bipe um código de barras para começar a venda.</p>
      ) : (
        <table>
          <thead>
            <tr>
              <th>Item</th>
              <th>Qtd</th>
              <th>Unitário</th>
              <th>Desconto</th>
              <th>Total</th>
              <th aria-label="Ações" />
            </tr>
          </thead>
          <tbody>
            {items.map((item) => (
              <tr key={item.key}>
                <td>{item.name}</td>
                <td>{item.quantity}</td>
                <td>{formatCents(item.unit_price_cents)}</td>
                <td>{formatCents(item.discount_cents)}</td>
                <td>
                  {formatCents(
                    item.unit_price_cents * item.quantity - item.discount_cents,
                  )}
                </td>
                <td>
                  <button
                    type="button"
                    aria-label={`Remover ${item.name}`}
                    onClick={() => removeItem(item.key)}
                  >
                    Remover
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}

      <p>
        {totals.quantity} {totals.quantity === 1 ? "item" : "itens"} · desconto{" "}
        {formatCents(totals.discount_cents)} · total{" "}
        <strong>{formatCents(totals.total_cents)}</strong>
      </p>

      <button
        className="primary"
        disabled={items.length === 0}
        onClick={() => setCheckoutOpen(true)}
      >
        Finalizar venda
      </button>

      {checkoutOpen && (
        <PaymentModal
          onClose={() => setCheckoutOpen(false)}
          onSuccess={(sale) => {
            setLastSale(sale);
            setCheckoutOpen(false);
          }}
        />
      )}
    </section>
  );
}
