// @vitest-environment jsdom
// Mod sets store (#251): switching confirms with the diff, then saves a "Before …" snapshot and
// offers Undo; a cancel or a refused write changes nothing; Update / Delete / Use this save's mods.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getLoadouts: vi.fn(() => Promise.resolve([])),
  saveLoadout: vi.fn(() => Promise.resolve(50)),
  deleteLoadout: vi.fn(() => Promise.resolve()),
  saveAutoSnapshot: vi.fn(() => Promise.resolve(60)),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));
vi.mock("../ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../ui")>()),
  ask: vi.fn(() => Promise.resolve(true)),
  askText: vi.fn(() => Promise.resolve(null)),
  toast: vi.fn(() => ({ id: 1, dismiss: vi.fn() })),
}));

import { deleteLoadout, getLoadouts, saveAutoSnapshot, saveLoadout } from "../api";
import { ask, askText, toast } from "../ui";
import type { Loadout, Savegame } from "../types";
import { createModSets, switchSummary } from "./modSetsStore.svelte";
import { activeSetDiff } from "../loadoutSafety";

const harvest: Loadout = { id: 1, name: "Harvest 2026", mods: ["A", "B"], kind: "user", createdAt: null };
const forestry: Loadout = { id: 2, name: "Forestry weekend", mods: ["C", "D", "Z"], kind: "user", createdAt: null };

function setup(active: string[] = ["A", "B"], library = ["A", "B", "C", "D"]) {
  let current = new Set(active);
  const setActiveSetTo = vi.fn(async (mods: string[], _label?: string) => {
    current = new Set(mods);
    return true as boolean | null;
  });
  const undo = { offer: vi.fn(), dismiss: vi.fn() };
  const refuse = vi.fn();
  const editRefused = vi.fn(() => false);
  const store = createModSets({
    active: () => current,
    library: () => new Set(library),
    title: (t) => `Mod ${t}`,
    setActiveSetTo,
    undo: undo as never,
    editRefused,
    refuse,
  });
  return { store, setActiveSetTo, undo, refuse, editRefused, active: () => current };
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(getLoadouts).mockResolvedValue([harvest, forestry]);
  vi.mocked(ask).mockResolvedValue(true);
});

describe("switchTo", () => {
  it("shows the diff, switches, saves a Before snapshot of the previous mods and offers Undo", async () => {
    const t = setup();
    await t.store.load();
    await t.store.switchTo(forestry);
    const opts = vi.mocked(ask).mock.calls[0][0];
    expect(opts.title).toBe("Switch to Forestry weekend?");
    expect(opts.body).toContain("2 mods turn on and 2 are parked. Nothing is deleted.");
    expect(opts.body).toContain("1 mod in this set isn't in your library");
    expect(opts.body).toContain("Silo saves your current mods as “Before Forestry weekend” so you can switch back.");
    expect(opts.confirm).toBe("Switch");
    expect(t.setActiveSetTo).toHaveBeenCalledWith(["C", "D", "Z"], "Switching to Forestry weekend");
    expect(saveAutoSnapshot).toHaveBeenCalledWith("Before Forestry weekend", ["A", "B"]);
    expect(t.undo.dismiss).toHaveBeenCalled();
    expect(t.undo.offer).toHaveBeenCalledWith(
      expect.objectContaining({ message: "Switched to Forestry weekend.", previous: ["A", "B"], expected: ["C", "D", "Z"] }),
    );
    expect(t.store.lastApplied).toEqual({
      id: 2,
      name: "Forestry weekend",
      snapshotId: 60,
      before: { id: 1, name: "Harvest 2026" },
    });
    // Remembered across restarts (a frontend pref).
    expect(JSON.parse(localStorage.getItem("silo.modSets.lastApplied")!).id).toBe(2);
  });

  it("changes nothing when cancelled", async () => {
    const t = setup();
    await t.store.load();
    vi.mocked(ask).mockResolvedValue(false);
    await t.store.switchTo(forestry);
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
    expect(t.undo.offer).not.toHaveBeenCalled();
    expect(t.store.lastApplied).toBeNull();
  });

  it("saves no snapshot when the write was refused (another one is running)", async () => {
    const t = setup();
    await t.store.load();
    t.setActiveSetTo.mockResolvedValueOnce(null);
    await t.store.switchTo(forestry);
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
    expect(t.undo.offer).not.toHaveBeenCalled();
  });

  it("doesn't ask when nothing would change", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    await t.store.switchTo(harvest);
    expect(ask).not.toHaveBeenCalled();
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(toast).toHaveBeenCalledWith({ message: "Already using Harvest 2026." });
  });

  it("with nothing active, switches without promising a snapshot", async () => {
    const t = setup([]);
    await t.store.load();
    await t.store.switchTo(harvest);
    expect(vi.mocked(ask).mock.calls[0][0].body).not.toContain("Before");
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
    expect(t.setActiveSetTo).toHaveBeenCalled();
  });

  it("an automatic snapshot offers Switch back", async () => {
    const snap: Loadout = { id: 9, name: "Before Forestry weekend", mods: ["C"], kind: "auto", createdAt: 1 };
    vi.mocked(getLoadouts).mockResolvedValue([harvest, snap]);
    const t = setup();
    await t.store.load();
    await t.store.switchTo(snap);
    expect(vi.mocked(ask).mock.calls[0][0]).toMatchObject({
      title: "Switch back to Before Forestry weekend?",
      confirm: "Switch back",
    });
    expect(saveAutoSnapshot).toHaveBeenCalledWith("Before switching back", ["A", "B"]);
  });

  it("the state follows the switch: the set, then Custom after one change", async () => {
    const t = setup();
    await t.store.load();
    expect(t.store.state).toMatchObject({ kind: "set", name: "Harvest 2026" });
    await t.store.switchTo(forestry);
    expect(t.store.state).toMatchObject({ kind: "set", name: "Forestry weekend" });
  });
});

