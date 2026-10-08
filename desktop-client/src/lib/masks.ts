/**
 * Máscaras e validadores de input (US03 Task 3.3).
 *
 * Padrão "máscara bloqueia + erro": estes helpers removem o caractere
 * inválido (nunca deixam persistir) e os validadores explicam o motivo.
 * O backend revalida tudo — aqui é UX, não autoridade.
 */

/** Só dígitos (EAN, NCM, quantidades inteiras). */
export function onlyDigits(value: string): string {
  return value.replace(/\D/g, "");
}

/** Tamanhos GTIN aceitos (8/12/13/14). */
const GTIN_LENS = [8, 12, 13, 14] as const;

export function isValidBarcode(digits: string): boolean {
  return (
    (GTIN_LENS as readonly number[]).includes(digits.length) &&
    /^[0-9]+$/.test(digits)
  );
}

export function barcodeError(digits: string): string | null {
  if (digits === "") return null; // opcional
  if (!/^[0-9]+$/.test(digits)) return "Código de barras aceita só números.";
  return `Código de barras deve ter ${GTIN_LENS.join(", ")} dígitos.`;
}

/** Moeda BRL a partir de centavos digitados: "2799" → "R$ 27,99". */
export function formatBRLFromCents(cents: string): string {
  const digits = onlyDigits(cents).slice(0, 12);
  const value = Number(digits === "" ? "0" : digits) / 100;
  return new Intl.NumberFormat("pt-BR", {
    style: "currency",
    currency: "BRL",
  }).format(value);
}

/** Centavos → decimal p/ API: "2799" → 27.99. */
export function centsToDecimal(cents: string): number {
  const digits = onlyDigits(cents).slice(0, 12);
  return (Number(digits === "" ? "0" : digits) / 100);
}

/**
 * Decimal da API (string exata, ex: "27.99") → centavos inteiros (2799).
 * SEM float no caminho: separa parte inteira/fracionária na string, então
 * `0.1 + 0.2 !== 0.3` nunca nos atinge. Retorna `null` se fora do domínio
 * (vazio, lixo, >2 casas, negativo, parte inteira > 10 dígitos = teto 10^10).
 * O carrinho (US06) guarda centavos; o backend revalida na venda (US07).
 */
export function decimalStringToCents(value: string): number | null {
  const v = value.trim();
  const m = /^(\d{1,10})(?:\.(\d{1,2}))?$/.exec(v);
  if (!m) return null;
  const cents = Number(m[1]) * 100 + Number(`${m[2] ?? ""}00`.slice(0, 2));
  return Number.isSafeInteger(cents) ? cents : null;
}

export function isValidNcm(digits: string): boolean {
  return digits.length === 8 && /^[0-9]{8}$/.test(digits);
}

export function ncmError(digits: string): string | null {
  if (digits === "") return "NCM é obrigatório.";
  if (!/^[0-9]+$/.test(digits)) return "NCM aceita só números.";
  if (digits.length !== 8) return "NCM deve ter exatamente 8 dígitos.";
  return null;
}

const DIGITS_7 = /^[0-9]{7}$/;
const DIGITS_4 = /^[0-9]{4}$/;

export function fieldError(
  kind: "cest" | "cfop" | "icms_origin" | "icms_rate" | "sku" | "name",
  value: string,
): string | null {
  const v = value.trim();
  switch (kind) {
    case "cest":
      if (v === "") return null;
      return DIGITS_7.test(v) ? null : "CEST deve ter 7 dígitos.";
    case "cfop":
      if (v === "") return null;
      return DIGITS_4.test(v) ? null : "CFOP deve ter 4 dígitos.";
    case "icms_origin":
      if (v === "") return null;
      return /^[0-8]$/.test(v) ? null : "Origem deve ser um dígito de 0 a 8.";
    case "icms_rate":
      if (v === "") return "Alíquota é obrigatória.";
      return null; // faixa 0–100 validada numericamente no submit
    case "sku":
      if (v === "") return "SKU é obrigatório.";
      if (v.length > 64) return "SKU deve ter no máximo 64 caracteres.";
      return /^[A-Za-z0-9\-_/.]+$/.test(v)
        ? null
        : "SKU aceita só letras, números e - _ / .";
    case "name":
      if (v === "") return "Nome é obrigatório.";
      if (v.length > 255) return "Nome deve ter no máximo 255 caracteres.";
      return /[\u0000-\u001F\u007F]/.test(v)
        ? "Nome contém caracteres inválidos."
        : null;
  }
}
