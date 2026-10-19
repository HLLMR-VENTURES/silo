// The mod the drawer shows, as the library has it now. The drawer is opened with one scan entry,
// but a rescan, a category change (the override layer builds new entries) or parking a loose mod
// (filing moves it, so its path changes) replace that entry; showing the old one leaves the
// drawer on a stale category, organized state and path.
//   let prev: ModEntry | null = null;
//   const drawerMod = $derived.by(() => (prev = currentEntry(detailMod, effectiveMods, prev)));
import type { ModEntry } from "../../types";

/**
 * Look the shown mod up again in the current list: by path, then (filing moved it) by tech name.
 * @param shown the entry the drawer was opened with, or null when it is closed
 * @param list the library's entries now
 * @param prev what this returned last time: kept when the new entry is equal to it, so a rescan
 *   that changed nothing about the mod doesn't hand the drawer a new object (its parts reset
 *   their state when the mod changes)
 * @returns the current entry, or null when the drawer is closed or the library no longer has the mod
 */
export function currentEntry<T extends Pick<ModEntry, "path" | "techName">>(
  shown: T | null,
  list: readonly T[],
  prev: T | null = null,
): T | null {
  if (!shown) return null;
  const now = list.find((m) => m.path === shown.path) ?? list.find((m) => m.techName === shown.techName) ?? null;
  if (now && prev && now !== prev && JSON.stringify(now) === JSON.stringify(prev)) return prev;
  return now;
}
