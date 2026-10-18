// App-wide keyboard shortcuts: which key press means which action. Pure (no DOM state) apart from
// isTypingTarget and the listener installer, so the keymap is unit-tested on its own.
//   resolveShortcut(e, { typing: isTypingTarget(e.target), modalOpen }) // -> { action, preventDefault }
//   const stop = installShortcuts(window, run, () => get(modalOpen))    // one keydown listener
// Rules: nothing fires while an IME is composing or a modal dialog is open; while typing in a field
// only Ctrl+K, Ctrl+, , F6 and the reload keys work. The reload keys (F5, Ctrl+R, Ctrl+Shift+R)
// are always swallowed so the webview never reloads mid-operation; they rescan instead (unless a
// dialog is open). Bare letters and numbers are never taken: views use Alt+1…Alt+5.

import type { ViewId } from "./nav.svelte";

export type ShortcutAction =
  | { kind: "focusSearch" }
  | { kind: "rescan" }
  | { kind: "settings" }
  | { kind: "view"; view: ViewId }
  | { kind: "back" }
  | { kind: "region"; dir: 1 | -1 }
  | { kind: "help" };

/** The parts of a KeyboardEvent the keymap reads. */
export interface KeyInput {
  key: string;
  code?: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  shiftKey?: boolean;
  isComposing?: boolean;
  repeat?: boolean;
  /** Something nearer the target already handled the key (a menu's typeahead, a field's Esc). */
  defaultPrevented?: boolean;
}

export interface ShortcutContext {
  /** Focus is in a text field, textarea, select or contenteditable. */
  typing: boolean;
  /** A modal dialog is open (overlayStack's modalOpen). */
  modalOpen: boolean;
}

export interface ShortcutResolution {
  action: ShortcutAction | null;
  /** Swallow the key (even when no action runs, e.g. a reload key behind a dialog). */
  preventDefault: boolean;
}

/** Alt+1 … Alt+5, in rail order. */
export const VIEW_SHORTCUTS: readonly ViewId[] = ["library", "browse", "modSets", "diagnose", "updates"];

const NONE: ShortcutResolution = { action: null, preventDefault: false };

/** Input types that take text (Space, letters, "/" and "?" belong to them). */
const NON_TEXT_INPUTS = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/**
 * Is `target` somewhere the user types (so bare keys must reach it)?
 * @param target the key event's target
 */
export function isTypingTarget(target: EventTarget | null): boolean {
  if (!target || typeof (target as Element).closest !== "function") return false;
  const el = target as HTMLElement;
  const tag = el.tagName;
  if (tag === "TEXTAREA" || tag === "SELECT") return true;
  if (tag === "INPUT") return !NON_TEXT_INPUTS.has(((el as HTMLInputElement).type || "text").toLowerCase());
  if (el.isContentEditable) return true;
  const editable = el.closest("[contenteditable]");
  return !!editable && editable.getAttribute("contenteditable") !== "false";
}

function hit(action: ShortcutAction): ShortcutResolution {
  return { action, preventDefault: true };
}

/** F5, Ctrl+F5, Shift+F5, Ctrl+R, Ctrl+Shift+R: the webview's reload keys. */
function isReloadKey(e: KeyInput): boolean {
  if (e.altKey || e.metaKey) return false;
  if (e.key === "F5") return true;
  return !!e.ctrlKey && (e.key === "r" || e.key === "R" || e.code === "KeyR");
}

/** Which of 1…5 an Alt+digit press is (by code first, so layouts where Alt+digit types a symbol still work). */
function digitOf(e: KeyInput): number | null {
  const m = /^(?:Digit|Numpad)([1-9])$/.exec(e.code ?? "") ?? /^([1-9])$/.exec(e.key);
  return m ? Number(m[1]) : null;
}

/**
 * The action a key press means, and whether to swallow it.
 * @param e the key press
 * @param ctx where focus is and whether a dialog is open
 * @return the action (or null) and whether to call preventDefault
 */
