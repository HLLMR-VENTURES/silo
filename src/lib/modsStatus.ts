import type { ModsRootStatus } from "./api";

/**
 * The folders to scan for this status: the live mods folder, unless it's unavailable or
 * blocked (it shares files with another layout, so Silo neither scans nor writes it).
 */
export function rootsFor(st: ModsRootStatus): string[] {
  return st.available && st.path && !st.layoutProblem ? [st.path] : [];
}

/**
 * Whether to say, once, that Silo now follows FS25's own folder — not while that folder is
 * blocked: its banner explains what's wrong instead.
 */
export function showOverrideNotice(st: ModsRootStatus, alreadyShown: boolean): boolean {
  return st.source === "gameOverride" && !alreadyShown && !st.layoutProblem;
}

/** What switching mods folders has to wait for. */
export interface SwitchState {
  moveUnfinished: boolean;
  scanning: boolean;
  busy: string | null;
  /** The current folder is blocked (`layoutProblem`): Silo changes nothing there. */
  blocked: boolean;
}

/**
 * Why switching folders isn't safe right now, or null. An interrupted guided bisection keeps
 * its folder until it's restored or discarded (`hasBisectSnapshot`) — except when the current
 * folder is blocked: nothing can be restored there, and switching away is the way out, so the
 * snapshot is kept and not asked about.
 */
export async function switchBlockReason(
  s: SwitchState,
  hasBisectSnapshot: () => Promise<boolean>,
): Promise<string | null> {
  if (s.moveUnfinished) return "Finish or cancel the archive move first.";
  if (s.scanning) return "Wait for the scan to finish.";
  if (s.busy) return `Wait for “${s.busy}” to finish.`;
  if (s.blocked) return null;
  if (await hasBisectSnapshot())
    return "Finish or cancel the guided bisection first — it belongs to this folder.";
  return null;
}
