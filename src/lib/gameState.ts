// The game install and the chosen exe (#461), kept in step with the backend. Every read or change
// runs after the one before it ends, and each answer carries the install and the choice together
// (read at one moment by the backend), so the app never shows one from before a change beside the
// other from after it.
import type { GameState } from "./api";

/**
 * An updater for the game state: `update(get)` runs `get` (a detection, or a change of the chosen
 * game) once every earlier update is done, then hands its answer to `apply`, saying whether the
 * install folder changed from `currentDir()`'s.
 * @param apply shows the answer
 * @param currentDir the install folder shown now, or null
 * @returns update: resolves once its answer is applied, rejects with `get`'s error (and applies
 *   nothing); a failed update never holds up the next
 */
export function gameUpdater(
  apply: (state: GameState, installChanged: boolean) => void,
  currentDir: () => string | null,
): (get: () => Promise<GameState>) => Promise<void> {
  let queue: Promise<unknown> = Promise.resolve();
  return (get) => {
    const run = queue.then(async () => {
      const before = currentDir();
      const state = await get();
      apply(state, (state.install?.installDir ?? null) !== before);
    });
    queue = run.catch(() => {});
    return run;
  };
}
