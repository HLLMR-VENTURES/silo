// What set_active can and can't park. It only moves mods filed in the library (an `organized`
// row): a loose zip in the mods folder is parked by filing it, and an unzipped folder mod can't
// be parked at all. Anything that sets a whole active set checks here first, so a mod is never
// "parked" on screen while the game still loads it.
//   const { loose, fixed } = parkPlan(mods, activeSet, target);
//   const stuck = stillOn(activeAfter, target, startSet);   // after a round: must be []
import { formatCount, plural } from "./plural";

/** The parts of a library mod this reads. */
export interface ParkableMod {
  techName: string;
  title?: string | null;
  organized: boolean;
  kind: string;
}

/** A loose zip: set_active can't park it; filing it into the library does. */
export const isLooseZip = (m: ParkableMod) => !m.organized && m.kind === "zip";
/** An unzipped folder mod Silo hasn't filed: nothing in Silo can park it. */
export const isUnparkable = (m: ParkableMod) => !m.organized && m.kind !== "zip";

/**
 * The active mods a new active set leaves out that set_active can't park by itself.
 * @param mods the library
 * @param active tech names active now
 * @param target the active set asked for
 * @returns `loose`: zips to file first; `fixed`: folder mods that stay on
 */
export function parkPlan<M extends ParkableMod>(
  mods: readonly M[],
  active: ReadonlySet<string>,
  target: Iterable<string>,
): { loose: M[]; fixed: M[] } {
  const want = new Set(target);
  const leaving = mods.filter((m) => active.has(m.techName) && !want.has(m.techName));
  return { loose: leaving.filter(isLooseZip), fixed: leaving.filter(isUnparkable) };
}

/**
 * After an active set was applied: the mods meant to be parked that are still active.
 * @param activeNow tech names active on disk now (after a rescan)
 * @param wanted the active set that was applied
 * @param among only these count (e.g. the mods a diagnosis started with; one added since isn't
 *   the round's business)
 */
export function stillOn(
  activeNow: Iterable<string>,
  wanted: Iterable<string>,
  among: ReadonlySet<string>,
): string[] {
  const want = new Set(wanted);
  return [...activeNow].filter((t) => among.has(t) && !want.has(t));
}

/** "Big Pack stays on" / "3 mods stay on", for a toast after a switch. */
export function fixedOnText(names: string[]): string {
  const who = names.length === 1 ? `${names[0]} stays` : `${formatCount(names.length)} ${plural(names.length, { one: "mod", other: "mods" })} stay`;
  return `${who} on: Silo can't park unzipped folder mods. Move ${names.length === 1 ? "its folder" : "their folders"} out of your mods folder to stop the game loading ${names.length === 1 ? "it" : "them"}.`;
}
