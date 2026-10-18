// @vitest-environment jsdom
// Views (#230): the outlet renders the current view; tabbed views follow nav.tab; the Library's
// More menu, Updates' check button and Help's links call back.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createRawSnippet, tick } from "svelte";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import OutletHarness from "./__tests__/OutletHarness.svelte";
import DiagnoseKeepHarness from "./__tests__/DiagnoseKeepHarness.svelte";
import LibraryView from "./LibraryView.svelte";
import ModSetsView from "./ModSetsView.svelte";
import DiagnoseView from "./DiagnoseView.svelte";
import ToolsView from "./ToolsView.svelte";
import SettingsView from "./SettingsView.svelte";
import UpdatesView from "./UpdatesView.svelte";
import HelpView, { BUG_REPORT_URL, DISCORD_URL, DISCUSSIONS_URL } from "./HelpView.svelte";
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
    setsTab: stub("My sets body"),
    openTab: stub("Open body"),
    savegamesTab: stub("Savegames body"),
  };

  it("titles itself and opens on My sets", () => {
    render(ModSetsView, props);
    expect(screen.getByRole("heading", { level: 2, name: "Mod sets" })).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((t) => t.textContent?.trim())).toEqual([
      "My sets",
      "Open a shared set",
      "Savegames",
    ]);
    expect(screen.getByText("My sets body")).toBeInTheDocument();
  });

  it("go('modSets', 'open') lands on Open a shared set; clicking a tab sets nav.tab", async () => {
    const user = userEvent.setup();
    render(ModSetsView, props);
    go("modSets", "open");
    await tick();
    expect(screen.getByText("Open body")).toBeInTheDocument();
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

describe("Diagnose → Last run while a bisection runs", () => {
  const probe = () => screen.getByRole("button", { name: /Next round/, hidden: true });
  const panel = () => screen.getByRole("tabpanel");

  it("switching tabs or views doesn't unmount it (no onBisecting(false)), and it comes back as it was", async () => {
    const user = userEvent.setup();
    const onBisecting = vi.fn();
    go("diagnose");
    render(DiagnoseKeepHarness, { bisecting: true, onBisecting });
    await tick();
    expect(panel()).toContainElement(probe());
    await user.click(probe());
    expect(probe()).toHaveTextContent("Next round (1)");

    await user.click(screen.getByRole("tab", { name: "Conflicts" }));
    await tick();
    expect(screen.getByText("Conflicts body")).toBeInTheDocument();
    expect(panel()).not.toContainElement(probe());
    expect(probe()).not.toBeVisible();
    expect(onBisecting).not.toHaveBeenCalled();

    go("library");
    await tick();
    expect(screen.queryByRole("tablist")).toBeNull();
    expect(onBisecting).not.toHaveBeenCalled();

    go("diagnose", "lastRun");
    await tick();
    expect(panel()).toContainElement(probe());
    expect(probe()).toBeVisible();
    expect(probe()).toHaveTextContent("Next round (1)"); // same instance, state kept
    expect(onBisecting).not.toHaveBeenCalled();
  });

  it("when not bisecting it mounts and unmounts with the tab", async () => {
    const user = userEvent.setup();
    const onBisecting = vi.fn();
    go("diagnose");
    render(DiagnoseKeepHarness, { bisecting: false, onBisecting });
    await tick();
    expect(panel()).toContainElement(probe());
    await user.click(screen.getByRole("tab", { name: "Problems" }));
    await tick();
    expect(screen.queryByRole("button", { name: /Next round/, hidden: true })).toBeNull();
    expect(onBisecting).toHaveBeenCalledWith(false);
  });

  it("once the bisection ends away from the tab, the hidden copy goes", async () => {
    const onBisecting = vi.fn();
    go("diagnose");
    const { rerender } = render(DiagnoseKeepHarness, { bisecting: true, onBisecting });
    await tick();
    go("updates");
    await tick();
    expect(probe()).toBeInTheDocument();
    await rerender({ bisecting: false, onBisecting });
    await tick();
    expect(screen.queryByRole("button", { name: /Next round/, hidden: true })).toBeNull();
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

describe("SettingsView", () => {
  const props = () => ({
    generalTab: stub("General body"),
    modsFolderTab: stub("Mods folder body"),
    libraryTab: stub("Library body"),
    accountsTab: createRawSnippet(() => ({
      render: () => `<section id="settings-accounts" tabindex="-1">GitHub card</section>`,
    })),
    diagnosticsTab: stub("Diagnostics body"),
    maintenanceTab: stub("Maintenance body"),
    aboutTab: stub("About body"),
  });

  it("has the seven sections as tabs and opens on General", () => {
    render(SettingsView, props());
    expect(screen.getByRole("heading", { level: 2, name: "Settings" })).toBeInTheDocument();
    expect(screen.getAllByRole("tab").map((t) => t.textContent?.trim())).toEqual([
      "General",
      "Mods folder",
      "Library",
      "Accounts",
      "Diagnostics",
      "Maintenance",
      "About",
    ]);
    expect(screen.getByText("General body")).toBeInTheDocument();
  });

  it("clicking a tab shows its section and sets nav.tab.settings, without focusing the section", async () => {
    const user = userEvent.setup();
    render(SettingsView, props());
    await user.click(screen.getByRole("tab", { name: "Maintenance" }));
    expect(screen.getByText("Maintenance body")).toBeInTheDocument();
    expect(screen.queryByText("General body")).toBeNull();
    expect(nav.tab.settings).toBe("maintenance");
    await user.click(screen.getByRole("tab", { name: "Accounts" }));
    await tick();
    await tick();
    expect(screen.getByText("GitHub card")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Accounts" })).toHaveFocus();
  });

  it("nav.tab.settings = 'accounts' opens Accounts", async () => {
    render(SettingsView, props());
    nav.tab.settings = "accounts";
    await tick();
    expect(screen.getByRole("tab", { name: "Accounts" })).toHaveAttribute("aria-selected", "true");
    expect(screen.getByText("GitHub card")).toBeInTheDocument();
  });

  it("go('settings', 'accounts') opens Accounts and focuses the GitHub card, once", async () => {
    render(SettingsView, props());
    go("settings", "accounts");
    await tick();
    await tick();
    expect(screen.getByText("GitHub card")).toHaveFocus();
    expect(nav.tab.settings).toBe("accounts"); // the tab stays (it isn't cleared any more)
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
      checkedAt: null,
      checkDisabled: false,
      oncheck,
      children: stub("Update rows"),
    });
    expect(screen.getByText("Update rows")).toBeInTheDocument();
    expect(screen.getByText(/Not checked yet/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Check now/ }));
    expect(oncheck).toHaveBeenCalledOnce();
    await rerender({
      checking: false,
      checkedAt: Date.now(),
      checkDisabled: true,
      oncheck,
      children: stub("Update rows"),
    });
    expect(screen.getByText(/Last checked just now/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Check now/ }));
    expect(oncheck).toHaveBeenCalledOnce();
  });
});

describe("HelpView", () => {
  it("opens the community links", async () => {
    const user = userEvent.setup();
    const onopen = vi.fn();
    render(HelpView, { onopen, oncopydiagnostics: vi.fn() });
    await user.click(screen.getByRole("button", { name: /GitHub Discussions/ }));
    await user.click(screen.getByRole("button", { name: /Discord/ }));
    expect(onopen.mock.calls.map((c) => c[0])).toEqual([DISCUSSIONS_URL, DISCORD_URL]);
    expect(screen.queryByRole("button", { name: "Keyboard shortcuts" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "See what's new" })).not.toBeInTheDocument();
  });

  it("Report a problem copies the diagnostics first, then opens the bug form", async () => {
    const user = userEvent.setup();
    const order: string[] = [];
    let finishCopy: () => void = () => {};
    const oncopydiagnostics = vi.fn(
      () =>
        new Promise<void>((resolve) => {
          order.push("copy started");
          finishCopy = () => {
            order.push("copied");
            resolve();
          };
        }),
    );
    const onopen = vi.fn((url: string) => order.push(`open ${url}`));
    render(HelpView, { onopen, oncopydiagnostics });
    await user.click(screen.getByRole("button", { name: /^Report a problem/ }));
    // The form waits for the copy.
    expect(onopen).not.toHaveBeenCalled();
    finishCopy();
    await vi.waitFor(() => expect(onopen).toHaveBeenCalledOnce());
    expect(order).toEqual(["copy started", "copied", `open ${BUG_REPORT_URL}`]);
  });

  it("Report a problem still opens the form when the copy fails", async () => {
    const user = userEvent.setup();
    const onopen = vi.fn();
    render(HelpView, { onopen, oncopydiagnostics: vi.fn(() => Promise.reject(new Error("no clipboard"))) });
    await user.click(screen.getByRole("button", { name: /^Report a problem/ }));
    await vi.waitFor(() => expect(onopen).toHaveBeenCalledWith(BUG_REPORT_URL));
  });

  it("Copy diagnostics only copies", async () => {
    const user = userEvent.setup();
    const onopen = vi.fn();
    const oncopydiagnostics = vi.fn();
    render(HelpView, { onopen, oncopydiagnostics });
    await user.click(screen.getByRole("button", { name: /Copy diagnostics only/ }));
    expect(oncopydiagnostics).toHaveBeenCalledOnce();
    expect(onopen).not.toHaveBeenCalled();
  });

  it("opens the keyboard shortcuts list and What's new", async () => {
    const user = userEvent.setup();
    const onshowshortcuts = vi.fn();
    const onshowwhatsnew = vi.fn();
    render(HelpView, { onopen: vi.fn(), oncopydiagnostics: vi.fn(), onshowshortcuts, onshowwhatsnew });
    expect(screen.getByRole("heading", { name: "Keyboard shortcuts" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Keyboard shortcuts" }));
    expect(onshowshortcuts).toHaveBeenCalledOnce();
    await user.click(screen.getByRole("button", { name: "See what's new" }));
    expect(onshowwhatsnew).toHaveBeenCalledOnce();
  });

  it("searching for loadouts finds Mod sets", async () => {
    const user = userEvent.setup();
    render(HelpView, { onopen: vi.fn(), oncopydiagnostics: vi.fn() });
    await user.type(screen.getByRole("searchbox", { name: "Search help" }), "loadouts");
    const topics = screen.getAllByRole("listitem").filter((li) => li.closest(".topics"));
    expect(topics.map((li) => li.querySelector("summary")?.textContent)).toEqual(["Mod sets"]);
    await user.clear(screen.getByRole("searchbox", { name: "Search help" }));
    await user.type(screen.getByRole("searchbox", { name: "Search help" }), "zzzz");
    expect(screen.getByText(/Nothing in Help matches/)).toBeInTheDocument();
  });

  it("About lists the third-party licenses, the fonts first", () => {
    render(HelpView, { onopen: vi.fn(), oncopydiagnostics: vi.fn(), appVer: "1.0.0" });
    expect(screen.getByText("Silo v1.0.0")).toBeInTheDocument();
    const names = [...document.querySelectorAll(".lic-name")].map((e) => e.textContent);
    expect(names.slice(0, 2)).toEqual(["Fraunces (font)", "Hanken Grotesk (font)"]);
    expect(names).toContain("@lucide/svelte");
  });
});

