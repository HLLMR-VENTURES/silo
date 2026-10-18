// In-app replacements for the OS confirm/prompt dialogs, rendered by <DialogHost /> (mounted once
// in App.svelte). Requests queue FIFO and show one at a time.
//   if (await ask({ title: "Delete mod set?", body: "...", confirm: "Delete", tone: "danger" })) ...
//   const name = await askText({ title: "Rename", label: "Name", value: old, confirm: "Rename" });
//   const { confirmed, checked } = await askWithOption({ ..., checkbox: { label: "Don't ask again" } });
// Deliberately not named `confirm`, so it never shadows the global (see scripts/check-confirm.mjs).
// Before the host first mounts, requests wait in the queue; check `dialogHostMounted()` to fall
// back to the OS dialog (`askOrOs` in confirm.ts) in code that can run that early. If the host
// was mounted and then went away (its view boundary caught a failure), `ask()` uses the OS dialog
// and `askText()` gives null with an error toast, so nothing waits on a dialog that can't show.
import type { Snippet } from "svelte";
import { confirmDialog } from "../api";
import { toast } from "./toasts.svelte";

export interface ConfirmFact {
  label: string;
  value: string;
}

export interface ConfirmOptions {
  title: string;
  body?: string | Snippet;
  /** Key/value lines shown as a two-column table. */
  facts?: ConfirmFact[];
  /** Affected items (e.g. mod names); scrolls past ~8 rows. */
  list?: string[];
  /** Verb label for the confirm button ("Delete", "Move 12 mods"). Required: never "OK". */
  confirm: string;
  /** Default "Cancel". */
  cancel?: string;
  /** "danger": red confirm button and initial focus on Cancel. Default "neutral". */
  tone?: "neutral" | "danger";
  /** A checkbox under the body ("Don't explain this again"). Only `askWithOption()` reports it. */
  checkbox?: { label: string; checked?: boolean };
}

/** What `askWithOption()` resolves to: the answer, and the checkbox as it was when answered. */
export interface ConfirmWithOption {
  confirmed: boolean;
  checked: boolean;
}

/** What `validate` may return: null = ok; a string or {error} blocks; {warn, confirm} asks for a second submit. */
export type PromptValidation = string | null | { error: string } | { warn: string; confirm: string };

export interface PromptOptions {
  title: string;
  label: string;
  value?: string;
  placeholder?: string;
  confirm: string;
  cancel?: string;
  /** Called with the trimmed value on submit. */
  validate?: (value: string) => PromptValidation;
}

export type DialogRequest =
  | { id: number; kind: "confirm"; opts: ConfirmOptions; resolve: (ok: boolean, checked: boolean) => void }
  | { id: number; kind: "prompt"; opts: PromptOptions; resolve: (value: string | null) => void };

let nextId = 1;
let queue = $state.raw<DialogRequest[]>([]);
let hosts = 0;
/** A host has mounted at least once: no host now means it failed, not that startup is early. */
let everMounted = false;

/**
 * How long (ms) a confirm or prompt dialog ignores its verb button (click, Enter, Space) after it
 * opens, and a prompt after its button turns into a warning ("Replace"). A double-click or a held
 * key meant for the previous dialog then can't answer the next one in the queue. Cancel, Esc and
 * the scrim work at once.
 */
export const ARM_MS = 300;
let armDelay = ARM_MS;

/** The current arm delay in ms (ARM_MS unless a test changed it). */
export function dialogArmDelay(): number {
  return armDelay;
}

/** Tests only: forget that a host ever mounted (back to the startup state). */
export function resetDialogHostHistory(): void {
  everMounted = false;
}

/**
 * Tests only: change the arm delay (0 = the verb button works at once).
 * @param ms the delay; default ARM_MS
 */
export function setDialogArmDelay(ms: number = ARM_MS): void {
  armDelay = Math.max(0, ms);
}

/**
 * Arm a verb button after `dialogArmDelay()` ms. Used by ConfirmDialog and PromptDialog.
 * @returns `armed()` (true once the delay has passed), `rearm()` (start the delay again) and
 *   `stop()` (clear the timer, on close / unmount)
 */
export function armTimer(): { armed: () => boolean; rearm: () => void; stop: () => void } {
  let ready = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  function stop() {
    clearTimeout(timer);
    timer = undefined;
  }
  function rearm() {
    stop();
    const ms = armDelay;
    ready = ms <= 0;
    if (!ready) timer = setTimeout(() => (ready = true), ms);
  }
  return { armed: () => ready, rearm, stop };
}

/** The request on screen (head of the queue), or undefined. Reactive. */
export function currentDialog(): DialogRequest | undefined {
  return queue[0];
}

