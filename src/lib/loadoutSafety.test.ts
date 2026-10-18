import { describe, expect, it } from "vitest";
import { activeSetDiff, findLoadoutByName, loadoutNameCheck } from "./loadoutSafety";

describe("loadoutNameCheck", () => {
  it("rejects an empty or blank name", () => {
    expect(loadoutNameCheck("", [])).toEqual({ error: "Give the loadout a name" });
    expect(loadoutNameCheck("   ", ["A"])).toEqual({ error: "Give the loadout a name" });
  });

  it("accepts a new name", () => {
    expect(loadoutNameCheck("Harvest", ["Planting", "Hay"])).toBeNull();
  });

  it("warns on a duplicate, ignoring case and spaces, naming the existing loadout", () => {
    expect(loadoutNameCheck("  harvest ", ["Planting", "Harvest"])).toEqual({
      warn: "You already have a loadout called Harvest. Replace it?",
      confirm: "Replace",
    });
  });
});

describe("findLoadoutByName", () => {
  it("matches case-insensitively", () => {
    const ls = [
      { id: 1, name: "Hay" },
      { id: 2, name: "Harvest" },
    ];
    expect(findLoadoutByName("HARVEST", ls)?.id).toBe(2);
    expect(findLoadoutByName("Silage", ls)).toBeUndefined();
  });
});

describe("activeSetDiff", () => {
  it("lists what turns on and what is parked", () => {
    expect(activeSetDiff(["a", "b", "c"], ["b", "c", "d"])).toEqual({
      on: ["d"],
      off: ["a"],
      missing: [],
    });
  });

  it("is empty when the sets match in any order", () => {
    expect(activeSetDiff(["a", "b"], ["b", "a"])).toEqual({ on: [], off: [], missing: [] });
  });

  it("moves target mods outside the library to missing instead of on", () => {
    const lib = new Set(["a", "b", "d"]);
    expect(activeSetDiff(["a"], ["a", "d", "x"], lib)).toEqual({
      on: ["d"],
      off: [],
      missing: ["x"],
    });
  });

  it("ignores duplicates", () => {
    expect(activeSetDiff(["a", "a"], ["b", "b"])).toEqual({ on: ["b"], off: ["a"], missing: [] });
  });

  it("handles empty sides", () => {
    expect(activeSetDiff([], ["a"])).toEqual({ on: ["a"], off: [], missing: [] });
    expect(activeSetDiff(["a"], [])).toEqual({ on: [], off: ["a"], missing: [] });
  });
});
