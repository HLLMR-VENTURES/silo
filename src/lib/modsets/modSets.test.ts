// Mod sets (#251): which set the active mods are on, the My sets comparisons, snapshot names,
// the remembered last-applied set, and the Savegames card lines.
import { describe, expect, it } from "vitest";
import type { Loadout, Savegame } from "../types";
import {
  activeSetLine,
  activeSetState,
  afterSwitch,
  compareSet,
  loadLastApplied,
  saveDateLabel,
  saveModStats,
  saveModsLine,
  sharedFileKind,
  snapshotNameFor,
  sortSets,
  storeLastApplied,
  stripSetName,
  type LastApplied,
} from "./modSets";

const set = (id: number, name: string, mods: string[], kind: Loadout["kind"] = "user", createdAt: number | null = null): Loadout => ({
  id,
  name,
  mods,
  kind,
  createdAt,
});
const harvest = set(1, "Harvest 2026", ["A", "B", "C"]);
const forestry = set(2, "Forestry weekend", ["D", "E"]);
const last = (id: number, name: string, extra: Partial<LastApplied> = {}): LastApplied => ({
  id,
  name,
  snapshotId: null,
  before: null,
  ...extra,
});

describe("activeSetState", () => {
  it("names a set the active mods match exactly, even with no last-applied set", () => {
    expect(activeSetState([forestry, harvest], new Set(["C", "B", "A"]), null)).toEqual({
      kind: "set",
      id: 1,
      name: "Harvest 2026",
    });
  });

  it("is Custom with the change count after edits since the last-applied set", () => {
    const st = activeSetState([harvest], new Set(["A", "B", "X"]), last(1, "Harvest 2026"));
    expect(st).toEqual({ kind: "custom", id: 1, name: "Harvest 2026", changes: 2 });
    expect(activeSetLine(st)).toBe("Custom: 2 changes since Harvest 2026");
    expect(stripSetName(st)).toBe("Custom (2 changes since Harvest 2026)");
  });

  it("doesn't count a set's mods that aren't in the library as changes", () => {
    const lib = new Set(["A", "B"]);
    expect(activeSetState([harvest], new Set(["A", "B"]), last(1, "Harvest 2026"), lib)).toMatchObject({
      kind: "set",
      name: "Harvest 2026",
    });
  });

  it("never matches an automatic snapshot on its own, and says No mod set", () => {
    const snap = set(9, "Before Forestry weekend", ["Q"], "auto");
    const st = activeSetState([snap], new Set(["Q"]), null);
    expect(st).toEqual({ kind: "none" });
    expect(activeSetLine(st)).toBe("No mod set");
    expect(stripSetName(st)).toBeNull();
  });

  it("after Undo (back to the switch's snapshot) counts the set from before the switch", () => {
    const snap = set(9, "Before Forestry weekend", ["A", "B", "X"], "auto");
    const l = last(2, "Forestry weekend", { snapshotId: 9, before: { id: 1, name: "Harvest 2026" } });
    expect(activeSetState([harvest, forestry, snap], new Set(["A", "B", "X"]), l)).toEqual({
      kind: "custom",
      id: 1,
      name: "Harvest 2026",
      changes: 2,
    });
    // Still on the switched-to set: it wins.
    expect(activeSetState([harvest, forestry, snap], new Set(["D", "E"]), l)).toMatchObject({ name: "Forestry weekend" });
  });

  it("forgets a last-applied set that was deleted", () => {
    expect(activeSetState([harvest], new Set(["Z"]), last(5, "Gone"))).toEqual({ kind: "none" });
  });

  it("says unchanged for a single change too, with the singular", () => {
    expect(activeSetLine({ kind: "set", id: 1, name: "Hay" })).toBe("Hay, unchanged");
    expect(activeSetLine({ kind: "custom", id: 1, name: "Hay", changes: 1 })).toBe("Custom: 1 change since Hay");
  });
});

describe("afterSwitch", () => {
  it("remembers the set, the snapshot it saved and where it came from", () => {
    expect(afterSwitch(null, forestry, 9, { kind: "custom", id: 1, name: "Harvest 2026", changes: 2 })).toEqual({
      id: 2,
      name: "Forestry weekend",
      snapshotId: 9,
      before: { id: 1, name: "Harvest 2026" },
    });
    expect(afterSwitch(null, forestry, null, { kind: "none" })?.before).toBeNull();
  });

  it("switching back to that snapshot returns to the set from before", () => {
    const snap = set(9, "Before Forestry weekend", ["A"], "auto");
    const l = last(2, "Forestry weekend", { snapshotId: 9, before: { id: 1, name: "Harvest 2026" } });
    expect(afterSwitch(l, snap, 10, { kind: "set", id: 2, name: "Forestry weekend" })).toEqual(last(1, "Harvest 2026"));
    expect(afterSwitch({ ...l, before: null }, snap, 10, { kind: "none" })).toBeNull();
  });
});

