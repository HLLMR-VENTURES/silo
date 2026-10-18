// Navigation state: go / back / tabs, history cap, persistence (and storage that throws), rail
// collapse rules and search scoping.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  back,
  canGoBack,
  go,
  HISTORY_MAX,
  historyDepth,
  nav,
  NAV_RAIL_KEY,
  NAV_VIEW_KEY,
  nextRailPin,
  railCollapsedFor,
  resetNav,
  restoreNav,
  searchPlaceholderFor,
  searchTargetFor,
  setRailPin,
  setTab,
} from "./nav.svelte";

function memoryStorage(): Storage {
  const map = new Map<string, string>();
  return {
    get length() {
      return map.size;
    },
    clear: () => map.clear(),
    getItem: (k) => map.get(k) ?? null,
    key: (i) => [...map.keys()][i] ?? null,
    removeItem: (k) => void map.delete(k),
    setItem: (k, v) => void map.set(k, String(v)),
  };
}

let store: Storage;

beforeEach(() => {
  store = memoryStorage();
  vi.stubGlobal("localStorage", store);
  resetNav();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("go / back", () => {
  it("starts on the Library with no history", () => {
    expect(nav.view).toBe("library");
    expect(canGoBack()).toBe(false);
    expect(back()).toBe(false);
  });

  it("switches views, sets the tab, and walks back", () => {
    go("diagnose", "bisect");
    expect(nav.view).toBe("diagnose");
    expect(nav.tab.diagnose).toBe("bisect");
    go("modSets", "share");
    expect(nav.view).toBe("modSets");
    expect(back()).toBe(true);
    expect(nav.view).toBe("diagnose");
    expect(nav.tab.diagnose).toBe("bisect");
    expect(back()).toBe(true);
    expect(nav.view).toBe("library");
    expect(canGoBack()).toBe(false);
  });

  it("going to the same view and tab is a no-op", () => {
    go("library");
    expect(historyDepth()).toBe(0);
    go("tools", "stats");
    go("tools", "stats");
    go("tools");
    expect(historyDepth()).toBe(1);
  });

  it("a tab change within a view is a history entry and back restores the tab", () => {
    go("diagnose", "lastRun");
    go("diagnose", "conflicts");
    expect(historyDepth()).toBe(2);
    back();
    expect(nav.view).toBe("diagnose");
    expect(nav.tab.diagnose).toBe("lastRun");
  });

  it("go without a tab keeps the view's last tab", () => {
    go("modSets", "multiplayer");
    go("library");
    go("modSets");
    expect(nav.tab.modSets).toBe("multiplayer");
  });

  it("setTab changes a tab without history", () => {
    setTab("settings", "appearance");
    expect(nav.tab.settings).toBe("appearance");
    expect(historyDepth()).toBe(0);
  });

  it("caps history at HISTORY_MAX, dropping the oldest", () => {
    const views = ["browse", "modSets", "diagnose", "updates"] as const;
    for (let i = 0; i < HISTORY_MAX + 5; i++) go(views[i % views.length]);
    expect(historyDepth()).toBe(HISTORY_MAX);
    let steps = 0;
    while (back()) steps++;
    expect(steps).toBe(HISTORY_MAX);
    // The oldest entry (the starting Library) was dropped.
    expect(nav.view).not.toBe("library");
  });
});

describe("persistence", () => {
  it("persists the view on go and back", () => {
    go("browse");
    expect(store.getItem(NAV_VIEW_KEY)).toBe("browse");
    go("updates");
    back();
    expect(store.getItem(NAV_VIEW_KEY)).toBe("browse");
  });

  it("restores a stored view and rail pin", () => {
    store.setItem(NAV_VIEW_KEY, "diagnose");
    store.setItem(NAV_RAIL_KEY, "collapsed");
    restoreNav();
    expect(nav.view).toBe("diagnose");
    expect(nav.rail).toBe("collapsed");
  });

  it("ignores junk values", () => {
    store.setItem(NAV_VIEW_KEY, "nowhere");
    store.setItem(NAV_RAIL_KEY, "sideways");
    restoreNav();
    expect(nav.view).toBe("library");
    expect(nav.rail).toBe("auto");
  });

  it("persists the rail pin", () => {
    setRailPin("expanded");
    expect(nav.rail).toBe("expanded");
    expect(store.getItem(NAV_RAIL_KEY)).toBe("expanded");
  });

  it("survives storage that throws", () => {
    const boom = () => {
      throw new Error("blocked");
    };
    vi.stubGlobal("localStorage", { getItem: boom, setItem: boom } as unknown as Storage);
    expect(() => restoreNav()).not.toThrow();
    expect(nav.view).toBe("library");
    expect(() => go("browse")).not.toThrow();
    expect(nav.view).toBe("browse");
    expect(() => setRailPin("collapsed")).not.toThrow();
    expect(nav.rail).toBe("collapsed");
  });

  it("survives a missing localStorage", () => {
    vi.stubGlobal("localStorage", undefined);
    expect(() => restoreNav()).not.toThrow();
    expect(() => go("help")).not.toThrow();
    expect(nav.view).toBe("help");
  });
});

describe("rail", () => {
  it("auto collapses below 1200px", () => {
    expect(railCollapsedFor(1199, "auto")).toBe(true);
    expect(railCollapsedFor(1200, "auto")).toBe(false);
  });

  it("pins win over width", () => {
    expect(railCollapsedFor(800, "expanded")).toBe(false);
    expect(railCollapsedFor(1600, "collapsed")).toBe(true);
  });

  it("a flip pins only when it differs from auto", () => {
    // Wide window, expanded: collapsing pins collapsed; expanding again returns to auto.
    expect(nextRailPin(false, 1400)).toBe("collapsed");
    expect(nextRailPin(true, 1400)).toBe("auto");
    // Narrow window, collapsed: expanding pins expanded; collapsing again returns to auto.
    expect(nextRailPin(true, 900)).toBe("expanded");
    expect(nextRailPin(false, 900)).toBe("auto");
  });
});

describe("search scope", () => {
  it("uses the catalog on Browse and your mods everywhere else", () => {
    expect(searchPlaceholderFor("library")).toBe("Search your mods");
    expect(searchPlaceholderFor("browse")).toBe("Search the catalog");
    expect(searchPlaceholderFor("diagnose")).toBe("Search your mods");
    expect(searchTargetFor("browse")).toBe("browse");
    expect(searchTargetFor("updates")).toBe("library");
  });
});
