// What set_active can park (#C12): loose zips are filed first, folder mods can't be parked, and a
// diagnosis round checks afterwards that nothing it meant to park is still loaded.
import { describe, expect, it } from "vitest";
import { fixedOnText, isLooseZip, isUnparkable, parkPlan, readyAfterFiling, stillOn } from "./parkable";

const mod = (techName: string, organized: boolean, kind = "zip") => ({ techName, title: techName, organized, kind });
const library = [
  mod("Filed", true),
  mod("LooseZip", false),
  mod("Folder", false, "dir"),
  mod("FiledFolder", true, "dir"),
  mod("Kept", false),
];

describe("parkPlan", () => {
  it("splits what a new active set leaves out into loose zips to file and folder mods that stay", () => {
    const active = new Set(["Filed", "LooseZip", "Folder", "FiledFolder", "Kept"]);
    const plan = parkPlan(library, active, ["Kept"]);
    expect(plan.loose.map((m) => m.techName)).toEqual(["LooseZip"]);
    expect(plan.fixed.map((m) => m.techName)).toEqual(["Folder"]);
  });

  it("asks nothing when only filed mods are parked, or nothing is", () => {
    expect(parkPlan(library, new Set(["Filed"]), [])).toEqual({ loose: [], fixed: [] });
    expect(parkPlan(library, new Set(["LooseZip"]), ["LooseZip"])).toEqual({ loose: [], fixed: [] });
  });

  it("knows which mods are which", () => {
    expect(isLooseZip(mod("x", false))).toBe(true);
    expect(isLooseZip(mod("x", true))).toBe(false);
    expect(isUnparkable(mod("x", false, "dir"))).toBe(true);
    expect(isUnparkable(mod("x", true, "dir"))).toBe(false);
  });
});

describe("stillOn (a round's post-condition)", () => {
  it("names the mods the round meant to park that are still active", () => {
    const start = new Set(["A", "B", "C", "MAP"]);
    // set_active skipped the loose C: it's still in the mods folder.
    expect(stillOn(["MAP", "A", "C"], ["MAP", "A"], start)).toEqual(["C"]);
    expect(stillOn(["MAP", "A"], ["MAP", "A"], start)).toEqual([]);
  });

  it("ignores a mod that wasn't part of the search (added since it started)", () => {
    expect(stillOn(["A", "NEW"], ["A"], new Set(["A", "B"]))).toEqual([]);
  });
});

describe("fixedOnText", () => {
  it("names one mod, counts several", () => {
    expect(fixedOnText(["Big Pack"])).toMatch(/^Big Pack stays on: Silo can't park unzipped folder mods\. Move its folder/);
    expect(fixedOnText(["A", "B"])).toMatch(/^2 mods stay on: .* their folders .* loading them\.$/);
  });
});

describe("readyAfterFiling (before a diagnosis starts)", () => {
  const filedA = { ...mod("A", true), isMap: false };
  const looseB = { ...mod("B", false), isMap: false };
  const looseMap = { ...mod("MAP", false), isMap: true };

  it("starts when the filed mods are in the library and still active", () => {
    expect(readyAfterFiling(true, [filedA], [filedA, looseMap], new Set(["A", "MAP"]))).toBe(true);
  });

  it("doesn't start when a filed mod wasn't linked back (it would stay parked at the end)", () => {
    expect(readyAfterFiling(true, [filedA], [filedA], new Set())).toBe(false);
  });

  it("doesn't start when a zip is still loose, or the filing was refused or had problems", () => {
    expect(readyAfterFiling(true, [looseB], [looseB], new Set(["B"]))).toBe(false);
    expect(readyAfterFiling(null, [filedA], [filedA], new Set(["A"]))).toBe(false);
    expect(readyAfterFiling(false, [filedA], [filedA], new Set(["A"]))).toBe(false);
  });
});
