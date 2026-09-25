import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Porta 1420 = devUrl do tauri.conf.json. Host fixo evita surpresa de porta.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
  },
  build: {
    target: "safari13",
  },
});
