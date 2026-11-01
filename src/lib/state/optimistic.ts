// Optimistic edits to a keyed map (#358). The UI shows a change before its save returns; when the
// save fails, the change is put back, but only when the entry still holds the value this edit set.
// A newer edit made while the save was out is kept (it has its own save, and its own failure path).
//   const prev = map[key];
//   map = { ...map, [key]: next };
//   try { await save(next); } catch (e) { map = revertEntry(map, key, next, prev) ?? map; throw e; }
// Entries are compared by identity, so the map must hold plain objects ($state.raw, not $state).

/**
 * Undo one optimistic edit after its save failed.
 * @param map the map as it is now
 * @param key the entry the edit touched
 * @param mine what the edit set (`undefined`: the edit removed the entry)
 * @param prev what the entry held before the edit (`undefined`: it had none)
 * @returns the map with `prev` put back, or null when a newer edit has replaced `mine` (leave it)
 */
export function revertEntry<T>(
  map: Readonly<Record<string, T>>,
  key: string,
  mine: T | undefined,
  prev: T | undefined,
): Record<string, T> | null {
  const now = Object.prototype.hasOwnProperty.call(map, key) ? map[key] : undefined;
  if (now !== mine) return null;
  const next = { ...map };
  if (prev === undefined) delete next[key];
  else next[key] = prev;
  return next;
}
