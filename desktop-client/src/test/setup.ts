import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import "@testing-library/jest-dom/vitest";

// Sem globals do Vitest, o desmonte entre testes é manual.
afterEach(() => {
  cleanup();
});
