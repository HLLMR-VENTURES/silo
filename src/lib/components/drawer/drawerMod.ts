// The mod the drawer shows, as the library has it now. The drawer is opened with one scan entry,
// but a rescan, a category change (the override layer builds new entries) or parking a loose mod
// (filing moves it, so its path changes) replace that entry; showing the old one leaves the
// drawer on a stale category, organized state and path.
//   const drawerMod = $derived(currentEntry(detailMod, effectiveMods));
import type { ModEntry } from "../../types";

/**
 * Look the shown mod up again in the current list: by path, then (filing moved it) by tech name.
 * @param shown the entry the drawer was opened with, or null when it is closed
 * @param list the library's entries now
 * @returns the current entry, or `shown` itself when the list no longer has the mod
 */
export function currentEntry<T extends Pick<ModEntry, "path" | "techName">>(
  shown: T | null,
  list: readonly T[],
): T | null {
  if (!shown) return null;
  return list.find((m) => m.path === shown.path) ?? list.find((m) => m.techName === shown.techName) ?? shown;
}
