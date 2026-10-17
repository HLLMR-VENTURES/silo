/** The active set to restore when guided bisection ends: the snapshot saved when it began,
 *  or (only if that's missing) the mods currently in play.
 *  @param snapshot the set saved by `bisectSnapshotSave` at start, if any
 *  @param alwaysOn mods kept on every round (maps)
 *  @param pool the bisection suspects as the caller currently sees them
 *  @returns the tech names to make active, without duplicates */
export function bisectRestoreSet(
  snapshot: string[] | null | undefined,
  alwaysOn: string[],
  pool: string[],
): string[] {
  const source = snapshot && snapshot.length > 0 ? snapshot : [...alwaysOn, ...pool];
  return [...new Set(source)];
}

/** Shown when a new bisection would overwrite an interrupted run's snapshot. */
export const BISECT_RESTORE_FIRST =
  "Finish restoring your last bisection first: use Restore in the banner above.";

/** A new bisection may start only when no earlier run's snapshot is waiting to be
 *  restored; starting would overwrite the only record of the user's real set.
 *  @param snapshot the stored snapshot, if any
 *  @returns true when it's safe to save a fresh snapshot */
export function canStartBisect(snapshot: string[] | null | undefined): boolean {
  return !snapshot || snapshot.length === 0;
}

/** The I/O `finishBisect` needs, injected so the ordering is unit-testable. */
export interface BisectFinishDeps {
  get: () => Promise<string[] | null>;
  apply: (techNames: string[]) => Promise<void>;
  clear: () => Promise<void>;
}

export type BisectFinishResult = { ok: true } | { ok: false; error: string };

/** Restore the saved set and only then clear the snapshot. Any failure leaves the
 *  snapshot in place so the restore can be offered again.
 *  @param deps snapshot read, set apply, snapshot clear
 *  @param alwaysOn mods kept on every round (fallback when no snapshot)
 *  @param pool current suspects (fallback when no snapshot)
 *  @returns ok, or the error message */
export async function finishBisect(
  deps: BisectFinishDeps,
  alwaysOn: string[],
  pool: string[],
): Promise<BisectFinishResult> {
  try {
    const snapshot = await deps.get();
    await deps.apply(bisectRestoreSet(snapshot, alwaysOn, pool));
    await deps.clear();
    return { ok: true };
  } catch (e) {
    return { ok: false, error: String(e) };
  }
}
