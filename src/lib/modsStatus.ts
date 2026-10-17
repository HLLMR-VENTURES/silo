import type { ModsRootStatus } from "./api";

/**
 * The folders to scan for this status: the live mods folder, unless it's unavailable or
 * blocked (it shares files with another layout, so Silo neither scans nor writes it).
 */
export function rootsFor(st: ModsRootStatus): string[] {
  return st.available && st.path && !st.layoutProblem ? [st.path] : [];
}
