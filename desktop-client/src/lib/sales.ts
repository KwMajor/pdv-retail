/**
 * Cliente da API de vendas (US07 Task 7.1/7.2).
 * JWT via `api()`; dinheiro trafega como string decimal ("19.00").
 */

import { api } from "./api";
import type { CreateSalePayload } from "./checkout";

export interface Sale {
  id: string;
  status: string;
  subtotal: string;
  discount: string;
  total: string;
  change_amount: string;
  created_at: string;
}

export function createSale(input: CreateSalePayload): Promise<Sale> {
  return api<Sale>("/api/v1/sales", {
    method: "POST",
    body: JSON.stringify(input),
  });
}