describe("Undo after a switch", () => {
  it("names the set from before the switch once the previous mods are back", async () => {
    const t = setup(["A", "B", "X"]);
    vi.mocked(getLoadouts).mockResolvedValue([harvest, forestry]);
    await t.store.load();
    // Custom since Harvest: remembered as last applied.
    await t.store.switchTo(harvest);
    expect(t.store.state).toMatchObject({ kind: "set", name: "Harvest 2026" });
    t.setActiveSetTo.mockClear();
    await t.setActiveSetTo(["A", "B", "X"], "edit");
    expect(t.store.state).toMatchObject({ kind: "custom", name: "Harvest 2026", changes: 1 });
    const snap: Loadout = { id: 60, name: "Before Forestry weekend", mods: ["A", "B", "X"], kind: "auto", createdAt: 1 };
    vi.mocked(getLoadouts).mockResolvedValue([harvest, forestry, snap]);
    await t.store.switchTo(forestry);
    expect(t.store.state).toMatchObject({ kind: "set", name: "Forestry weekend" });
    // Undo puts the previous mods back (through App's reapply path).
    const offer = t.undo.offer.mock.calls.at(-1)![0];
    await t.setActiveSetTo(offer.previous, "Switching back");
    expect(t.store.state).toEqual({ kind: "custom", id: 1, name: "Harvest 2026", changes: 1 });
  });
});

describe("updateFromCurrent and remove", () => {
  it("replaces a set's mods after the confirmation, keeping its id", async () => {
    const t = setup(["A", "B", "C"]);
    await t.store.load();
    await t.store.updateFromCurrent(harvest);
    expect(vi.mocked(ask).mock.calls[0][0]).toMatchObject({
      title: "Replace Harvest 2026 with your current 3 mods?",
      confirm: "Replace",
    });
    expect(vi.mocked(ask).mock.calls[0][0].body).toContain("It had 2. 1 is added and none removed.");
    expect(saveLoadout).toHaveBeenCalledWith(1, "Harvest 2026", ["A", "B", "C"]);
    expect(toast).toHaveBeenCalledWith(expect.objectContaining({ message: "Saved your current mods to Harvest 2026." }));
  });

  it("deletes after the confirmation and Undo saves it again", async () => {
    const t = setup();
    await t.store.load();
    await t.store.remove(forestry);
    expect(vi.mocked(ask).mock.calls[0][0]).toMatchObject({ title: "Delete Forestry weekend?", tone: "danger" });
    expect(deleteLoadout).toHaveBeenCalledWith(2);
    const undo = vi.mocked(toast).mock.calls.at(-1)![0].action!;
    expect(undo.label).toBe("Undo");
    await undo.run();
    expect(saveLoadout).toHaveBeenCalledWith(null, "Forestry weekend", ["C", "D", "Z"]);
  });

  it("never writes your edits into an automatic set", async () => {
    const t = setup();
    await t.store.updateFromCurrent({ id: 9, name: "Before X", mods: [], kind: "auto", createdAt: 1 });
    expect(t.refuse).toHaveBeenCalledWith("Before X is an automatic set. Save your current mods as a new set instead.");
    expect(ask).not.toHaveBeenCalled();
    expect(saveLoadout).not.toHaveBeenCalled();
  });

  it("refuses while a library change runs", async () => {
    const t = setup();
    t.editRefused.mockReturnValue(true);
    await t.store.remove(forestry);
    await t.store.updateFromCurrent(harvest);
    expect(ask).not.toHaveBeenCalled();
    expect(deleteLoadout).not.toHaveBeenCalled();
  });
});

