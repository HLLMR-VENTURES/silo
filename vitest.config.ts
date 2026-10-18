import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import { svelteTesting } from "@testing-library/svelte/vite";

// Frontend unit tests. Pure TypeScript tests run in the default `node` environment.
// Component tests opt into the DOM with a `// @vitest-environment jsdom` docblock.
// Kept separate from vite.config.ts so dev-server settings stay out.
export default defineConfig({
  plugins: [
    svelte(),
    // Resolves Svelte's browser build under Vitest (Svelte 5 mounts only in the browser build).
    // Auto-cleanup is off: src/vitest-setup.ts registers it for jsdom tests only, so pure tests
    // don't load testing-library at all.
    svelteTesting({ autoCleanup: false }),
  ],
  resolve: process.env.VITEST ? { conditions: ["browser"] } : undefined,
  test: {
    include: ["src/**/*.test.ts"],
    environment: "node",
    setupFiles: ["src/vitest-setup.ts"],
  },
});
