// @vitest-environment jsdom
// The mod drawer (#245): header verdict badges, every action reachable, focus to the title on
// open, Esc closes and focus goes back to the row it showed.
import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ModDetail from "../ModDetail.svelte";
import DrawerHarness from "./__tests__/DrawerHarness.svelte";
import { setVerdict } from "../../provenanceCache.svelte";
import { catalogCheckUpdates, revealInFolder, scanLog, setCuration } from "../../api";
import type { Conflict, CurationRow, LogReport, ModEntry } from "../../types";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  getModIcon: vi.fn(() => Promise.resolve(null)),
  catalogDetailByTech: vi.fn(() => Promise.resolve(null)),
  catalogCheckUpdates: vi.fn(() => Promise.resolve([])),
  scanLog: vi.fn(() => Promise.reject(new Error("no log"))),
  guessRepo: vi.fn(() => Promise.resolve(null)),
  revealInFolder: vi.fn(() => Promise.resolve()),
  setCuration: vi.fn(() => Promise.resolve()),
}));

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
  vi.mocked(catalogCheckUpdates).mockImplementation(() => Promise.resolve([]));
  vi.mocked(scanLog).mockImplementation(() => Promise.reject(new Error("no log")));
});

function entry(i: number, over: Partial<ModEntry> = {}): ModEntry {
  return {
    techName: `FS25_Mod${i}`,
    path: `C:/mods/FS25_Mod${i}.zip`,
    kind: "zip",
    size: 1024 * 1024 * 3,
    mtimeMs: 1000 + i,
    title: `Mod ${i}`,
    author: "Someone",
    version: "1.0.0.0",
    descVersion: 1,
    iconFilename: null,
    isMap: false,
    mapTitle: null,
    category: "Tools",
    subcategory: null,
    organized: false,
    active: false,
    dependencies: [],
    scriptCount: 0,
    registrationCount: 0,
    uniqueType: null,
    storeItemCount: 0,
    mpSupported: true,
    ignoredDigitPrefix: false,
    error: null,
    hasSettings: false,
    settingsSource: "none",
    ...over,
  };
}

function props(mod: ModEntry, over: Record<string, unknown> = {}) {
  const curation: CurationRow = { techName: mod.techName, favorite: false, hidden: false, broken: false, rating: 0, note: null, skipUpdates: false };
  return {
    mod,
    curation,
    tags: [],
    active: true,
    organized: false,
    hasSettings: false,
    libraryTechNames: new Set<string>(),
    conflicts: [] as Conflict[],
    categories: ["Tools", "Vehicles"],
    isOverridden: false,
    onSetCategory: vi.fn(),
    onResetCategory: vi.fn(),
    onClose: vi.fn(),
    onToggle: vi.fn(),
    onToggleActive: vi.fn(),
    onOpenSettings: vi.fn(),
    onFindInBrowse: vi.fn(),
    onCurationChange: vi.fn(),
    onTagsChange: vi.fn(),
    onFilterTag: vi.fn(),
    repo: null,
    onRepoChange: vi.fn(),
    source: null,
    onSourceChange: vi.fn(),
    onInstalled: vi.fn(),
    foreign: null,
    onAdoptForeign: vi.fn(),
    onRestoreForeign: vi.fn(),
    ...over,
  };
}

async function settle() {
  for (let i = 0; i < 4; i++) await tick();
  await new Promise((r) => setTimeout(r, 0));
  await tick();
}

const header = () => screen.getByRole("heading", { level: 2 }).parentElement!;

