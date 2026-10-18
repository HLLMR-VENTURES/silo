// Keep only the newest answer of an async check that can overlap itself. The conflict check
// runs on every change of the active set; a slow check of an older set must never land after,
// and overwrite, the check of the set that's active now (#260: the status chip said "All good"
// after a mod set switch turned on conflicting mods).
//   const conflictsLatest = latestOnly<Conflict[]>();
//   await conflictsLatest.run(() => detectConflicts(input), (c) => (conflicts = c));

export interface LatestOnly<T> {
  /**
   * Run `work`; call `apply` with its result only if no newer run started meanwhile.
   * @return true when the result was applied
   */
  run: (work: () => Promise<T>, apply: (value: T) => void) => Promise<boolean>;
  /** Drop any run in flight (its result won't apply). */
  cancel: () => void;
}

export function latestOnly<T>(): LatestOnly<T> {
  let generation = 0;
  return {
    async run(work, apply) {
      const mine = ++generation;
      const value = await work();
      if (mine !== generation) return false;
      apply(value);
      return true;
    },
    cancel() {
      generation++;
    },
  };
}
