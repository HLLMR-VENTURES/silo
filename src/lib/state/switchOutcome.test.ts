// What a switch of the active mods reports (#347): a failed switch that changed nothing is not
// recorded (no snapshot, no "switched", no Undo); one that changed some mods is, with problems.
import { describe, expect, it } from "vitest";
import { switchOutcome } from "./switchOutcome";

describe("switchOutcome", () => {
  it("is clean when set_active reported no problems", () => {
    expect(switchOutcome(0, ["A"], ["B"], true)).toBe(true);
    // No rescan runs after a clean apply: its result doesn't matter.
    expect(switchOutcome(0, ["A"], ["B"], false)).toBe(true);
  });

  it("is nothing changed when set_active threw and the rescan found the old mods", () => {
    expect(switchOutcome(null, ["A", "B"], new Set(["B", "A"]), true)).toBeNull();
  });

  it("is with problems when set_active threw but the rescan found some mods changed", () => {
    expect(switchOutcome(null, ["A", "B"], ["A", "C"], true)).toBe(false);
  });

  it("is nothing changed when set_active threw and the rescan failed (the mods were put back)", () => {
    expect(switchOutcome(null, ["A"], ["A"], false)).toBeNull();
  });

  it("is nothing changed when set_active reported problems and the rescan found the old mods", () => {
    expect(switchOutcome(2, ["A"], ["A"], true)).toBeNull();
  });

  it("is with problems when set_active reported problems and the mods changed", () => {
    expect(switchOutcome(1, ["A"], ["B"], true)).toBe(false);
  });

  it("is with problems when set_active reported problems and the rescan couldn't check", () => {
    expect(switchOutcome(1, ["A"], ["B"], false)).toBe(false);
  });
});
