import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  test: { include: ["src/**/*.test.ts"] },
  // Mantém os erros do Rust visíveis no terminal.
  clearScreen: false,
  server: {
    // O Tauri espera exatamente esta porta.
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
