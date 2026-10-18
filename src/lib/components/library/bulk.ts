// The BulkBar's Turn on / Park: which of the selected mods change, and the confirmation a big
// change needs first (more than BULK_CONFIRM_OVER mods: each one is a link or a copy to make or
// remove, which takes a while on a cloud-synced folder). Pure, so the counts unit-test.
//   const c = bulkChanges(selectedMods, true, activeSet);   // the ones not on yet
//   if (!(await confirmBulkChange(c.length, true, cloudSynced, ask))) return; // asks over 25
import type { ConfirmOptions } from "../../ui/dialogs.svelte";
import { modsCount } from "../../archiveMoveLogic";

/** Bigger changes than this ask first. */
export const BULK_CONFIRM_OVER = 25;

/**
 * The mods a bulk Turn on / Park actually changes (the ones already right are left out).
 * @param mods the selected mods
 * @param on true for Turn on, false for Park
 * @param active the active set now
 */
export function bulkChanges<M extends { techName: string }>(
  mods: readonly M[],
  on: boolean,
  active: ReadonlySet<string>,
): M[] {
  return mods.filter((m) => (on ? !active.has(m.techName) : active.has(m.techName)));
}

/**
 * The confirmation for a big bulk change, or null when it's small enough to just do.
 * @param count how many mods change
 * @param on true for Turn on, false for Park
 * @param cloudSynced the mods folder is cloud-synced (each mod is a copy, not a link)
 */
export function bulkConfirm(count: number, on: boolean, cloudSynced: boolean): ConfirmOptions | null {
  if (count <= BULK_CONFIRM_OVER) return null;
  const verb = on ? "Turn on" : "Park";
  const tail = cloudSynced
    ? " Your mods folder is cloud-synced, so each one is copied or removed on its own. This can take a while."
    : "";
  return {
    title: `${verb} ${modsCount(count)}?`,
    body:
      `Each one is ${on ? "linked into" : "removed from"} your mods folder; nothing is deleted from your library.${tail} ` +
      "You can undo it right after.",
    facts: [{ label: on ? "Turn on" : "Park", value: modsCount(count) }],
    confirm: `${verb} ${modsCount(count)}`,
  };
}

/**
 * Ask before a big bulk change; a small one goes ahead without asking.
 * @param count how many mods change
 * @param on true for Turn on, false for Park
 * @param cloudSynced the mods folder is cloud-synced
 * @param ask the dialog (ui's ask)
 * @returns true to go ahead
 */
export async function confirmBulkChange(
  count: number,
  on: boolean,
  cloudSynced: boolean,
  ask: (opts: ConfirmOptions) => Promise<boolean>,
): Promise<boolean> {
  const opts = bulkConfirm(count, on, cloudSynced);
  if (!opts) return true;
  return await ask(opts);
}
