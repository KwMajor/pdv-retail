/**
 * Leitor de código de barras (US06 Task 6.2): bip rápido + Enter vira scan;
 * digitação humana, alvo editável e lixo não viram.
 */
import { renderHook } from "@testing-library/react";
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "../stores/cart";
import { useBarcodeScanner, useScanToCart } from "./useBarcodeScanner";

const CODE = "7891020304050"; // QA do TASKS.md: digitar rápido + Enter

function press(key: string, target: EventTarget = window): void {
  target.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true }));
}

/** Simula o leitor: todos os dígitos + Enter em rajada (< 50ms/tecla). */
function scan(code: string): void {
  for (const digit of code) press(digit);
  press("Enter");
}

beforeEach(() => {
  useCart.getState().clearCart();
  vi.useRealTimers();
});

afterEach(() => {
  vi.unstubAllGlobals();
  vi.useRealTimers();
});

describe("useBarcodeScanner", () => {
  it("rajada de dígitos + Enter dispara onScan (QA: 7891020304050)", () => {
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan }));
    scan(CODE);
    expect(onScan).toHaveBeenCalledTimes(1);
    expect(onScan).toHaveBeenCalledWith(CODE);
  });

  it("digitação humana (lenta) + Enter NÃO dispara", () => {
    vi.useFakeTimers();
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan }));
    for (const digit of CODE) {
      press(digit);
      vi.advanceTimersByTime(200); // humano: 200ms entre teclas
    }
    press("Enter");
    expect(onScan).not.toHaveBeenCalled();
  });

  it("sequência curta, com letra ou vazia NÃO dispara", () => {
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan }));
    scan("123"); // curta (< 4)
    // Letra quebra a sequência e a cauda "12" segue curta: nada dispara.
    for (const key of ["7", "8", "9", "a", "1", "2"]) press(key);
    press("Enter");
    press("Enter"); // buffer vazio
    expect(onScan).not.toHaveBeenCalled();
  });

  it("ignora bip com foco em campo editável (vai p/ o campo, não p/ o carrinho)", () => {
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan }));
    const input = document.createElement("input");
    document.body.appendChild(input);
    try {
      for (const digit of CODE) {
        input.dispatchEvent(new KeyboardEvent("keydown", { key: digit, bubbles: true }));
      }
      input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }));
      expect(onScan).not.toHaveBeenCalled();
    } finally {
      input.remove();
    }
  });

  it("tecla lenta no meio reinicia: prefixo lento + rajada = só a rajada", () => {
    vi.useFakeTimers();
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan }));
    press("9");
    vi.advanceTimersByTime(500); // pausa humana: descarta o "9"
    for (const digit of "1234") press(digit);
    press("Enter");
    expect(onScan).toHaveBeenCalledTimes(1);
    expect(onScan).toHaveBeenCalledWith("1234");
  });

  it("enabled=false desliga o listener", () => {
    const onScan = vi.fn();
    renderHook(() => useBarcodeScanner({ onScan, enabled: false }));
    scan(CODE);
    expect(onScan).not.toHaveBeenCalled();
  });
});

describe("useScanToCart", () => {
  const apiProduct = {
    id: "11111111-1111-1111-1111-111111111111",
    store_id: "22222222-2222-2222-2222-222222222222",
    sku: "ARROZ-T1",
    barcode: CODE,
    name: "Arroz T1 5kg",
    price: "10.00",
    cost: "7.00",
    ncm: "10063021",
    cest: null,
    cfop: "5102",
    icms_origin: "0",
    icms_rate: "18.00",
    is_active: true,
  };

  function stubFetch(body: unknown): void {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, status: 200, json: async () => body })),
    );
  }

  it("bip de produto cadastrado cai no carrinho sem clicar em nada", async () => {
    stubFetch([apiProduct]);
    renderHook(() => useScanToCart());
    await act(async () => {
      scan(CODE);
    });
    const { items } = useCart.getState();
    expect(items).toHaveLength(1);
    expect(items[0].barcode).toBe(CODE);
    expect(items[0].unit_price_cents).toBe(1000);
  });

  it("bip de código desconhecido NÃO adiciona e expõe lastError", async () => {
    stubFetch([]);
    const { result } = renderHook(() => useScanToCart());
    await act(async () => {
      scan("0000000000000");
    });
    expect(useCart.getState().items).toHaveLength(0);
    expect(result.current.lastError).toMatch(/não cadastrado/);
    act(() => {
      result.current.clearError();
    });
    expect(result.current.lastError).toBeNull();
  });
});
