// In-app replacements for the OS confirm/prompt dialogs, rendered by <DialogHost /> (mounted once
// in App.svelte). Requests queue FIFO and show one at a time.
//   if (await ask({ title: "Delete mod set?", body: "...", confirm: "Delete", tone: "danger" })) ...
//   const name = await askText({ title: "Rename", label: "Name", value: old, confirm: "Rename" });
// Deliberately not named `confirm`, so it never shadows the global (see scripts/check-confirm.mjs).
// Before the host mounts, requests wait in the queue; check `dialogHostMounted()` to fall back to
// the OS dialog (`confirmDialog` in api.ts) in code that can run that early.
import type { Snippet } from "svelte";

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
  | { id: number; kind: "confirm"; opts: ConfirmOptions; resolve: (ok: boolean) => void }
  | { id: number; kind: "prompt"; opts: PromptOptions; resolve: (value: string | null) => void };

let nextId = 1;
let queue = $state.raw<DialogRequest[]>([]);
let hosts = 0;

/** The request on screen (head of the queue), or undefined. Reactive. */
export function currentDialog(): DialogRequest | undefined {
  return queue[0];
}

/** True once a <DialogHost /> is mounted, i.e. `ask()` / `askText()` will actually show something. */
export function dialogHostMounted(): boolean {
  return hosts > 0;
}

/**
 * Ask the user to confirm an action.
 * @param opts title, body, facts, list, verb label and tone
 * @returns true only when the confirm button is pressed; Esc, scrim, Cancel or host unmount give false
 */
export function ask(opts: ConfirmOptions): Promise<boolean> {
  return new Promise((resolve) => {
    queue = [...queue, { id: nextId++, kind: "confirm", opts, resolve }];
  });
}

/** Alias of `ask`. */
export const confirmAction = ask;

/**
 * Ask the user for a line of text.
 * @param opts title, field label, initial value, verb label, optional validation
 * @returns the trimmed value, or null when cancelled
 */
export function askText(opts: PromptOptions): Promise<string | null> {
  return new Promise((resolve) => {
    queue = [...queue, { id: nextId++, kind: "prompt", opts, resolve }];
  });
}

/**
 * Resolve a request and drop it from the queue. Safe to call twice; the first answer wins.
 * @param req the request being answered
 * @param value boolean for confirm requests, string | null for prompts
 */
export function settleDialog(req: DialogRequest, value: boolean | string | null): void {
  if (!queue.includes(req)) return;
  queue = queue.filter((r) => r !== req);
  if (req.kind === "confirm") req.resolve(value === true);
  else req.resolve(typeof value === "string" ? value : null);
}

/**
 * Register a mounted host (DialogHost calls this).
 * @returns unregister; when the last host goes away every pending request resolves false / null
 */
export function registerDialogHost(): () => void {
  hosts++;
  return () => {
    hosts--;
    if (hosts > 0) return;
    const pending = queue;
    queue = [];
    for (const req of pending) {
      if (req.kind === "confirm") req.resolve(false);
      else req.resolve(null);
    }
  };
}
