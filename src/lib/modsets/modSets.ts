// Pure helpers for the Mod sets page: which set the active mods are on ("Harvest 2026" or
// "Custom (3 changes since Harvest 2026)"), the My sets table's comparisons, snapshot names, the
// remembered last-applied set, and the Savegames cards. Mod sets are `Loadout`s in the code and
// the database; the app only ever says "mod set".
//   const state = activeSetState(sets, activeSet, lastApplied, libraryTechNames);
//   activeSetLine(state)  // "Custom: 3 changes since Harvest 2026" (Play menu)
//   stripSetName(state)   // "Custom (3 changes since Harvest 2026)" (Library status strip)
import type { Loadout, Savegame } from "../types";
import { activeSetDiff, type ActiveSetDiff } from "../loadoutSafety";
import { formatCount, plural } from "../plural";

/** The set the user last switched to, remembered so one change doesn't forget it. */
export interface LastApplied {
  id: number;
  name: string;
  /** The automatic "Before …" snapshot that switch saved, if any. */
  snapshotId: number | null;
  /** The set the user was on before that switch (where Undo / Switch back returns). */
  before: { id: number; name: string } | null;
}

export type ActiveSetState =
  | { kind: "set"; id: number; name: string }
  | { kind: "custom"; id: number; name: string; changes: number }
  | { kind: "none" };

/**
 * How many mods differ between the active set and a set's mods (turned on plus parked). Mods the
 * set names that aren't in the library don't count: they can't be turned on.
 * @param active tech names active now
 * @param mods the set's tech names
 * @param library tech names in the library, or undefined before the first scan
 */
export function changesBetween(
  active: Iterable<string>,
  mods: readonly string[],
  library?: ReadonlySet<string>,
): number {
  const d = activeSetDiff([...active], mods, library);
  return d.on.length + d.off.length;
}

/**
 * Which set the active mods are on.
 * The last-applied set wins when it still matches; then any set the user saved that matches
 * exactly; then "Custom" relative to the last-applied set. When the active mods are back to the
 * snapshot the last switch saved (Undo, or Switch back), the set from before that switch counts.
 * @param sets every saved set
 * @param active tech names active now
 * @param last the remembered last-applied set, or null
 * @param library tech names in the library, or undefined before the first scan
 */
export function activeSetState(
  sets: readonly Loadout[],
  active: ReadonlySet<string>,
  last: LastApplied | null,
  library?: ReadonlySet<string>,
): ActiveSetState {
  const byId = (id: number) => sets.find((s) => s.id === id);
  const same = (s: Loadout) => changesBetween(active, s.mods, library) === 0;
  let base: { id: number; name: string } | null = last;
  if (last && last.snapshotId !== null) {
    const snap = byId(last.snapshotId);
    const target = byId(last.id);
    if (snap && same(snap) && !(target && same(target))) base = last.before;
  }
  const baseSet = base ? byId(base.id) : undefined;
  if (baseSet && same(baseSet)) return { kind: "set", id: baseSet.id, name: baseSet.name };
  const exact = sets.find((s) => s.kind === "user" && same(s));
  if (exact) return { kind: "set", id: exact.id, name: exact.name };
  if (baseSet)
    return {
      kind: "custom",
      id: baseSet.id,
      name: baseSet.name,
      changes: changesBetween(active, baseSet.mods, library),
    };
  return { kind: "none" };
}

/** "3 changes" / "1 change". */
function changesText(n: number): string {
  return `${formatCount(n)} ${plural(n, { one: "change", other: "changes" })}`;
}

/**
 * The Play menu's heading line for the active set.
 * @return "Harvest 2026, unchanged", "Custom: 3 changes since Harvest 2026" or "No mod set"
 */
export function activeSetLine(s: ActiveSetState): string {
  if (s.kind === "set") return `${s.name}, unchanged`;
  if (s.kind === "custom") return `Custom: ${changesText(s.changes)} since ${s.name}`;
  return "No mod set";
}

/**
 * The set's part of the Library status strip.
 * @return "Harvest 2026", "Custom (3 changes since Harvest 2026)", or null for none
 */
export function stripSetName(s: ActiveSetState): string | null {
  if (s.kind === "set") return s.name;
  if (s.kind === "custom") return `Custom (${changesText(s.changes)} since ${s.name})`;
  return null;
}

/**
 * What to remember after switching to `target`.
 * Switching to the snapshot the last switch saved goes back to the set from before it.
 * @param last what was remembered before the switch
 * @param target the set switched to
 * @param snapshotId the "Before …" snapshot this switch saved, or null
 * @param was the state before the switch (its set becomes `before`)
 */
export function afterSwitch(
  last: LastApplied | null,
  target: { id: number; name: string; kind: Loadout["kind"] },
  snapshotId: number | null,
  was: ActiveSetState,
): LastApplied | null {
  if (target.kind === "auto" && last && last.snapshotId === target.id) {
    return last.before ? { ...last.before, snapshotId: null, before: null } : null;
  }
  return {
    id: target.id,
    name: target.name,
    snapshotId,
    before: was.kind === "none" ? null : { id: was.id, name: was.name },
  };
}

/** A set the user just saved, or replaced with the active mods: the active mods match it. */
export function savedAs(set: { id: number; name: string }): LastApplied {
  return { id: set.id, name: set.name, snapshotId: null, before: null };
}

/**
 * The automatic snapshot's name for a switch to `target`.
 * @return "Before Forestry weekend", or "Before switching back" when the target is itself a snapshot
 */
export function snapshotNameFor(target: { name: string; kind: Loadout["kind"] }): string {
  return target.kind === "auto" ? "Before switching back" : `Before ${target.name}`;
}

