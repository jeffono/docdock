import { svelte } from "@sveltejs/vite-plugin-svelte";
import { defineConfig } from "vitest/config";

export default defineConfig({
  plugins: [svelte()],
  test: { include: ["src/**/*.test.ts"] },
  // Keep Rust errors visible in the terminal.
  clearScreen: false,
  server: {
    // Tauri expects this exact port.
    port: 1420,
    strictPort: true,
    host: "127.0.0.1",
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
