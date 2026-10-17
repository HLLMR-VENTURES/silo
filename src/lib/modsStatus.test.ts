import { describe, expect, it } from "vitest";
import { rootsFor } from "./modsStatus";
import type { ModsRootStatus } from "./api";

function status(over: Partial<ModsRootStatus> = {}): ModsRootStatus {
  return {
    path: "C:/mods",
    source: "user",
    gameReads: "C:/mods",
    mismatch: false,
    available: true,
    modsFolderMissing: false,
    known: [],
    canWriteGameSetting: true,
    gameSettingProblem: null,
    unresolvedOverride: null,
    layoutProblem: null,
    ...over,
  };
}

describe("rootsFor", () => {
  it("scans the live folder", () => {
    expect(rootsFor(status())).toEqual(["C:/mods"]);
  });

  it("scans nothing for a missing or unavailable folder", () => {
    expect(rootsFor(status({ path: null }))).toEqual([]);
    expect(rootsFor(status({ available: false }))).toEqual([]);
  });

  it("scans nothing while the folder is blocked", () => {
    expect(
      rootsFor(status({ layoutProblem: "That folder overlaps another mods folder's archive." })),
    ).toEqual([]);
  });
});