describe("saveCurrent", () => {
  it("checks the name against the user's sets only and saves a new set", async () => {
    vi.mocked(getLoadouts).mockResolvedValue([
      harvest,
      { id: 9, name: "Before X", mods: [], kind: "auto", createdAt: 1 },
    ]);
    const t = setup();
    await t.store.load();
    vi.mocked(askText).mockImplementation(async (o) => {
      expect(o.validate!("Before X")).toBeNull();
      expect(o.validate!("harvest 2026")).toEqual({
        warn: "You already have a set called Harvest 2026. Replace it?",
        confirm: "Replace",
      });
      return "Weekend";
    });
    await t.store.saveCurrent();
    expect(saveLoadout).toHaveBeenCalledWith(null, "Weekend", ["A", "B"]);
    expect(t.store.lastApplied).toMatchObject({ id: 50, name: "Weekend" });
  });
});

describe("useSave", () => {
  const save: Savegame = {
    index: 1,
    folder: "savegame1",
    name: "Riverbend",
    mapTitle: null,
    saveDate: null,
    mods: [
      { modName: "C", title: null, version: null, required: true, fileHash: null, isDlc: false },
      { modName: "Q", title: null, version: null, required: true, fileHash: null, isDlc: false },
      { modName: "pdlc_x", title: null, version: null, required: false, fileHash: null, isDlc: true },
    ],
  };

  it("uses the same switch confirmation and saves a set named after the save only after it switched", async () => {
    const t = setup();
    await t.store.load();
    await t.store.useSave(save);
    const opts = vi.mocked(ask).mock.calls[0][0];
    expect(opts.title).toBe("Use Riverbend's mods?");
    expect(opts.body).toContain("Silo saves 1 mod from this save as a set called Riverbend and switches to it.");
    expect(opts.body).toContain("1 mod the save used isn't in your library and is left out.");
    expect(t.setActiveSetTo).toHaveBeenCalledWith(["C"], "Switching to Riverbend");
    expect(saveAutoSnapshot).toHaveBeenCalledWith("Before Riverbend", ["A", "B"]);
    expect(saveLoadout).toHaveBeenCalledWith(null, "Riverbend", ["C"]);
    expect(t.setActiveSetTo.mock.invocationCallOrder[0]).toBeLessThan(vi.mocked(saveLoadout).mock.invocationCallOrder[0]);
  });

  it("cancelled: nothing is saved or switched", async () => {
    const t = setup();
    await t.store.load();
    vi.mocked(ask).mockResolvedValue(false);
    await t.store.useSave(save);
    expect(saveLoadout).not.toHaveBeenCalled();
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
  });

  it("says so when none of the save's mods are in the library", async () => {
    const t = setup(["A"], ["A"]);
    await t.store.useSave(save);
    expect(t.refuse).toHaveBeenCalledWith("None of Riverbend's mods are in your library yet.");
    expect(ask).not.toHaveBeenCalled();
  });
});

describe("switchSummary", () => {
  it("lists the changes by title and caps the list", () => {
    const on = Array.from({ length: 60 }, (_, i) => `M${i}`);
    const s = switchSummary(activeSetDiff([], on), (t) => `T${t}`);
    expect(s.list).toHaveLength(51);
    expect(s.list[0]).toBe("On: TM0");
    expect(s.list.at(-1)).toBe("…and 10 more");
    expect(s.facts.map((f) => f.label)).toEqual(["Turn on", "Park"]);
  });
});
