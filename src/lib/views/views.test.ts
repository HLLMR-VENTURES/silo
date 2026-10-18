// @vitest-environment jsdom
// Views (#230): the outlet renders the current view; tabbed views follow nav.tab; the Library's
// More menu, Updates' check button and Help's links call back.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createRawSnippet, tick } from "svelte";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import OutletHarness from "./__tests__/OutletHarness.svelte";
import LibraryView from "./LibraryView.svelte";
import ModSetsView from "./ModSetsView.svelte";
import DiagnoseView from "./DiagnoseView.svelte";
import ToolsView from "./ToolsView.svelte";
import UpdatesView from "./UpdatesView.svelte";
import HelpView, { DISCORD_URL, DISCUSSIONS_URL, NEW_ISSUE_URL } from "./HelpView.svelte";
import { go, nav, resetNav, VIEW_IDS } from "../state/nav.svelte";

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

/** A snippet that renders `<p>{text}</p>`. */
const stub = (text: string) => createRawSnippet(() => ({ render: () => `<p>${text}</p>` }));

beforeEach(() => resetNav());

describe("ViewOutlet", () => {
  it("renders the current view's snippet and follows go()", async () => {
    render(OutletHarness);
    expect(screen.getByTestId("view")).toHaveTextContent("library");
    for (const id of VIEW_IDS.filter((v) => v !== "help")) {
      go(id);
      await tick();
      expect(screen.getByTestId("view")).toHaveTextContent(id);
    }
  });

  it("renders nothing for a view without a snippet", async () => {
    render(OutletHarness);
    go("help");
    await tick();
    expect(screen.queryByTestId("view")).toBeNull();
  });
});

describe("ModSetsView", () => {
  const props = {
    setsTab: stub("Loadouts body"),
    shareTab: stub("Collections body"),
    multiplayerTab: stub("MP body"),
    savegamesTab: stub("Savegames body"),
  };

  it("titles itself and opens on My sets", () => {
    render(ModSetsView, props);
    expect(screen.getByRole("heading", { level: 2, name: "Mod sets" })).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((t) => t.textContent?.trim())).toEqual([
      "My sets",
      "Share",
      "Multiplayer",
      "Savegames",
    ]);
    expect(screen.getByText("Loadouts body")).toBeInTheDocument();
  });

  it("go('modSets', 'share') lands on Share; clicking a tab sets nav.tab", async () => {
    const user = userEvent.setup();
    render(ModSetsView, props);
    go("modSets", "share");
    await tick();
    expect(screen.getByText("Collections body")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: "Savegames" }));
    expect(nav.tab.modSets).toBe("savegames");
    expect(screen.getByText("Savegames body")).toBeInTheDocument();
  });
});

describe("DiagnoseView", () => {
  it("shows badges only for non-zero counts and switches tabs from nav", async () => {
    render(DiagnoseView, {
      lastRunTab: stub("Triage body"),
      conflictsTab: stub("Conflicts body"),
      problemsTab: stub("Health body"),
      conflictsBadge: 2,
      problemsBadge: 0,
    });
    expect(screen.getByText("Triage body")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: /Conflicts/ })).toHaveTextContent("2");
    expect(screen.getByRole("tab", { name: "Problems" })).toBeInTheDocument();
    go("diagnose", "problems");
    await tick();
    expect(screen.getByText("Health body")).toBeInTheDocument();
  });
});

describe("ToolsView", () => {
  it("has the four tools as tabs", async () => {
    render(ToolsView, {
      statsTab: stub("Stats body"),
      controlsTab: stub("Bindings body"),
      bridgeTab: stub("Bridge body"),
      graphicsTab: stub("Graphics body"),
    });
    expect(screen.getAllByRole("tab").map((t) => t.textContent?.trim())).toEqual([
      "Library stats",
      "Controls map",
      "Fill type bridge",
      "Graphics settings",
    ]);
    go("tools", "graphics");
    await tick();
    expect(screen.getByText("Graphics body")).toBeInTheDocument();
  });
});

describe("LibraryView", () => {
  it("shows the strip and runs the More menu's items", async () => {
    const user = userEvent.setup();
    const onmenuselect = vi.fn();
    const run = vi.fn();
    render(LibraryView, {
      stripLeft: "Ready to play · 3 of 9 active",
      stripItems: [{ id: "updates", text: "2 updates", action: "See updates", run }],
      menuItems: [
        { id: "rescan", label: "Rescan", icon: "rescan" },
        { id: "rebuild", label: "Re-read every mod" },
      ],
      onmenuselect,
      list: stub("The list"),
    });
    expect(screen.getByRole("heading", { level: 2, name: "Library" })).toBeInTheDocument();
    expect(screen.getByText("Ready to play · 3 of 9 active")).toBeInTheDocument();
    expect(screen.getByText("The list")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "See updates" }));
    expect(run).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "More library actions" }));
    await user.click(await screen.findByRole("menuitem", { name: "Re-read every mod" }));
    expect(onmenuselect).toHaveBeenCalledWith("rebuild");
  });
});

describe("UpdatesView", () => {
  it("the header button checks for updates, and is off when told", async () => {
    const user = userEvent.setup();
    const oncheck = vi.fn();
    const { rerender } = render(UpdatesView, {
      checking: false,
      checkDisabled: false,
      oncheck,
      children: stub("Update rows"),
    });
    expect(screen.getByText("Update rows")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(oncheck).toHaveBeenCalledOnce();
    await rerender({ checking: false, checkDisabled: true, oncheck, children: stub("Update rows") });
    await user.click(screen.getByRole("button", { name: /Check for updates/ }));
    expect(oncheck).toHaveBeenCalledOnce();
  });
});

describe("HelpView", () => {
  it("opens the community links and copies diagnostics", async () => {
    const user = userEvent.setup();
    const onopen = vi.fn();
    const oncopydiagnostics = vi.fn();
    render(HelpView, { onopen, oncopydiagnostics });
    await user.click(screen.getByRole("button", { name: /GitHub Discussions/ }));
    await user.click(screen.getByRole("button", { name: /Discord/ }));
    await user.click(screen.getByRole("button", { name: /Report a problem/ }));
    expect(onopen.mock.calls.map((c) => c[0])).toEqual([DISCUSSIONS_URL, DISCORD_URL, NEW_ISSUE_URL]);
    await user.click(screen.getByRole("button", { name: /Copy diagnostics/ }));
    expect(oncopydiagnostics).toHaveBeenCalledOnce();
  });
});
