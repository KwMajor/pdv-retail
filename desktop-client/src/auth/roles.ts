/**
 * RBAC do frontend (US02 Task 2.3).
 *
 * Espelha os papéis do JWT (`admin`|`manager`|`cashier`, minúsculos como no
 * banco). É "cadeado visual": esconde menus/botões fora do cargo — a
 * autoridade real continua no backend (`TenantContext::require_roles`,
 * que devolve 403). Nunca confie só nesta camada contra Postman/cURL.
 */

export type AppRole = "admin" | "manager" | "cashier";

/** Papéis de gestão (retaguarda, cadastros, relatórios). */
export const MANAGERIAL_ROLES: readonly AppRole[] = ["admin", "manager"];

/** Papel com acesso total (dono/admin). */
export const ADMIN_ROLES: readonly AppRole[] = ["admin"];

/**
 * `true` se `role` está em `allowed`. Papel ausente/desconhecido = sem acesso
 * (fail-closed: sessão restaurada sem identidade não enxerga nada sensível).
 */
export function roleAllows(
  role: string | null | undefined,
  allowed: readonly AppRole[],
): boolean {
  if (!role) return false;
  return (allowed as readonly string[]).includes(role);
}

/** Atalho: retaguarda visível? */
export function canSeeBackoffice(role: string | null | undefined): boolean {
  return roleAllows(role, MANAGERIAL_ROLES);
}
