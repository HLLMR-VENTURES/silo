// Confirmations that may run before the app shell (and its <DialogHost />) is mounted.
// Views call `ask()` from `$lib/ui` directly; this helper is for stores and startup code.
import { confirmDialog } from "./api";
import { ask, dialogHostMounted, type ConfirmOptions } from "./ui/dialogs.svelte";

/**
 * The OS-dialog text for a confirmation: title, body, facts and list as plain lines.
 * A Snippet body can't be shown by the OS dialog and is left out.
 * @param opts the in-app confirm options
 * @return the message for `confirmDialog`
 */
export function osConfirmText(opts: ConfirmOptions): string {
  const parts: string[] = [opts.title];
  if (typeof opts.body === "string" && opts.body) parts.push(opts.body);
  if (opts.facts?.length) parts.push(opts.facts.map((f) => `${f.label}: ${f.value}`).join("\n"));
  if (opts.list?.length) parts.push(opts.list.join("\n"));
  return parts.join("\n\n");
}

/**
 * Ask in-app when the dialog host is mounted, else fall back to the OS dialog with the same text.
 * ALWAYS `await` this (scripts/check-confirm.mjs enforces it).
 * @param opts title, body, facts, list, verb label and tone
 * @return true only when the confirm (verb) button is pressed
 */
export async function askOrOs(opts: ConfirmOptions): Promise<boolean> {
  if (dialogHostMounted()) return await ask(opts);
  return await confirmDialog(osConfirmText(opts), {
    okLabel: opts.confirm,
    cancelLabel: opts.cancel ?? "Cancel",
    kind: opts.tone === "danger" ? "warning" : "info",
  });
}