/** True once a <DialogHost /> is mounted, i.e. `ask()` / `askText()` will actually show something. */
export function dialogHostMounted(): boolean {
  return hosts > 0;
}

/** The host mounted once and is gone now (its view boundary caught a failure). */
function hostLost(): boolean {
  return everMounted && hosts === 0;
}

/**
 * The OS-dialog text for a confirmation: title, body, facts and list as plain lines.
 * A Snippet body can't be shown by the OS dialog and is left out.
 * @param opts the in-app confirm options
 * @returns the message for `confirmDialog`
 */
export function osConfirmText(opts: ConfirmOptions): string {
  const parts: string[] = [opts.title];
  if (typeof opts.body === "string" && opts.body) parts.push(opts.body);
  if (opts.facts?.length) parts.push(opts.facts.map((f) => `${f.label}: ${f.value}`).join("\n"));
  if (opts.list?.length) parts.push(opts.list.join("\n"));
  return parts.join("\n\n");
}

/**
 * Ask with the OS dialog, with the same text and labels as the in-app one.
 * @param opts the in-app confirm options
 * @returns true only when the confirm (verb) button is pressed; false if the dialog fails
 */
export async function askOs(opts: ConfirmOptions): Promise<boolean> {
  try {
    return await confirmDialog(osConfirmText(opts), {
      okLabel: opts.confirm,
      cancelLabel: opts.cancel ?? "Cancel",
      kind: opts.tone === "danger" ? "warning" : "info",
    });
  } catch (e) {
    console.error("[Silo] the OS confirm dialog failed", e);
    return false;
  }
}

/**
 * Ask the user to confirm an action.
 * @param opts title, body, facts, list, verb label and tone
 * @returns true only when the confirm button is pressed; Esc, scrim, Cancel or host unmount give
 *   false. After the host failed, the OS dialog asks instead.
 */
export async function ask(opts: ConfirmOptions): Promise<boolean> {
  if (hostLost()) return await askOs(opts);
  return await new Promise((resolve) => {
    queue = [...queue, { id: nextId++, kind: "confirm", opts, resolve }];
  });
}

/**
 * Ask the user to confirm an action, with a checkbox under the body (`opts.checkbox`).
 * @param opts as for `ask`, plus `checkbox: { label, checked? }`
 * @returns `confirmed` as `ask` would give it, and whether the box was ticked. After the host
 *   failed, the OS dialog asks instead (it has no checkbox, so `checked` is false).
 */
export async function askWithOption(opts: ConfirmOptions): Promise<ConfirmWithOption> {
  if (hostLost()) return { confirmed: await askOs(opts), checked: false };
  return await new Promise((resolve) => {
    queue = [
      ...queue,
      {
        id: nextId++,
        kind: "confirm",
        opts,
        resolve: (ok, checked) => resolve({ confirmed: ok, checked }),
      },
    ];
  });
}

/** Alias of `ask`. */
export const confirmAction = ask;

/**
 * Ask the user for a line of text.
 * @param opts title, field label, initial value, verb label, optional validation
 * @returns the trimmed value, or null when cancelled (or, with an error toast, when the host failed)
 */
export async function askText(opts: PromptOptions): Promise<string | null> {
  if (hostLost()) {
    toast({
      message: `Silo couldn't open the "${opts.title}" dialog.`,
      body: "Reload it from the problem card, then try again.",
      tone: "danger",
      sticky: true,
    });
    return null;
  }
  return await new Promise((resolve) => {
    queue = [...queue, { id: nextId++, kind: "prompt", opts, resolve }];
  });
}

/**
 * Resolve a request and drop it from the queue. Safe to call twice; the first answer wins.
 * @param req the request being answered
 * @param value boolean for confirm requests, string | null for prompts
 * @param checked a confirm's checkbox (`opts.checkbox`) when it was answered
 */
export function settleDialog(req: DialogRequest, value: boolean | string | null, checked = false): void {
  if (!queue.includes(req)) return;
  queue = queue.filter((r) => r !== req);
  if (req.kind === "confirm") req.resolve(value === true, checked);
  else req.resolve(typeof value === "string" ? value : null);
}

/**
 * Register a mounted host (DialogHost calls this).
 * @returns unregister; when the last host goes away every pending request resolves false / null
 */
export function registerDialogHost(): () => void {
  hosts++;
  everMounted = true;
  return () => {
    hosts--;
    if (hosts > 0) return;
    const pending = queue;
    queue = [];
    for (const req of pending) {
      if (req.kind === "confirm") req.resolve(false, false);
      else req.resolve(null);
    }
  };
}
