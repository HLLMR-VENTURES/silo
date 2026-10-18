// The Undo toast after a switch (#242): it appears with Undo, Undo re-applies the previous active
// set, a stale Undo (the active set changed since) refuses with a warn toast, and a new switch
// dismisses the previous switch's toast.
import { describe, expect, it, vi } from "vitest";
import { createSwitchUndo, SWITCH_UNDO_MS, switchMessage } from "./switchUndo";
import type { ToastOptions } from "../ui/toasts.svelte";

function setup(blocked: (expected: string[]) => string | null = () => null) {
  const shown: { opts: ToastOptions; dismiss: ReturnType<typeof vi.fn> }[] = [];
  const toast = vi.fn((opts: ToastOptions) => {
    const dismiss = vi.fn();
    shown.push({ opts, dismiss });
    return { dismiss };
  });
  const reapply = vi.fn(() => Promise.resolve(true));
  const undo = createSwitchUndo({ toast, blocked, reapply });
  return { undo, toast, reapply, shown };
}

describe("switch Undo", () => {
  it("shows the switch toast with Undo for 10 s, and Undo re-applies the previous set", async () => {
    const { undo, shown, reapply } = setup();
    undo.offer({ message: "Parked Courseplay.", previous: ["A", "Courseplay"], expected: ["A"] });
    expect(shown).toHaveLength(1);
    const t = shown[0].opts;
    expect(t.message).toBe("Parked Courseplay.");
    expect(t.action?.label).toBe("Undo");
    expect(t.duration).toBe(SWITCH_UNDO_MS);
    await t.action!.run();
    expect(reapply).toHaveBeenCalledWith(["A", "Courseplay"]);
  });

  it("refuses a stale Undo with a warn toast and changes nothing", async () => {
    const blocked = vi.fn(() => "Can't undo: your active mods changed since the switch.");
    const { undo, shown, reapply } = setup(blocked);
    undo.offer({ message: "Turned on 12 mods.", previous: ["A"], expected: ["A", "B"] });
    await shown[0].opts.action!.run();
    expect(blocked).toHaveBeenCalledWith(["A", "B"]);
    expect(reapply).not.toHaveBeenCalled();
    expect(shown[1].opts).toMatchObject({
      message: "Can't undo: your active mods changed since the switch.",
      tone: "warn",
    });
  });

  it("a new switch dismisses the previous switch toast", () => {
    const { undo, shown } = setup();
    undo.offer({ message: "Parked A.", previous: ["A"], expected: [] });
    undo.offer({ message: "Turned on B.", previous: [], expected: ["B"] });
    expect(shown[0].dismiss).toHaveBeenCalled();
    expect(shown[1].dismiss).not.toHaveBeenCalled();
    undo.dismiss();
    expect(shown[1].dismiss).toHaveBeenCalled();
  });

  it("keeps its own copies of the sets", async () => {
    const { undo, shown, reapply } = setup();
    const previous = ["A"];
    undo.offer({ message: "x", previous, expected: [] });
    previous.push("B");
    await shown[0].opts.action!.run();
    expect(reapply).toHaveBeenCalledWith(["A"]);
  });
});

describe("switchMessage", () => {
  it("names one mod, counts several, and says when it had problems", () => {
    expect(switchMessage(["Courseplay"], false)).toBe("Parked Courseplay.");
    expect(switchMessage(["Courseplay"], true)).toBe("Turned on Courseplay.");
    expect(switchMessage(Array.from({ length: 12 }, (_, i) => `M${i}`), true)).toBe("Turned on 12 mods.");
    expect(switchMessage(["A", "B", "C"], false, false)).toBe("Parking 3 mods finished with problems.");
  });
});
