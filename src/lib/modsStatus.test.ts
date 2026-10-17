import { describe, expect, it } from "vitest";
import { rootsFor, switchBlockReason, type SwitchState } from "./modsStatus";
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
    layoutProblemCause: null,
    layoutConflict: null,
    ...over,
  };
}

describe("switchBlockReason", () => {
  const idle: SwitchState = { moveUnfinished: false, scanning: false, busy: null, blocked: false };

  it("waits for a move, a scan or a busy action first", async () => {
    const none = async () => false;
    expect(await switchBlockReason({ ...idle, moveUnfinished: true }, none)).toBe(
      "Finish or cancel the archive move first.",
    );
    expect(await switchBlockReason({ ...idle, scanning: true }, none)).toBe(
      "Wait for the scan to finish.",
    );
    expect(await switchBlockReason({ ...idle, busy: "Organizing" }, none)).toBe(
      "Wait for “Organizing” to finish.",
    );
    expect(await switchBlockReason(idle, none)).toBeNull();
  });

  it("keeps an interrupted bisection's folder until it's dealt with", async () => {
    expect(await switchBlockReason(idle, async () => true)).toMatch(/guided bisection/);
  });

  it("never gates leaving a blocked folder on a bisection snapshot", async () => {
    let asked = false;
    const snap = async () => {
      asked = true;
      return true;
    };
    expect(await switchBlockReason({ ...idle, blocked: true }, snap)).toBeNull();
    expect(asked).toBe(false);
    // A move still comes first, even then.
    expect(await switchBlockReason({ ...idle, blocked: true, moveUnfinished: true }, snap)).toBe(
      "Finish or cancel the archive move first.",
    );
  });
});

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
