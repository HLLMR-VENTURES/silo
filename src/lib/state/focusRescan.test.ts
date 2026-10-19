// Coming back to the window rescans the library, but never in the middle of a mods-folder
// switch: the folder picker and its dialogs take focus, and a scan started then reads the old
// folder and lands its mods under the new one.
import { describe, expect, it } from "vitest";
import { FOCUS_RESCAN_AFTER_MS, focusRescanAllowed, waitWhile } from "./rescanGate";

const idle = {
  scanning: false,
  writeBlocked: false,
  moveRunning: false,
  bisecting: false,
  switchingFolder: false,
  hasRoots: true,
  sinceLastScanMs: FOCUS_RESCAN_AFTER_MS + 1,
};

describe("focusRescanAllowed", () => {
  it("rescans when nothing runs and the last scan is a while ago", () => {
    expect(focusRescanAllowed(idle)).toBe(true);
  });

  it("holds off while the mods folder is being switched", () => {
    expect(focusRescanAllowed({ ...idle, switchingFolder: true })).toBe(false);
  });

  it("holds off for a scan, a write, a running move, a diagnosis, no folder, or a recent scan", () => {
    for (const k of ["scanning", "writeBlocked", "moveRunning", "bisecting"] as const)
      expect(focusRescanAllowed({ ...idle, [k]: true })).toBe(false);
    expect(focusRescanAllowed({ ...idle, hasRoots: false })).toBe(false);
    expect(focusRescanAllowed({ ...idle, sinceLastScanMs: 1000 })).toBe(false);
  });
});

describe("waitWhile", () => {
  it("resolves once the condition clears", async () => {
    let n = 0;
    await waitWhile(() => ++n < 3, 1);
    expect(n).toBe(3);
  });
});
