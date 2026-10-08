/**
 * Cliente da API de produtos (US03 Task 3.1/3.3).
 * JWT via `api()` (memória + cofre); dinheiro trafega como número.
 */

import { api } from "./api";

export interface Product {
  id: string;
  store_id: string;
  sku: string;
  barcode: string | null;
  name: string;
  price: string;
  cost: string;
  ncm: string | null;
  cest: string | null;
  cfop: string | null;
  icms_origin: string | null;
  icms_rate: string;
  is_active: boolean;
}

export interface NewProduct {
  name: string;
  sku: string;
  barcode?: string;
  price: number;
  cost: number;
  ncm: string;
  cest?: string;
  cfop?: string;
  icms_origin?: string;
  icms_rate: number;
}

export function listProducts(q?: string): Promise<Product[]> {
  const query = q ? `?q=${encodeURIComponent(q)}` : "";
  return api<Product[]>(`/api/v1/products${query}`);
}

/**
 * Acha o produto pelo código de barras exato (US06 Task 6.2: o bip).
 * Usa a busca textual da API (`q` cobre nome/sku/barcode, isolada por loja
 * e só ativos) e filtra o match exato no cliente. `null` = não cadastrado.
 */
export async function findProductByBarcode(code: string): Promise<Product | null> {
  const candidates = await listProducts(code);
  return candidates.find((p) => p.barcode === code) ?? null;
}

export function createProduct(input: NewProduct): Promise<Product> {
  return api<Product>("/api/v1/products", {
    method: "POST",
    body: JSON.stringify(input),
  });
}
