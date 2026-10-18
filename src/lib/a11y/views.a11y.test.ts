// @vitest-environment jsdom
// axe over the main views inside the real app frame (#258): rail, header (search, status chip,
// Play), notices, and each view filled with its real panels and stub data. The frame is whole, so
// the landmark rule (`region`) stays on here.
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import { tick } from "svelte";

// A working localStorage before nav.svelte loads (Node's own global one only warns under jsdom).
vi.hoisted(() => {
  const map = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => void map.set(k, String(v)),
      removeItem: (k: string) => void map.delete(k),
      clear: () => map.clear(),
    },
  });
});

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getModIcon: vi.fn(() => Promise.resolve(null)),
  catalogDetailByTech: vi.fn(() => Promise.resolve(null)),
  catalogCheckUpdates: vi.fn(() => Promise.resolve([])),
  scanLog: vi.fn(() => Promise.reject(new Error("no log"))),
  guessRepo: vi.fn(() => Promise.resolve(null)),
  browseMods: vi.fn(() =>
    Promise.resolve({
      mods: [
        {
          id: "a",
          techName: "FS25_Lizard",
          title: "Lizard Pack",
          author: "Someone",
          category: null,
          description: null,
          imageUrl: null,
          latestVersion: "1.0.0.0",
          trustScore: null,
          updatedAt: null,
          sources: [
            {
              source: "github",
              sourceUrl: "https://github.com/o/r",
              version: "1.0.0.0",
              installable: true,
              downloadUrl: "https://example.invalid/a.zip",
            },
          ],
          pageUrl: null,
        },
      ],
      total: 1,
      page: 1,
      pageSize: 48,
    }),
  ),
  siloapiStats: vi.fn(() => Promise.resolve({ mods: 3, sources: 4 })),
  siloapiCategories: vi.fn(() => Promise.resolve([])),
  siloapiFacets: vi.fn(() => Promise.resolve({ facets: {} })),
  onInstallProgress: vi.fn(() => Promise.resolve(() => {})),
  catalogImage: vi.fn(() => Promise.resolve("")),
  ghStatus: vi.fn(() =>
    Promise.resolve({ clientId: null, user: null, builtin: true, canWrite: false, canGist: false }),
  ),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));

import ViewHarness from "./__tests__/ViewHarness.svelte";
import { go, resetNav, type ViewId } from "../state/nav.svelte";
import { clearNotice, setNotice } from "../state/notify.svelte";
import { expectNoAxeViolations } from "./axe";
import { syntheticLibrary } from "./fixtures";

beforeAll(() => {
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});

beforeEach(() => {
  resetNav();
  setNotice({
    key: "a11y-game",
    level: "warning",
    title: "Farming Simulator 25 is running",
    body: "Silo won't change your mods folder until you quit.",
    dismissible: true,
    actions: [{ label: "Show me", run: () => {}, primary: true }],
  });
  return () => clearNotice("a11y-game");
});

/** Let mounts, mocked promises and the renders they cause settle. */
async function settle() {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await tick();
  }
}

const mods = syntheticLibrary(40);

const CASES: { which: string; view: ViewId; tab?: string; expectText: RegExp }[] = [
  { which: "library", view: "library", expectText: /mods, \d+ active/ },
  { which: "librarySelecting", view: "library", expectText: /2 selected/i },
  { which: "libraryCards", view: "library", expectText: /mods, \d+ active/ },
  { which: "libraryDrawer", view: "library", expectText: /Mod details|Valley|Mod 0/ },
  { which: "updates", view: "updates", expectText: /FS25_Ready/ },
  { which: "modSets", view: "modSets", expectText: /Harvest 2026/ },
  { which: "savegames", view: "modSets", tab: "savegames", expectText: /Riverbend/ },
  { which: "settings", view: "settings", expectText: /Theme/ },
  { which: "tools", view: "tools", expectText: /Tools/ },
  { which: "help", view: "help", expectText: /Help/ },
  { which: "diagnose", view: "diagnose", tab: "problems", expectText: /FS25_gone/ },
  { which: "browse", view: "browse", expectText: /Lizard Pack/ },
];

describe("axe: views in the app frame", () => {
  for (const c of CASES) {
    it(c.which, async () => {
      go(c.view, c.tab);
      render(ViewHarness, { which: c.which, mods });
      await settle();
      expect(screen.getAllByText(c.expectText).length).toBeGreaterThan(0);
      await expectNoAxeViolations(document.body);
    });
  }
});