export function resolveShortcut(e: KeyInput, ctx: ShortcutContext): ShortcutResolution {
  if (e.isComposing) return NONE;

  if (isReloadKey(e)) {
    // Always swallowed: a reload would drop whatever Silo is in the middle of.
    const run = !ctx.modalOpen && !e.repeat;
    return { action: run ? { kind: "rescan" } : null, preventDefault: true };
  }

  if (ctx.modalOpen || e.defaultPrevented || e.metaKey) return NONE;

  const ctrl = !!e.ctrlKey;
  const alt = !!e.altKey;
  const shift = !!e.shiftKey;

  // Allowed while typing.
  if (ctrl && !alt && !shift && (e.key === "k" || e.key === "K" || e.code === "KeyK")) {
    return hit({ kind: "focusSearch" });
  }
  if (ctrl && !alt && !shift && (e.key === "," || e.code === "Comma")) return hit({ kind: "settings" });
  if (e.key === "F6" && !ctrl && !alt) return hit({ kind: "region", dir: shift ? -1 : 1 });

  if (ctx.typing) return NONE;

  if (alt && !ctrl && !shift) {
    if (e.key === "ArrowLeft") return hit({ kind: "back" });
    const n = digitOf(e);
    if (n !== null && n <= VIEW_SHORTCUTS.length) return hit({ kind: "view", view: VIEW_SHORTCUTS[n - 1] });
    return NONE;
  }

  if (!ctrl && !alt) {
    if (e.key === "/" && !shift) return hit({ kind: "focusSearch" });
    if (e.key === "?") return hit({ kind: "help" });
  }
  return NONE;
}

/**
 * The aria-keyshortcuts value for a rail item ("Alt+1"), or undefined when it has none.
 * @param view the rail item's view
 */
export function viewKeyshortcut(view: ViewId): string | undefined {
  if (view === "settings") return "Control+,";
  const i = VIEW_SHORTCUTS.indexOf(view);
  return i === -1 ? undefined : `Alt+${i + 1}`;
}

/** One row of the shortcuts list: alternative key combos (each a list of key caps) and what they do. */
export interface ShortcutDoc {
  keys: readonly (readonly string[])[];
  label: string;
}

/** Every shortcut, as the Keyboard shortcuts dialog lists them. */
export const SHORTCUT_DOCS: readonly ShortcutDoc[] = [
  { keys: [["Ctrl", "K"], ["/"]], label: "Search" },
  { keys: [["F5"], ["Ctrl", "R"]], label: "Rescan your library" },
  { keys: [["Alt", "1"]], label: "Library" },
  { keys: [["Alt", "2"]], label: "Browse" },
  { keys: [["Alt", "3"]], label: "Mod sets" },
  { keys: [["Alt", "4"]], label: "Diagnose" },
  { keys: [["Alt", "5"]], label: "Updates" },
  { keys: [["Ctrl", ","]], label: "Settings" },
  { keys: [["Alt", "←"]], label: "Back to the previous page" },
  { keys: [["F6"], ["Shift", "F6"]], label: "Move between the sidebar, header, notices, page and details" },
  { keys: [["Esc"]], label: "Close the mod details, a menu or a dialog" },
  { keys: [["?"]], label: "Show keyboard shortcuts" },
];

/**
 * Listen for shortcuts on `target` (one keydown listener) and hand each resolved action to `run`.
 * @param target usually `window`
 * @param run performs an action
 * @param isModalOpen whether a modal dialog is open right now
 * @return removes the listener
 */
export function installShortcuts(
  target: Pick<Window, "addEventListener" | "removeEventListener">,
  run: (action: ShortcutAction) => void,
  isModalOpen: () => boolean,
): () => void {
  const onKeydown = (e: KeyboardEvent) => {
    const r = resolveShortcut(e, { typing: isTypingTarget(e.target), modalOpen: isModalOpen() });
    if (r.preventDefault) e.preventDefault();
    if (r.action) run(r.action);
  };
  target.addEventListener("keydown", onKeydown as EventListener);
  return () => target.removeEventListener("keydown", onKeydown as EventListener);
}