/** Sets the user saved (by name, as loaded), then automatic snapshots, newest first. */
export function sortSets(sets: readonly Loadout[]): Loadout[] {
  const user = sets.filter((s) => s.kind !== "auto");
  const auto = sets
    .filter((s) => s.kind === "auto")
    .sort((a, b) => (b.createdAt ?? 0) - (a.createdAt ?? 0) || b.id - a.id);
  return [...user, ...auto];
}

/** One My sets row's "Compared with your active mods" cell. */
export interface SetComparison {
  /** "Same", or "+31 / −77" (turned on / parked). */
  text: string;
  /** Accessible wording: "31 turn on and 77 are parked". */
  label: string;
  same: boolean;
  diff: ActiveSetDiff;
}

/**
 * Compare a set with the active mods.
 * @param mods the set's tech names
 * @param active tech names active now
 * @param library tech names in the library, or undefined before the first scan
 */
export function compareSet(
  mods: readonly string[],
  active: ReadonlySet<string>,
  library?: ReadonlySet<string>,
): SetComparison {
  const diff = activeSetDiff([...active], mods, library);
  const on = diff.on.length;
  const off = diff.off.length;
  if (on + off === 0) return { text: "Same", label: "Same as your active mods", same: true, diff };
  return {
    text: `+${formatCount(on)} / −${formatCount(off)}`,
    label: `${formatCount(on)} ${on === 1 ? "turns" : "turn"} on and ${formatCount(off)} ${off === 1 ? "is" : "are"} parked`,
    same: false,
    diff,
  };
}

/** "Oct 17" for a snapshot's date (this year), "Oct 17, 2025" otherwise; "" when unknown. */
export function shortDate(ms: number | null, now = new Date()): string {
  if (ms === null) return "";
  const d = new Date(ms);
  const opts: Intl.DateTimeFormatOptions =
    d.getFullYear() === now.getFullYear()
      ? { month: "short", day: "numeric" }
      : { month: "short", day: "numeric", year: "numeric" };
  return new Intl.DateTimeFormat("en", opts).format(d);
}

/**
 * A savegame's last-played date as the game wrote it ("2026-08-02") → "Aug 2, 2026".
 * @return the formatted date, the raw text if it isn't a plain date, or "" when missing
 */
export function saveDateLabel(raw: string | null): string {
  if (!raw) return "";
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(raw.trim());
  if (!m) return raw.trim();
  const d = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
  return new Intl.DateTimeFormat("en", { month: "short", day: "numeric", year: "numeric" }).format(d);
}

/** A save's user mods (not DLC) split by whether the library has them. */
export interface SaveModStats {
  total: number;
  present: { modName: string; title: string | null }[];
  missing: { modName: string; title: string | null }[];
}

/**
 * Cross-reference a save's user mods (DLC left out) against the library.
 * @param s the savegame
 * @param library tech names in the library
 */
export function saveModStats(s: Savegame, library: ReadonlySet<string>): SaveModStats {
  const userMods = s.mods.filter((m) => !m.isDlc);
  return {
    total: userMods.length,
    present: userMods.filter((m) => library.has(m.modName)),
    missing: userMods.filter((m) => !library.has(m.modName)),
  };
}

/** "41 of 44 mods in your library", "All 12 mods in your library", "Uses no mods". */
export function saveModsLine(st: SaveModStats): string {
  if (st.total === 0) return "Uses no mods";
  const noun = plural(st.total, { one: "mod", other: "mods" });
  if (st.missing.length === 0)
    return st.total === 1 ? "Its 1 mod is in your library" : `All ${formatCount(st.total)} ${noun} in your library`;
  return `${formatCount(st.present.length)} of ${formatCount(st.total)} ${noun} in your library`;
}

/**
 * Whether a picked shared file is a Multiplayer check file (checked against your mods) or a
 * mod set (previewed, then saved and maybe downloaded).
 */
export function sharedFileKind(path: string): "multiplayer" | "set" {
  return /\.silomp$/i.test(path.trim()) ? "multiplayer" : "set";
}

/** The file name part of a path (either separator). */
export function baseName(path: string): string {
  return path.split(/[\\/]/).pop() ?? path;
}

const LAST_KEY = "silo.modSets.lastApplied";

function validLast(v: unknown): v is LastApplied {
  if (!v || typeof v !== "object") return false;
  const o = v as Record<string, unknown>;
  const ref = (x: unknown) =>
    !!x && typeof x === "object" && typeof (x as { id: unknown }).id === "number" &&
    typeof (x as { name: unknown }).name === "string";
  return (
    typeof o.id === "number" &&
    typeof o.name === "string" &&
    (o.snapshotId === null || typeof o.snapshotId === "number") &&
    (o.before === null || ref(o.before))
  );
}

/** The remembered last-applied set (a per-viewer pref), or null when none or unreadable. */
export function loadLastApplied(storage: Pick<Storage, "getItem"> | undefined = globalThis.localStorage): LastApplied | null {
  try {
    const raw = storage?.getItem(LAST_KEY);
    if (!raw) return null;
    const v: unknown = JSON.parse(raw);
    return validLast(v) ? v : null;
  } catch {
    return null;
  }
}

/** Remember (or forget, with null) the last-applied set. Best effort. */
export function storeLastApplied(
  v: LastApplied | null,
  storage: Pick<Storage, "setItem" | "removeItem"> | undefined = globalThis.localStorage,
): void {
  try {
    if (v) storage?.setItem(LAST_KEY, JSON.stringify(v));
    else storage?.removeItem(LAST_KEY);
  } catch {
    /* storage unavailable: it's only remembered for this run */
  }
}
