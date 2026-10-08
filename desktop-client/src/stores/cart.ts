/**
 * Carrinho do PDV (US06 Task 6.1).
 *
 * Zustand em MEMÓRIA (nunca localStorage: preços e snapshots fiscais não
 * podem sobreviver em disco do navegador):
 * - dinheiro em CENTAVOS INTEIROS de ponta a ponta (`unit_price_cents`,
 *   `discount_cents`) — `0.1 + 0.2 !== 0.3` em float não nos atinge;
 * - totais DERIVADOS (`selectTotals`), nunca armazenados — impossível
 *   divergir do array de itens;
 * - cada item congela o fiscal do produto (`fiscal`) no momento do bip
 *   (congelamento fiscal: mudanças futuras no catálogo não alteram a venda).
 * O backend revalida tudo no fechamento (US07) — aqui é UX + performance.
 */

import { create } from "zustand";
import { decimalStringToCents } from "../lib/masks";
import type { Product } from "../lib/products";

/** Foto do fiscal no momento do bip (strings exatas da API, sem matemática). */
export interface CartFiscalSnapshot {
  ncm: string | null;
  cest: string | null;
  cfop: string | null;
  icms_origin: string | null;
  icms_rate: string;
}

export interface CartItem {
  /** Chave da linha (estável p/ React `key` e remoção). */
  key: string;
  product_id: string;
  barcode: string | null;
  name: string;
  /** Preço unitário em centavos (inteiro ≥ 0). */
  unit_price_cents: number;
  /** Quantidade inteira ≥ 1. */
  quantity: number;
  /** Desconto da linha em centavos (0 ≤ d ≤ unit_price_cents*quantity). */
  discount_cents: number;
  fiscal: CartFiscalSnapshot;
}

export interface CartTotals {
  lines: number;
  quantity: number;
  /** Σ unit*qty (bruto, sem desconto). */
  gross_cents: number;
  discount_cents: number;
  /** A cobrar: bruto − desconto. */
  total_cents: number;
}

export const EMPTY_TOTALS: CartTotals = {
  lines: 0,
  quantity: 0,
  gross_cents: 0,
  discount_cents: 0,
  total_cents: 0,
};

/** Puro e testável: deriva os totais do array (nunca armazena). */
export function selectTotals(items: CartItem[]): CartTotals {
  let quantity = 0;
  let gross_cents = 0;
  let discount_cents = 0;
  for (const item of items) {
    quantity += item.quantity;
    gross_cents += item.unit_price_cents * item.quantity;
    discount_cents += item.discount_cents;
  }
  return {
    lines: items.length,
    quantity,
    gross_cents,
    discount_cents,
    total_cents: gross_cents - discount_cents,
  };
}

function snapshotOf(product: Product): CartFiscalSnapshot {
  return {
    ncm: product.ncm,
    cest: product.cest,
    cfop: product.cfop,
    icms_origin: product.icms_origin,
    icms_rate: product.icms_rate,
  };
}

interface CartState {
  items: CartItem[];
  /** Adiciona (ou soma na linha existente do mesmo produto sem desconto). */
  addItem: (product: Product, quantity?: number) => void;
  removeItem: (key: string) => void;
  /** Desconto da linha em centavos (trava em 0..bruto da linha). */
  applyDiscount: (key: string, discountCents: number) => void;
  clearCart: () => void;
}

function isValidQuantity(quantity: number): boolean {
  return Number.isInteger(quantity) && quantity >= 1;
}

export const useCart = create<CartState>()((set) => ({
  items: [],

  addItem: (product, quantity = 1) => {
    if (!isValidQuantity(quantity)) return;
    // Preço inválido nunca entra no carrinho (backend barraria na venda).
    const unit_price_cents = decimalStringToCents(product.price);
    if (unit_price_cents === null) return;
    set((state) => {
      // Linha existente do mesmo produto E sem desconto manual: soma a
      // quantidade (bipar 2× = 2 linhas? não — 1 linha com qty 2). Linha com
      // desconto é "travada": bipar de novo abre linha nova, preservando o
      // desconto já autorizado.
      const existing = state.items.find(
        (item) => item.product_id === product.id && item.discount_cents === 0,
      );
      if (existing) {
        return {
          items: state.items.map((item) =>
            item.key === existing.key
              ? { ...item, quantity: item.quantity + quantity }
              : item,
          ),
        };
      }
      const line: CartItem = {
        key: crypto.randomUUID(),
        product_id: product.id,
        barcode: product.barcode,
        name: product.name,
        unit_price_cents,
        quantity,
        discount_cents: 0,
        fiscal: snapshotOf(product),
      };
      return { items: [...state.items, line] };
    });
  },

  removeItem: (key) => {
    set((state) => ({ items: state.items.filter((item) => item.key !== key) }));
  },

  applyDiscount: (key, discountCents) => {
    if (!Number.isFinite(discountCents)) return;
    set((state) => ({
      items: state.items.map((item) => {
        if (item.key !== key) return item;
        const gross = item.unit_price_cents * item.quantity;
        // Trava 0..bruto: sem desconto negativo, sem "desconto" que zera
        // além da linha (abuso seria barrado no backend de todo modo).
        const clamped = Math.min(Math.max(Math.floor(discountCents), 0), gross);
        return { ...item, discount_cents: clamped };
      }),
    }));
  },

  clearCart: () => {
    set({ items: [] });
  },
}));
