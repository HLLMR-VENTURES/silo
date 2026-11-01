// @vitest-environment jsdom
// Mod sets store (#251): switching confirms with the diff, then saves a "Before …" snapshot and
// offers Undo; a cancel or a refused write changes nothing; Update / Delete / Use this save's mods.
import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  getLoadouts: vi.fn(() => Promise.resolve([])),
  saveLoadout: vi.fn(() => Promise.resolve(50)),
  replaceLoadoutMods: vi.fn(() => Promise.resolve()),
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

import { deleteLoadout, getLoadouts, replaceLoadoutMods, saveAutoSnapshot, saveLoadout } from "../api";
import { ask, askText, toast } from "../ui";
import type { Loadout, Savegame } from "../types";
import { createModSets, switchSummary } from "./modSetsStore.svelte";
import { activeSetDiff } from "../loadoutSafety";
import { membershipEdits, membershipRows, type Membership } from "./membership";

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
  const activeUndoBlocked = vi.fn((_expected: string[]) => null as string | null);
  const park = vi.fn();
  const settle = vi.fn(async () => true);
  const store = createModSets({
    active: () => current,
    library: () => new Set(library),
    title: (t) => `Mod ${t}`,
    setActiveSetTo,
    settle,
    undo: undo as never,
    editRefused,
    refuse,
    activeUndoBlocked,
    park,
  });
  return { store, setActiveSetTo, settle, undo, refuse, editRefused, activeUndoBlocked, park, active: () => current };
}

beforeEach(() => {
  vi.clearAllMocks();
  localStorage.clear();
  vi.mocked(getLoadouts).mockResolvedValue([harvest, forestry]);
  vi.mocked(ask).mockResolvedValue(true);
});

