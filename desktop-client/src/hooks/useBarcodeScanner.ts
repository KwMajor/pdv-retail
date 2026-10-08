/**
 * Leitor de código de barras (US06 Task 6.2).
 *
 * Leitores USB fingem ser teclado: digitam RÁPIDO e apertam `Enter`. O hook
 * escuta `keydown` globalmente e considera "bip" a sequência que:
 * - tem só dígitos (`key` de 1 char, sem Ctrl/Alt/Meta),
 * - chega com intervalo entre teclas ≤ `maxIntervalMs` (padrão 50ms —
 *   humano rápido digita a ~80ms+/tecla, leitor a ~5–15ms),
 * - termina em `Enter` com ao menos `minLength` dígitos (padrão 4).
 * Qualquer tecla lenta quebra a sequência (volta a ser "digitação humana").
 *
 * Alvos editáveis (`input`/`textarea`/`select`/contentEditable) são
 * ignorados: com foco num campo, o bip deve ir para o campo, não para o
 * carrinho (evita item duplicado: digitado + capturado).
 */

import { useEffect, useState } from "react";
import { findProductByBarcode } from "../lib/products";
import { useCart } from "../stores/cart";

export interface BarcodeScannerOptions {
  onScan: (code: string) => void;
  /** Dígitos mínimos p/ validar o bip (padrão 4). */
  minLength?: number;
  /** Intervalo máximo entre teclas em ms (padrão 50). */
  maxIntervalMs?: number;
  /** Desliga o listener sem desmontar (padrão true). */
  enabled?: boolean;
}

function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  if (target.isContentEditable) return true;
  const tag = target.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export function useBarcodeScanner({
  onScan,
  minLength = 4,
  maxIntervalMs = 50,
  enabled = true,
}: BarcodeScannerOptions): void {
  useEffect(() => {
    if (!enabled) return;
    let buffer = "";
    let lastAt = 0;

    const onKeyDown = (event: KeyboardEvent): void => {
      if (event.ctrlKey || event.metaKey || event.altKey) return;
      if (isEditable(event.target)) return;
      const now = Date.now();

      if (event.key === "Enter") {
        // Fim do bip: sequência rápida + longa + só dígitos = código.
        if (buffer.length >= minLength && /^\d+$/.test(buffer)) {
          onScan(buffer);
        }
        buffer = "";
        lastAt = 0;
        return;
      }
      // Só dígito isolado compõe o buffer; resto (Shift, setas, F1…) quebra.
      if (event.key.length !== 1 || event.key < "0" || event.key > "9") {
        buffer = "";
        lastAt = 0;
        return;
      }
      // Tecla lenta = humano digitando: recomeça a sequência desta tecla.
      if (lastAt !== 0 && now - lastAt > maxIntervalMs) {
        buffer = "";
      }
      buffer += event.key;
      lastAt = now;
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onScan, minLength, maxIntervalMs, enabled]);
}

/**
 * Cola pronta "bip → carrinho": busca o produto na API e dá `addItem`
 * automaticamente. `lastError` alimenta a UI ("produto não cadastrado").
 */
export function useScanToCart(options?: {
  minLength?: number;
  maxIntervalMs?: number;
  enabled?: boolean;
}): { lastError: string | null; clearError: () => void } {
  const [lastError, setLastError] = useState<string | null>(null);
  const addItem = useCart((s) => s.addItem);

  useBarcodeScanner({
    minLength: options?.minLength,
    maxIntervalMs: options?.maxIntervalMs,
    enabled: options?.enabled,
    onScan: (code) => {
      void findProductByBarcode(code).then((product) => {
        if (product) {
          addItem(product);
          setLastError(null);
        } else {
          setLastError(`Produto ${code} não cadastrado.`);
        }
      });
    },
  });

  return { lastError, clearError: () => setLastError(null) };
}
