// Confirmations that may run before the app shell (and its <DialogHost />) is mounted.
// Views call `ask()` from `$lib/ui` directly; this helper is for stores and startup code.
import { ask, askOs, dialogHostMounted, type ConfirmOptions } from "./ui/dialogs.svelte";

export { osConfirmText } from "./ui/dialogs.svelte";

/**
 * Ask in-app when the dialog host is mounted, else fall back to the OS dialog with the same text.
 * ALWAYS `await` this (scripts/check-confirm.mjs enforces it).
 * @param opts title, body, facts, list, verb label and tone
 * @return true only when the confirm (verb) button is pressed
 */
export async function askOrOs(opts: ConfirmOptions): Promise<boolean> {
  if (dialogHostMounted()) return await ask(opts);
  return await askOs(opts);
}