describe("drawer header badges", () => {
  it("shows no verdict for an unchecked mod, then Modified with the file count once checked", async () => {
    const mod = entry(1, { path: "C:/mods/badges1.zip" });
    render(ModDetail, props(mod));
    await settle();
    expect(within(header()).queryByText(/Verified build|Modified|Problem|Update available/)).toBeNull();
    setVerdict(mod, { status: "modified", how: null, changed: ["a.xml", "b.xml"], added: [], removed: [], matchedVersion: null, note: null });
    await settle();
    expect(within(header()).getByText("Modified (2 files)")).toBeInTheDocument();
  });

  it("shows Verified build, Update available and Problem (critical conflict)", async () => {
    const mod = entry(2, { path: "C:/mods/badges2.zip" });
    setVerdict(mod, { status: "verified", how: "exact", changed: [], added: [], removed: [], matchedVersion: null, note: null });
    const conflicts: Conflict[] = [
      { id: "uniqueType:seedSlot:FS25_Mod2,FS25_Other", severity: "critical", kind: "uniqueType", name: "seedSlot", explanation: "", mods: ["FS25_Mod2", "FS25_Other"], techNames: ["FS25_Mod2", "FS25_Other"] },
    ];
    render(ModDetail, props(mod, { hasUpdate: true, conflicts }));
    await settle();
    const h = within(header());
    const words = [...header().querySelectorAll(".badge")].map((b) => b.textContent?.trim());
    expect(words).toEqual(["Problem", "Verified build", "Update available"]);
    expect(h.getByText("Problem")).toBeInTheDocument();
    // The pills are hover-explained; a screen reader gets the explanations as text.
    expect(h.getByText(/Problem: In a conflict that needs fixing/)).toBeInTheDocument();
    // Conflict kinds are named in plain words.
    expect(screen.getByText(/Exclusive slot “seedSlot”, with FS25_Other/)).toBeInTheDocument();
    // The conflict banner names the other mod and links to Diagnose.
    expect(screen.getByText(/Conflicts with 1 active mod/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "See conflicts" })).toBeInTheDocument();
  });

  it("takes Update from the drawer's own catalog check, and Problem from a crashed last run", async () => {
    vi.mocked(catalogCheckUpdates).mockImplementation(() =>
      Promise.resolve([{ techName: "FS25_Mod3", latest: "1.1.0.0", hasUpdate: true, downloadUrl: null, source: "modhub" }]),
    );
    const log: LogReport = {
      engineVersion: null,
      modCount: 1,
      cleanExit: false,
      crashed: true,
      mods: [{ modName: "FS25_Mod3", errors: 14, warnings: 0, benign: 0, sample: "Error: boom", sampleLine: 1 }],
      unattributed: 0,
      totalFindings: 14, groundPileCap: null, groundPileDisabled: [], fillTypeCap: null, droppedFillTypes: [], truncated: false,
    };
    vi.mocked(scanLog).mockImplementation(() => Promise.resolve(log));
    render(ModDetail, props(entry(3, { path: "C:/mods/badges3.zip" })));
    await settle();
    const words = [...header().querySelectorAll(".badge")].map((b) => b.textContent?.trim());
    expect(words).toEqual(["Problem", "Update available"]);
    expect(screen.getByText(/Logged 14 errors in your last game/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "See in Diagnose" })).toBeInTheDocument();
  });
});

