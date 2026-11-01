// What a switch of the active mods reports (#347): a failed switch that changed nothing is not
// recorded (no snapshot, no "switched", no Undo); one that changed some mods is, with problems.
import { describe, expect, it } from "vitest";
import { setSwitchOutcome, switchOutcome } from "./switchOutcome";

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

describe("setSwitchOutcome", () => {
  it("is clean when the apply was clean and every mod left out could be parked", () => {
    expect(setSwitchOutcome(true, true, ["A", "F"], ["B"])).toBe(true);
  });

  it("is nothing changed when the only change asked for was parking a folder mod (#347)", () => {
    // Active {A, F}, the target {A}: F can't be parked, so the apply gets {A, F} and is clean.
    expect(setSwitchOutcome(true, false, ["A", "F"], new Set(["F", "A"]))).toBeNull();
  });

  it("is with problems when it stopped short but some mods changed", () => {
    // Active {A, F}, the target {B}: B turned on, A parked, F stays on.
    expect(setSwitchOutcome(true, false, ["A", "F"], ["B", "F"])).toBe(false);
    // The apply finished with problems after changing some mods.
    expect(setSwitchOutcome(false, true, ["A"], ["B"])).toBe(false);
  });

  it("is with problems when the apply was refused or failed after filing changed the mods", () => {
    // A loose mod L was filed but couldn't be linked back, so it's parked now.
    expect(setSwitchOutcome(null, true, ["A", "L"], ["A"])).toBe(false);
  });

  it("is nothing changed when the apply was refused or failed and the mods are as they were", () => {
    expect(setSwitchOutcome(null, true, ["A"], ["A"])).toBeNull();
    expect(setSwitchOutcome(false, true, ["A"], ["A"])).toBeNull();
  });
});
