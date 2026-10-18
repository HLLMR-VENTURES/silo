// Vitest setup: DOM matchers and per-test cleanup, only for tests running under jsdom.
// Pure TypeScript tests (node environment) skip all of this and stay fast.
import { beforeEach } from "vitest";

if (typeof document !== "undefined") {
  await import("@testing-library/jest-dom/vitest");
  const { act, cleanup, setup } = await import("@testing-library/svelte");
  beforeEach(async () => {
    await setup();
    return async () => {
      await act();
      cleanup();
    };
  });
}
