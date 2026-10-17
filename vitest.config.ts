import { defineConfig } from "vitest/config";

// Frontend unit tests: pure TypeScript logic only (no Svelte components, no Tauri).
// Kept separate from vite.config.ts so the Svelte plugin and dev-server settings stay out.
export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
  },
});