describe("drawer Check for updates switch (#333)", () => {
  it("is on by default; switching it off saves skipUpdates and explains what that does", async () => {
    const mod = entry(1);
    const p = props(mod);
    const { rerender } = render(ModDetail, p);
    await settle();
    const sw = screen.getByRole("switch", { name: "Check for updates" });
    expect(sw).toHaveAttribute("aria-checked", "true");
    await userEvent.setup().click(sw);
    expect(p.onCurationChange).toHaveBeenCalledWith(expect.objectContaining({ skipUpdates: true }));
    expect(vi.mocked(setCuration)).toHaveBeenCalledWith(expect.objectContaining({ skipUpdates: true }));
    await rerender({ ...p, curation: { ...p.curation, skipUpdates: true } });
    expect(screen.getByRole("switch", { name: "Check for updates" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText(/leaves this mod out of update checks and Update all/)).toBeInTheDocument();
  });

  it("a switch that didn't save hands the row back to be put back (#358)", async () => {
    vi.mocked(setCuration).mockRejectedValueOnce("database is locked");
    const mod = entry(2);
    const onCurationFailed = vi.fn();
    const p = props(mod, { onCurationFailed });
    render(ModDetail, p);
    await settle();
    await userEvent.setup().click(screen.getByRole("switch", { name: "Check for updates" }));
    await settle();
    const sent = vi.mocked(p.onCurationChange).mock.calls[0][0];
    expect(onCurationFailed).toHaveBeenCalledWith(sent, p.curation);
  });
});

describe("drawer lookups", () => {
  it("ignores a catalog answer that arrives after the drawer moved to another mod", async () => {
    let answer!: (rows: Awaited<ReturnType<typeof catalogCheckUpdates>>) => void;
    vi.mocked(catalogCheckUpdates).mockImplementationOnce(() => new Promise((r) => (answer = r)));
    const p = props(entry(20, { path: "C:/mods/stale20.zip" }));
    const { rerender } = render(ModDetail, p);
    await settle();
    await rerender({ ...p, mod: entry(21, { path: "C:/mods/stale21.zip" }) });
    await settle();
    answer([{ techName: "FS25_Mod20", latest: "9.0.0.0", hasUpdate: true, downloadUrl: null, source: null }]);
    await settle();
    expect(within(header()).queryByText("Update available")).toBeNull();
    expect(screen.getByText(/Not in Silo's catalog/)).toBeInTheDocument();
  });

  it("doesn't call a mod with no version outdated on the catalog's word alone", async () => {
    vi.mocked(catalogCheckUpdates).mockImplementation(() =>
      Promise.resolve([{ techName: "FS25_Mod22", latest: "1.0.0.0", hasUpdate: true, downloadUrl: null, source: null }]),
    );
    render(ModDetail, props(entry(22, { path: "C:/mods/nover22.zip", version: null })));
    await settle();
    expect(within(header()).queryByText("Update available")).toBeNull();
  });
});

describe("drawer actions", () => {
  it("has the Active switch, Settings, and a ⋯ menu with the other actions", async () => {
    const user = userEvent.setup();
    const p = props(entry(4), { hasSettings: true });
    render(ModDetail, p);
    await settle();
    const sw = screen.getByRole("switch", { name: /^Active: / });
    expect(sw).toHaveAttribute("aria-checked", "true");
    await user.click(sw);
    expect(p.onToggleActive).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    expect(p.onOpenSettings).toHaveBeenCalledTimes(1);

    const more = screen.getByRole("button", { name: "More actions for Mod 4" });
    more.focus();
    await user.keyboard("{Enter}");
    await settle();
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent?.trim())).toEqual([
      "Favorite",
      "Flag",
      "Hide",
      "Change category…",
      "Show file",
      "Find in Browse",
    ]);
    await user.click(within(menu).getByRole("menuitem", { name: "Flag" }));
    expect(p.onToggle).toHaveBeenCalledWith("broken");

    await user.click(more);
    await settle();
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Show file" }));
    expect(revealInFolder).toHaveBeenCalledWith("C:/mods/FS25_Mod4.zip");

    await user.click(more);
    await settle();
    await user.click(within(screen.getByRole("menu")).getByRole("menuitem", { name: "Change category…" }));
    await settle();
    expect(screen.getByRole("combobox", { name: "Category" })).toHaveFocus();
  });

  it("keeps notes, tags, rating, source page, GitHub repo, integrity and Mod ID reachable", async () => {
    render(ModDetail, props(entry(5)));
    await settle();
    for (const name of ["Notes", "Add a tag", "The page you downloaded this mod from", "GitHub repo, as owner/repo"]) {
      expect(screen.getByRole("textbox", { name })).toBeInTheDocument();
    }
    expect(screen.getByRole("button", { name: "3 stars" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Verify integrity" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy Mod ID" })).toBeInTheDocument();
    const headings = screen.getAllByRole("heading", { level: 3 }).map((h) => h.textContent?.trim());
    expect(headings).toEqual(["Details", "Integrity", "Source and updates", "My notes", "Category"]);
  });
});

describe("drawer focus", () => {
  it("moves focus to the title on open", async () => {
    render(ModDetail, props(entry(6)));
    await settle();
    expect(screen.getByRole("heading", { level: 2, name: "Mod 6" })).toHaveFocus();
  });

  it("refocuses the title for another mod, not for a rescan of the same file", async () => {
    const p = props(entry(8));
    const { rerender } = render(ModDetail, p);
    await settle();
    const notes = screen.getByRole("textbox", { name: "Notes" });
    notes.focus();
    await rerender({ ...p, mod: { ...p.mod, mtimeMs: 99 } }); // same path: a fresh scan entry
    await settle();
    expect(notes).toHaveFocus();
    await rerender({ ...p, mod: entry(9) });
    await settle();
    expect(screen.getByRole("heading", { level: 2, name: "Mod 9" })).toHaveFocus();
  });

  it("Esc with the ⋯ menu open closes only the menu", async () => {
    const user = userEvent.setup();
    const p = props(entry(10));
    render(ModDetail, p);
    await settle();
    const more = screen.getByRole("button", { name: "More actions for Mod 10" });
    await user.click(more);
    await settle();
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await settle();
    expect(screen.queryByRole("menu")).toBeNull();
    expect(p.onClose).not.toHaveBeenCalled();
    expect(more).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(p.onClose).toHaveBeenCalledTimes(1);
  });

  it("Esc closes while focus is in the drawer, not while it's elsewhere", async () => {
    const p = props(entry(7));
    render(ModDetail, p);
    await settle();
    await fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(p.onClose).toHaveBeenCalledTimes(1);

    const outside = document.createElement("input");
    document.body.appendChild(outside);
    outside.focus();
    await fireEvent.keyDown(outside, { key: "Escape" });
    expect(p.onClose).toHaveBeenCalledTimes(1);
    outside.remove();
  });

  it("Esc hands focus back to the row the drawer showed, by key, after the list scrolled", async () => {
    const user = userEvent.setup();
    const mods = Array.from({ length: 120 }, (_, i) => entry(100 + i));
    const { component, container } = render(DrawerHarness, { mods });
    await settle();
    component.focusKey(mods[3].path);
    await settle();
    await user.keyboard("{Enter}");
    await settle();
    expect(screen.getByRole("heading", { level: 2, name: "Mod 103" })).toHaveFocus();

    // Scroll the list away from the row (the list keeps its current row rendered; focusKey brings
    // it back into view).
    const vp = container.querySelector<HTMLElement>(".vg-viewport")!;
    vp.scrollTop = 56 * 80;
    await fireEvent.scroll(vp);
    await settle();

    await user.keyboard("{Escape}");
    await settle();
    expect(screen.queryByRole("heading", { level: 2 })).toBeNull();
    const row = container.querySelector<HTMLElement>(`[role="listitem"][data-key="${mods[3].path}"]`);
    expect(row).not.toBeNull();
    expect(row).toHaveFocus();
  });
});
