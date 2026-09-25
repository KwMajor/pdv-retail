/**
 * `RoleGate` (US02 Task 2.3): renderiza `children` só se o papel da sessão
 * estiver na lista exigida. Caixa não vê menus de retaguarda — eles nem
 * entram no DOM (não é `display: none`).
 */

import type { ReactNode } from "react";
import { useSession } from "../stores/session";
import { roleAllows, type AppRole } from "../auth/roles";

interface RoleGateProps {
  allowed: readonly AppRole[];
  children: ReactNode;
}

export function RoleGate({ allowed, children }: RoleGateProps) {
  const role = useSession((s) => s.user?.role);
  if (!roleAllows(role, allowed)) return null;
  return <>{children}</>;
}
