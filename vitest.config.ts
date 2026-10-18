import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// Frontend unit tests: pure TypeScript logic (no Tauri). The Svelte plugin is here only so the
// icon registry test can import @lucide/svelte's .svelte components; tests don't render them.
export default defineConfig({
  plugins: [svelte()],
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    server: { deps: { inline: ["@lucide/svelte"] } },
  },
});
