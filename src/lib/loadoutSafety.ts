// Pure helpers behind the loadout save / switch / overwrite confirmations in App.svelte.
// Loadouts and the active set are both lists of mod tech names.

/** What `askText`'s validate may return for a loadout name. */
export type LoadoutNameCheck = null | { error: string } | { warn: string; confirm: string };

/**
 * Validate a name for "Save current mods as a loadout".
 * @param name the typed name (trimmed by the caller or here)
 * @param existing names of the loadouts that already exist
 * @return null when fine, {error} for an empty name, {warn, confirm: "Replace"} when a loadout
 *   with the same name (ignoring case) exists
 */
export function loadoutNameCheck(name: string, existing: readonly string[]): LoadoutNameCheck {
  const n = name.trim();
  if (!n) return { error: "Give the loadout a name" };
  const key = n.toLocaleLowerCase();
  const hit = existing.find((e) => e.trim().toLocaleLowerCase() === key);
  if (hit === undefined) return null;
  return { warn: `You already have a loadout called ${hit}. Replace it?`, confirm: "Replace" };
}

/**
 * The existing loadout a name refers to (same case-insensitive match as `loadoutNameCheck`).
 * @param name the typed name
 * @param loadouts the existing loadouts
 * @return the match, or undefined
 */
export function findLoadoutByName<T extends { name: string }>(
  name: string,
  loadouts: readonly T[],
): T | undefined {
  const key = name.trim().toLocaleLowerCase();
  return loadouts.find((l) => l.name.trim().toLocaleLowerCase() === key);
}

export interface ActiveSetDiff {
  /** In the target, not active now, and (when a library is given) in the library. */
  on: string[];
  /** Active now and not in the target. */
  off: string[];
  /** In the target but not in the library (only filled when a library is given). */
  missing: string[];
}

/**
 * What switching the active set to `target` changes.
 * @param active tech names active now
 * @param target tech names the new set holds
 * @param library tech names in the library; when given, target mods outside it go to `missing`
 *   instead of `on` (they can't be turned on)
 * @return on / off / missing, each in input order without duplicates
 */
export function activeSetDiff(
  active: readonly string[],
  target: readonly string[],
  library?: ReadonlySet<string>,
): ActiveSetDiff {
  const now = new Set(active);
  const want = new Set(target);
  const on: string[] = [];
  const missing: string[] = [];
  for (const t of want) {
    if (library && !library.has(t)) missing.push(t);
    else if (!now.has(t)) on.push(t);
  }
  const off = [...now].filter((t) => !want.has(t));
  return { on, off, missing };
}

/**
 * True when two mod lists hold the same tech names, ignoring order and duplicates. The Undo
 * after a switch or an overwrite uses it to check nothing changed in between.
 * @param a one list (or set) of tech names
 * @param b the other
 * @return whether they name the same mods
 */
export function sameMods(a: Iterable<string>, b: Iterable<string>): boolean {
  const x = new Set(a);
  const y = new Set(b);
  if (x.size !== y.size) return false;
  for (const t of x) if (!y.has(t)) return false;
  return true;
}