describe("switchTo", () => {
  it("Undo expects what the switch left, including a mod Silo couldn't park", async () => {
    const t = setup(["A", "B", "F"], ["A", "B", "C", "D", "F"]);
    // App's switch keeps an unzipped folder mod (F) on: set_active can't park it.
    const live = t.active() as Set<string>;
    t.setActiveSetTo.mockImplementation(async (mods: string[]) => {
      live.clear();
      for (const m of [...mods, "F"]) live.add(m);
      return true;
    });
    await t.store.load();
    await t.store.switchTo(forestry);
    expect(t.undo.offer).toHaveBeenCalledWith(expect.objectContaining({ expected: ["C", "D", "Z", "F"] }));
  });

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

  it("a switch that failed with nothing changed isn't recorded: no snapshot, not remembered, no Undo (#347)", async () => {
    const t = setup();
    await t.store.load();
    // App's switch threw (the archive drive was unplugged, say), reported it and rescanned: the
    // mods folder still holds the old set, so it says nothing changed.
    t.setActiveSetTo.mockResolvedValueOnce(null);
    await t.store.switchTo(forestry);
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
    expect(t.undo.offer).not.toHaveBeenCalled();
    expect(t.store.lastApplied).toBeNull();
    expect(t.store.state).toMatchObject({ kind: "set", name: "Harvest 2026" });
  });

  it("a switch that changed some mods but finished with problems is recorded with a warning", async () => {
    const t = setup();
    await t.store.load();
    t.setActiveSetTo.mockResolvedValueOnce(false);
    await t.store.switchTo(forestry);
    expect(saveAutoSnapshot).toHaveBeenCalledWith("Before Forestry weekend", ["A", "B"]);
    expect(t.undo.offer).toHaveBeenCalledWith(
      expect.objectContaining({ message: "Switch to Forestry weekend finished with problems.", tone: "warn" }),
    );
  });

  it("Undo expects what the switch left, not a change a scan brought in while the snapshot saved (#347)", async () => {
    const t = setup();
    await t.store.load();
    vi.mocked(saveAutoSnapshot).mockImplementationOnce(async () => {
      (t.active() as Set<string>).add("X"); // turned on outside Silo; a scan landed meanwhile
      return 60;
    });
    await t.store.switchTo(forestry);
    expect(t.undo.offer).toHaveBeenCalledWith(expect.objectContaining({ expected: ["C", "D", "Z"] }));
  });

  it("waits for a running scan before it reads the active mods, and asks nothing when it still runs (#347)", async () => {
    const t = setup();
    await t.store.load();
    t.settle.mockResolvedValueOnce(false);
    await t.store.switchTo(forestry);
    expect(ask).not.toHaveBeenCalled();
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
  });

  it("waits again after the confirmation, before it keeps the mods for the snapshot (#347)", async () => {
    const t = setup();
    await t.store.load();
    // A scan started while the dialog was open, and still runs.
    t.settle.mockResolvedValueOnce(true).mockResolvedValueOnce(false);
    await t.store.switchTo(forestry);
    expect(ask).toHaveBeenCalled();
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
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
    // Said once (L14): not again as "stays off until you get it", since it isn't in the set.
    expect(opts.body).not.toMatch(/until you get/);
    expect(opts.facts?.map((f) => f.label)).not.toContain("Not in your library");
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

  it("a switch that failed with nothing changed saves no set named after the save (#347)", async () => {
    const t = setup();
    await t.store.load();
    t.setActiveSetTo.mockResolvedValueOnce(null);
    await t.store.useSave(save);
    expect(saveLoadout).not.toHaveBeenCalled();
    expect(saveAutoSnapshot).not.toHaveBeenCalled();
    expect(t.undo.offer).not.toHaveBeenCalled();
    expect(t.store.lastApplied).toBeNull();
  });

  it("reads the library after a running scan, so a mod it finds counts (#347)", async () => {
    const library = ["A", "B", "C"];
    const t = setup(["A", "B"], library);
    await t.store.load();
    // The scan running when the user clicked finds Q.
    t.settle.mockImplementationOnce(async () => {
      library.push("Q");
      return true;
    });
    await t.store.useSave(save);
    expect(t.setActiveSetTo).toHaveBeenCalledWith(["C", "Q"], "Switching to Riverbend");
    expect(saveLoadout).toHaveBeenCalledWith(null, "Riverbend", ["C", "Q"]);
  });

  it("asks nothing while a scan still runs (#347)", async () => {
    const t = setup();
    await t.store.load();
    t.settle.mockResolvedValueOnce(false);
    await t.store.useSave(save);
    expect(ask).not.toHaveBeenCalled();
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
    expect(s.list[0]).toBe("Turn on: TM0");
    expect(s.list.at(-1)).toBe("…and 10 more");
    expect(s.facts.map((f) => f.label)).toEqual(["Turn on", "Park"]);
  });
});

describe("applyMembership (the Library's Mod sets… dialog, #323)", () => {
  // A tiny database, so reloads see what was written.
  let db: Loadout[];
  beforeEach(() => {
    db = [harvest, forestry].map((l) => ({ ...l, mods: [...l.mods] }));
    vi.mocked(getLoadouts).mockImplementation(async () => db.map((l) => ({ ...l, mods: [...l.mods] })));
    vi.mocked(replaceLoadoutMods).mockImplementation(async (edits) => {
      for (const e of edits) db.find((l) => l.id === e.id)!.mods = [...e.mods];
    });
  });
  const editsFor = (t: ReturnType<typeof setup>, names: string[], chosen: [number, Membership][]) =>
    membershipEdits(membershipRows(t.store.sets, names), names, new Map(chosen));
  const lastToast = () => vi.mocked(toast).mock.calls.at(-1)![0];

  it("writes every edited set in one call; a set you're not on leaves your mods alone", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    const edits = editsFor(t, ["A"], [[2, "all"]]);
    expect(await t.store.applyMembership(["A"], edits)).toBe(true);
    expect(replaceLoadoutMods).toHaveBeenCalledTimes(1);
    expect(replaceLoadoutMods).toHaveBeenCalledWith([{ id: 2, mods: ["C", "D", "Z", "A"] }]);
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(lastToast().message).toBe("Added Mod A to Forestry weekend.");
    expect(lastToast().secondaryAction).toBeUndefined();
  });

  it("turns on what the set you're on gains first, so you stay on it", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    expect(t.store.state).toMatchObject({ kind: "set", id: 1 });
    await t.store.applyMembership(["C"], editsFor(t, ["C"], [[1, "all"]]));
    expect(t.setActiveSetTo).toHaveBeenCalledWith(["A", "B", "C"], "Turning on mods");
    expect(t.undo.dismiss).toHaveBeenCalled();
    expect(lastToast().message).toBe("Added Mod C to Harvest 2026. Turned on Mod C.");
    expect(t.store.state).toMatchObject({ kind: "set", id: 1 });
  });

  it("saves nothing when the switch is refused or cancelled", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    t.setActiveSetTo.mockResolvedValueOnce(null);
    expect(await t.store.applyMembership(["B"], editsFor(t, ["B"], [[1, "none"]]))).toBe(false);
    expect(replaceLoadoutMods).not.toHaveBeenCalled();
    expect(toast).not.toHaveBeenCalled();
  });

  it("saves nothing while a scan still runs (#347)", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    t.settle.mockResolvedValueOnce(false);
    expect(await t.store.applyMembership(["B"], editsFor(t, ["B"], [[1, "none"]]))).toBe(false);
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(replaceLoadoutMods).not.toHaveBeenCalled();
  });

  it("saves nothing while a library change runs", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    t.editRefused.mockReturnValue(true);
    expect(await t.store.applyMembership(["A"], editsFor(t, ["A"], [[2, "all"]]))).toBe(false);
    expect(replaceLoadoutMods).not.toHaveBeenCalled();
  });

  it("offers Park them too while mods removed from sets are still on", async () => {
    const t = setup(["A", "B", "C"]);
    await t.store.load();
    // Not on a set (A, B, C matches none): removing C from Forestry parks nothing by itself.
    await t.store.applyMembership(["C"], editsFor(t, ["C"], [[2, "none"]]));
    const opts = lastToast();
    expect(opts.message).toBe("Removed Mod C from Forestry weekend.");
    expect(opts.secondaryAction?.label).toBe("Park it too");
    await opts.secondaryAction!.run();
    expect(t.park).toHaveBeenCalledWith(["C"]);
  });

  it("says Park them too for more than one mod still on", async () => {
    const t = setup(["A", "B", "C", "D"]);
    await t.store.load();
    await t.store.applyMembership(["C", "D"], editsFor(t, ["C", "D"], [[2, "none"]]));
    expect(lastToast().secondaryAction?.label).toBe("Park them too");
  });

  it("Undo puts every set back and reverses the switch", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    await t.store.applyMembership(
      ["B"],
      editsFor(t, ["B"], [
        [1, "none"],
        [2, "all"],
      ]),
    );
    expect(t.setActiveSetTo).toHaveBeenLastCalledWith(["A"], "Parking mods");
    const opts = lastToast();
    expect(opts.message).toBe("Added Mod B to Forestry weekend and removed it from Harvest 2026. Parked Mod B.");
    // Parked by the save already: nothing left to park.
    expect(opts.secondaryAction).toBeUndefined();
    await opts.action!.run();
    expect(t.activeUndoBlocked).toHaveBeenCalledWith(["A"]);
    expect(t.setActiveSetTo).toHaveBeenLastCalledWith(["A", "B"], "Switching back");
    expect(replaceLoadoutMods).toHaveBeenLastCalledWith([
      { id: 1, mods: ["A", "B"] },
      { id: 2, mods: ["C", "D", "Z"] },
    ]);
    expect(db.map((l) => l.mods)).toEqual([
      ["A", "B"],
      ["C", "D", "Z"],
    ]);
  });

  it("Undo waits for a running scan before its check, and comes back when the scan still runs (#347)", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    await t.store.applyMembership(["B"], editsFor(t, ["B"], [[1, "none"]]));
    const opts = lastToast();
    vi.mocked(toast).mockClear();
    vi.mocked(replaceLoadoutMods).mockClear();
    t.setActiveSetTo.mockClear();
    t.settle.mockResolvedValueOnce(false);
    await opts.action!.run();
    expect(t.activeUndoBlocked).not.toHaveBeenCalled();
    expect(t.setActiveSetTo).not.toHaveBeenCalled();
    expect(replaceLoadoutMods).not.toHaveBeenCalled();
    // The same Undo again, to try once the scan is done.
    expect(lastToast()).toMatchObject({ message: opts.message, action: { label: "Undo" } });
    await lastToast().action!.run();
    expect(t.setActiveSetTo).toHaveBeenLastCalledWith(["A", "B"], "Switching back");
  });

  it("Undo expects what the switch left, not a change a scan brought in while the sets saved (#347)", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    vi.mocked(replaceLoadoutMods).mockImplementationOnce(async (edits) => {
      for (const e of edits) db.find((l) => l.id === e.id)!.mods = [...e.mods];
      (t.active() as Set<string>).add("X");
    });
    await t.store.applyMembership(["B"], editsFor(t, ["B"], [[1, "none"]]));
    await lastToast().action!.run();
    expect(t.activeUndoBlocked).toHaveBeenCalledWith(["A"]);
  });

  it("Undo refuses once a set changed since", async () => {
    const t = setup(["A", "B"]);
    await t.store.load();
    await t.store.applyMembership(["A"], editsFor(t, ["A"], [[2, "all"]]));
    const undo = lastToast().action!;
    db[1].mods = ["Q"];
    await t.store.load();
    vi.mocked(replaceLoadoutMods).mockClear();
    await undo.run();
    expect(lastToast()).toMatchObject({ message: "Can't undo: Forestry weekend changed since.", tone: "warn" });
    expect(replaceLoadoutMods).not.toHaveBeenCalled();
  });
});
