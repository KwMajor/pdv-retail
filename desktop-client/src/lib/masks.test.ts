/**
 * Validadores puros do PDV (US03 Task 3.3 + padrão de validação frontend).
 * Sem DOM: lógica de domínio testável isolada (máscara bloqueante na UI,
 * backend como autoridade).
 */
import { describe, expect, it } from "vitest";
import {
  barcodeError,
  centsToDecimal,
  decimalStringToCents,
  fieldError,
  formatBRLFromCents,
  isValidBarcode,
  isValidNcm,
  ncmError,
  onlyDigits,
} from "./masks";

describe("onlyDigits", () => {
  it("remove tudo que não é dígito", () => {
    expect(onlyDigits("789a1234b")).toBe("7891234");
    expect(onlyDigits("R$ 27,99")).toBe("2799");
    expect(onlyDigits("")).toBe("");
  });
});

describe("barcode (GTIN)", () => {
  it("aceita 8/12/13/14 dígitos", () => {
    for (const code of ["12345678", "789123456789", "7891234567890", "17891234567890"]) {
      expect(isValidBarcode(code)).toBe(true);
    }
  });

  it("rejeita tamanho ou letra com mensagem", () => {
    expect(isValidBarcode("123")).toBe(false);
    expect(isValidBarcode("7891234a")).toBe(false);
    expect(barcodeError("123")).toMatch(/8.*12.*13.*14/);
    expect(barcodeError("7891234a")).toMatch(/só números/);
    expect(barcodeError("")).toBeNull();
  });
});

describe("moeda BRL", () => {
  it("centavos ↔︎ exibição sem perda", () => {
    expect(centsToDecimal("2799")).toBe(27.99);
    expect(centsToDecimal("")).toBe(0);
    expect(formatBRLFromCents("2799")).toContain("27,99");
  });

  it("decimal da API → centavos exatos (US06, sem float)", () => {
    expect(decimalStringToCents("27.99")).toBe(2799);
    expect(decimalStringToCents("10")).toBe(1000);
    expect(decimalStringToCents("10.5")).toBe(1050);
    expect(decimalStringToCents("  0.10  ")).toBe(10);
    expect(decimalStringToCents("0.00")).toBe(0);
  });

  it("rejeita fora do domínio com null", () => {
    for (const bad of ["", "   ", "abc", "10.999", "-1.00", "12,34", "1.2.3", ".5"]) {
      expect(decimalStringToCents(bad)).toBeNull();
    }
    // Acima do teto 10^10 do backend.
    expect(decimalStringToCents("10000000000.00")).toBeNull();
  });
});

describe("NCM", () => {
  it("exige 8 dígitos numéricos", () => {
    expect(isValidNcm("10063021")).toBe(true);
    expect(isValidNcm("1234567")).toBe(false);
    expect(isValidNcm("123456789")).toBe(false);
    expect(isValidNcm("1234567a")).toBe(false);
    expect(ncmError("")).toBe("NCM é obrigatório.");
    expect(ncmError("1234567")).toContain("8 dígitos");
  });
});

describe("campos fiscais e SKU", () => {
  it("cest/cfop/origem com tamanhos exatos", () => {
    expect(fieldError("cest", "1234567")).toBeNull();
    expect(fieldError("cest", "123")).toContain("7 dígitos");
    expect(fieldError("cfop", "5102")).toBeNull();
    expect(fieldError("cfop", "510")).toContain("4 dígitos");
    expect(fieldError("icms_origin", "0")).toBeNull();
    expect(fieldError("icms_origin", "9")).toContain("0 a 8");
    expect(fieldError("icms_origin", "")).toBeNull();
  });

  it("sku bloqueia símbolos e excesso", () => {
    expect(fieldError("sku", "ARROZ-T1_5KG/x.y")).toBeNull();
    expect(fieldError("sku", "")).toContain("obrigatório");
    expect(fieldError("sku", "A@!")).toContain("só letras");
    expect(fieldError("sku", "a".repeat(65))).toContain("64");
  });

  it("nome obrigatório com teto", () => {
    expect(fieldError("name", "")).toContain("obrigatório");
    expect(fieldError("name", "Arroz")).toBeNull();
    expect(fieldError("name", "a".repeat(256))).toContain("255");
  });
});
