// The Library's "Mod sets…" dialog (#323), pure: whether the chosen mods are in each saved set
// (ticked: all of them, unticked: none, mixed: some), how a click cycles that, and what Save
// writes. Mixed is only ever a starting state: a click goes mixed → ticked → unticked → mixed.
//   const rows = membershipRows(sets, ["FS25_A", "FS25_B"]);   // saved sets only, with their state
//   next(rows[0].initial, rows[0].initial)                   // "some" → "all"
//   const edits = membershipEdits(sets, techNames, chosen);  // only the sets that change
//   activeChange(edits, currentId, active)                   // what the set you're on turns on / parks
import type { Loadout } from "../types";
import { formatCount, plural } from "../plural";

/** A set holds every chosen mod ("all"), none of them, or some. */
export type Membership = "all" | "none" | "some";

/** One set's row in the dialog. */
export interface MembershipRow {
  set: Loadout;
  /** The state it opened with. */
  initial: Membership;
}

/** One set's change on Save. */
export interface MembershipEdit {
  set: Loadout;
  /** The set's mod list after Save. */
  mods: string[];
  /** Chosen mods Save adds (they weren't in the set). */
  added: string[];
  /** Chosen mods Save removes (they were in the set). */
  removed: string[];
}

/**
 * Whether a set holds the chosen mods.
 * @param setMods the set's tech names
 * @param techNames the chosen mods (at least one)
 */
export function membershipOf(setMods: readonly string[], techNames: readonly string[]): Membership {
  const has = new Set(setMods);
  const n = techNames.filter((t) => has.has(t)).length;
  if (n === 0) return "none";
  return n === techNames.length ? "all" : "some";
}

/**
 * The state after a click: ticked ↔ unticked, and back to mixed only for a set that opened mixed.
 * @param cur the state now
 * @param initial the state the row opened with
 */
export function next(cur: Membership, initial: Membership): Membership {
  if (cur === "some") return "all";
  if (cur === "all") return "none";
  return initial === "some" ? "some" : "all";
}

/**
 * The dialog's rows: the sets the user saved (automatic "Before …" snapshots are left out), in
 * their order, each with its starting state.
 * @param sets every saved set
 * @param techNames the chosen mods
 */
export function membershipRows(sets: readonly Loadout[], techNames: readonly string[]): MembershipRow[] {
  return sets.filter((s) => s.kind === "user").map((set) => ({ set, initial: membershipOf(set.mods, techNames) }));
}

/**
 * What Save writes: one edit per set whose state changed (a set left mixed, or put back as it
 * opened, isn't touched). A set's other mods keep their order; added mods go on the end.
 * @param rows the dialog's rows
 * @param techNames the chosen mods
 * @param chosen each set's state now, by set id (a missing id means unchanged)
 */
export function membershipEdits(
  rows: readonly MembershipRow[],
  techNames: readonly string[],
  chosen: ReadonlyMap<number, Membership>,
): MembershipEdit[] {
  const edits: MembershipEdit[] = [];
  for (const { set, initial } of rows) {
    const to = chosen.get(set.id) ?? initial;
    if (to === initial || to === "some") continue;
    const has = new Set(set.mods);
    if (to === "all") {
      const added = techNames.filter((t) => !has.has(t));
      edits.push({ set, mods: [...set.mods, ...added], added, removed: [] });
    } else {
      const drop = new Set(techNames);
      const removed = techNames.filter((t) => has.has(t));
      edits.push({ set, mods: set.mods.filter((t) => !drop.has(t)), added: [], removed });
    }
  }
  return edits;
}

/**
 * What editing the set you're on also does to your active mods, so you stay on it: the mods it
 * gains that are parked turn on, the mods it loses that are on are parked.
 * @param edits what Save writes
 * @param currentId the set you're on (ActiveSetState's id), or null
 * @param active tech names active now
 */
export function activeChange(
  edits: readonly MembershipEdit[],
  currentId: number | null,
  active: ReadonlySet<string>,
): { on: string[]; off: string[] } {
  const e = currentId === null ? undefined : edits.find((x) => x.set.id === currentId);
  if (!e) return { on: [], off: [] };
  return { on: e.added.filter((t) => !active.has(t)), off: e.removed.filter((t) => active.has(t)) };
}

/**
 * The note beside the set you're on once it's edited, or "" when your active mods don't change.
 * @param to the row's state now
 * @param initial the state it opened with
 * @param techNames the chosen mods
 * @param active tech names active now
 */
export function currentSetNote(
  to: Membership,
  initial: Membership,
  techNames: readonly string[],
  active: ReadonlySet<string>,
): string {
  if (to === initial || to === "some") return "";
  const them = techNames.length === 1 ? "it" : "them";
  if (to === "all") return techNames.some((t) => !active.has(t)) ? `also turns ${them} on now` : "";
  return techNames.some((t) => active.has(t)) ? `also parks ${them} now` : "";
}

/**
 * The success toast's message after Save.
 * @param names the chosen mods' titles
 * @param edits what Save wrote (at least one)
 * @returns "Added Courseplay to Riverbend." / "Added 3 mods to 2 sets and removed them from Hutan."
 */
export function membershipMessage(names: readonly string[], edits: readonly MembershipEdit[]): string {
  const what = names.length === 1 ? names[0] : `${formatCount(names.length)} mods`;
  const them = names.length === 1 ? "it" : "them";
  const setsText = (es: readonly MembershipEdit[]) =>
    es.length === 1 ? es[0].set.name : `${formatCount(es.length)} ${plural(es.length, { one: "set", other: "sets" })}`;
  const adds = edits.filter((e) => e.added.length > 0);
  const removes = edits.filter((e) => e.removed.length > 0);
  if (adds.length && removes.length) return `Added ${what} to ${setsText(adds)} and removed ${them} from ${setsText(removes)}.`;
  if (adds.length) return `Added ${what} to ${setsText(adds)}.`;
  return `Removed ${what} from ${setsText(removes)}.`;
}
