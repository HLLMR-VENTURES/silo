// The Undo toast after the active set changed: a single switch, a bulk Turn on / Park, or a
// loadout switch. One at a time: a new switch dismisses the previous switch's toast. Undo puts the
// previous active set back through the caller's guarded path, but only when the active set is
// still what the switch left (otherwise it refuses with a warn toast).
//   const undo = createSwitchUndo({ toast, blocked: activeUndoBlocked, reapply: (s) => setActiveSetTo(s, "Switching back") });
//   undo.dismiss();                         // a new switch is starting
//   undo.offer({ message: switchMessage(["Courseplay"], false), previous, expected: [...activeSet] });
import type { ToastHandle, ToastOptions, ToastTone } from "../ui/toasts.svelte";

export interface SwitchUndoDeps {
  toast: (opts: ToastOptions) => ToastHandle;
  /**
   * Why Undo can't run now, or null.
   * @param expected the active set the switch left
   */
  blocked: (expected: string[]) => string | null;
  /** Put this active set back (a guarded write; it reports its own problems). */
  reapply: (previous: string[]) => Promise<unknown>;
}

export interface SwitchUndoOffer {
  message: string;
  tone?: ToastTone;
  /** The active set before the switch: what Undo puts back. */
  previous: string[];
  /** The active set the switch left: Undo refuses once it changed. */
  expected: string[];
}

export interface SwitchUndo {
  /** Show the Undo toast for a switch that just applied (dismissing the previous one). */
  offer(o: SwitchUndoOffer): ToastHandle;
  /** Dismiss the current switch toast, if any. */
  dismiss(): void;
}

/** How long the switch toast stays (ms, time paused doesn't count). */
export const SWITCH_UNDO_MS = 10_000;

/**
 * The Undo-after-switch toasts, one at a time.
 * @param deps the toast function, the staleness guard and the re-apply path
 */
export function createSwitchUndo(deps: SwitchUndoDeps): SwitchUndo {
  let current: ToastHandle | null = null;
  function dismiss() {
    current?.dismiss();
    current = null;
  }
  function offer(o: SwitchUndoOffer): ToastHandle {
    dismiss();
    const previous = [...o.previous];
    const expected = [...o.expected];
    const handle = deps.toast({
      message: o.message,
      tone: o.tone ?? "neutral",
      duration: SWITCH_UNDO_MS,
      action: {
        label: "Undo",
        run: async () => {
          if (current === handle) current = null;
          const why = deps.blocked(expected);
          if (why) {
            deps.toast({ message: why, tone: "warn" });
            return;
          }
          await deps.reapply(previous);
        },
      },
    });
    current = handle;
    return handle;
  }
  return { offer, dismiss };
}

/**
 * The switch toast's message.
 * @param names the titles of the mods that changed
 * @param on true when they were turned on, false when parked
 * @param clean false when the change finished with problems
 * @returns "Parked Courseplay." / "Turned on 12 mods." / "Parking 3 mods finished with problems."
 */
export function switchMessage(names: string[], on: boolean, clean = true): string {
  const what = names.length === 1 ? names[0] : `${names.length} mods`;
  if (!clean) return `${on ? "Turning on" : "Parking"} ${what} finished with problems.`;
  return `${on ? "Turned on" : "Parked"} ${what}.`;
}
