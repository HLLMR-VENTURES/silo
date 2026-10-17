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
