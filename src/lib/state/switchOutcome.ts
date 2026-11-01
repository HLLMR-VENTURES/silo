// What a switch of the active mods tells its caller (#347), once set_active is done and, after a
// problem, the rescan that reads the mods folder again has landed. Callers record a switch, save
// its "Before …" snapshot and offer Undo only when something changed.
//   const outcome = switchOutcome(errors, before, activeSet, !scanFailed);
//   if (outcome === null) return null; // nothing changed: no snapshot, no "switched", no Undo
import { sameMods } from "../loadoutSafety";

/**
 * @param errors how many problems set_active reported, or null when it threw
 * @param before the active mods before the switch
 * @param after the active mods now (what the rescan found, after a problem)
 * @param rescanned the rescan after a problem read the mods folder (false: it failed)
 * @returns true when it applied cleanly; false when it finished with problems and the active mods
 *   changed (or the rescan couldn't tell); null when nothing changed: the rescan found the mods
 *   from before, or set_active threw and the rescan couldn't check (a throw is taken as nothing
 *   applied, the active mods were put back)
 */
export function switchOutcome(
  errors: number | null,
  before: Iterable<string>,
  after: Iterable<string>,
  rescanned: boolean,
): boolean | null {
  if (errors === 0) return true;
  if (rescanned) return sameMods(before, after) ? null : false;
  return errors === null ? null : false;
}

/**
 * What a whole-set switch tells its caller, once filing loose mods (if any) and the apply are
 * done. It can finish short of the target: the apply was refused or failed, or mods it can't park
 * (unzipped folder mods, a zip it couldn't file) stay on. Short and with the active mods as they
 * were, nothing happened: the caller mustn't record it (no snapshot, no "switched", no Undo).
 * @param applied what setActiveSetTo returned (switchOutcome, or null when refused)
 * @param complete every mod the target leaves out could be parked
 * @param baseline the active mods before any filing
 * @param after the active mods now
 * @returns true when it reached the target cleanly; false when it stopped short but the active
 *   mods changed (a mod filing couldn't link back, a partial apply); null when nothing changed
 */
export function setSwitchOutcome(
  applied: boolean | null,
  complete: boolean,
  baseline: Iterable<string>,
  after: Iterable<string>,
): boolean | null {
  if (applied === true && complete) return true;
  return sameMods(baseline, after) ? null : false;
}