describe("snapshot names and order", () => {
  it("names a snapshot after the set being switched to", () => {
    expect(snapshotNameFor(forestry)).toBe("Before Forestry weekend");
    expect(snapshotNameFor(set(9, "Before X", [], "auto"))).toBe("Before switching back");
  });

  it("lists the user's sets first, then snapshots newest first", () => {
    const a = set(3, "Before A", [], "auto", 100);
    const b = set(4, "Before B", [], "auto", 300);
    expect(sortSets([a, harvest, b, forestry]).map((s) => s.id)).toEqual([1, 2, 4, 3]);
  });
});

describe("compareSet", () => {
  it("says Same, or +on / −parked", () => {
    expect(compareSet(["A", "B"], new Set(["B", "A"])).text).toBe("Same");
    const c = compareSet(["A", "D", "E"], new Set(["A", "B"]));
    expect(c.text).toBe("+2 / −1");
    expect(c.label).toBe("2 turn on and 1 is parked");
    expect(c.same).toBe(false);
    expect(compareSet(["A", "B"], new Set(["A"])).label).toBe("1 turns on");
    expect(compareSet(["A"], new Set(["A", "B", "C"])).label).toBe("2 are parked");
  });

  it("counts mods missing from the library separately", () => {
    const c = compareSet(["A", "Z"], new Set(["A"]), new Set(["A"]));
    expect(c.same).toBe(true);
    expect(c.diff.missing).toEqual(["Z"]);
  });
});

describe("Savegames", () => {
  const save: Savegame = {
    index: 3,
    folder: "savegame3",
    name: "Riverbend",
    mapTitle: "Riverbend Springs",
    saveDate: "2026-08-02",
    mods: [
      { modName: "A", title: "Mod A", version: null, required: true, fileHash: null, isDlc: false },
      { modName: "B", title: null, version: null, required: false, fileHash: null, isDlc: false },
      { modName: "pdlc_x", title: null, version: null, required: false, fileHash: null, isDlc: true },
    ],
  };

  it("counts the save's mods in the library, leaving DLC out", () => {
    const st = saveModStats(save, new Set(["A"]));
    expect(st.total).toBe(2);
    expect(st.missing.map((m) => m.modName)).toEqual(["B"]);
    expect(saveModsLine(st)).toBe("1 of 2 mods in your library");
    expect(saveModsLine(saveModStats(save, new Set(["A", "B"])))).toBe("All 2 mods in your library");
    expect(saveModsLine(saveModStats({ ...save, mods: [] }, new Set()))).toBe("Uses no mods");
  });

  it("formats the game's save date", () => {
    expect(saveDateLabel("2026-08-02")).toBe("Aug 2, 2026");
    expect(saveDateLabel("yesterday")).toBe("yesterday");
    expect(saveDateLabel(null)).toBe("");
  });
});

describe("shared files", () => {
  it("tells a Multiplayer check file from a mod set file", () => {
    expect(sharedFileKind("C:\\x\\Server.SILOMP")).toBe("multiplayer");
    expect(sharedFileKind("/x/harvest.silo")).toBe("set");
    expect(sharedFileKind("/x/collection.json")).toBe("set");
  });
});

describe("last-applied storage", () => {
  function fakeStorage() {
    const m = new Map<string, string>();
    return {
      getItem: (k: string) => m.get(k) ?? null,
      setItem: (k: string, v: string) => void m.set(k, v),
      removeItem: (k: string) => void m.delete(k),
      raw: m,
    };
  }

  it("round-trips, forgets, and ignores junk", () => {
    const s = fakeStorage();
    const v = last(2, "Forestry", { snapshotId: 9, before: { id: 1, name: "Harvest" } });
    storeLastApplied(v, s);
    expect(loadLastApplied(s)).toEqual(v);
    storeLastApplied(null, s);
    expect(loadLastApplied(s)).toBeNull();
    s.raw.set("silo.modSets.lastApplied", "{not json");
    expect(loadLastApplied(s)).toBeNull();
    s.raw.set("silo.modSets.lastApplied", JSON.stringify({ id: "2", name: 3 }));
    expect(loadLastApplied(s)).toBeNull();
  });

  it("survives storage that throws", () => {
    const bad = {
      getItem: () => {
        throw new Error("denied");
      },
      setItem: () => {
        throw new Error("denied");
      },
      removeItem: () => {
        throw new Error("denied");
      },
    };
    expect(loadLastApplied(bad)).toBeNull();
    expect(() => storeLastApplied(last(1, "x"), bad)).not.toThrow();
  });
});
