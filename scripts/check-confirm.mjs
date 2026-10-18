// Build guard for confirmations (#177, #226). Fails (exit 1) when, anywhere under src/:
//  - a bare `confirm(` / `window.confirm(` appears: tauri-plugin-dialog replaces `window.confirm`
//    with an async function, so it returns an always-truthy Promise and the guarded action runs
//    no matter what the user clicks;
//  - `prompt(` / `window.prompt(` appears: views ask for text with `await askText(...)`;
//  - an `ask(` / `confirmAction(` / `askText(` / `askOrOs(` / `confirmDialog(` / `pluginConfirm(`
//    call (also as a member, `ui.ask(`) isn't awaited: an un-awaited one is a Promise, always
//    truthy, so a `!ask(...)` guard never blocks. Also allowed: `return ask(` on a line that
//    says `async`, and `async () => ask(` (the arrow is async, so whoever calls it awaits it);
//  - one of those helpers (or `confirm`) is imported or exported under another name
//    (`import { ask as yes }`), which would hide its calls from this scan. The one exception is
//    `confirm as pluginConfirm` in src/lib/api.ts, which `confirmDialog` wraps.
// Use `await ask({...})` from src/lib/ui (or `await askOrOs({...})` from src/lib/confirm.ts in
// code that can run before the app shell mounts). A line is skipped only when it is nothing but
// comment once `/* ... */` spans are removed (`//`, ` *`, `/*` lines).
// The checks are simple line regexes, exported for src/lib/checkConfirm.test.ts; the scan only
// runs when this file is executed directly.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** A bare `confirm(` / `window.confirm(` call (not `askConfirm(`). */
export const bareConfirm = /(?<![\w$])(?:window\.)?confirm\s*\(/;
/** A `prompt(` / `window.prompt(` call (not `x.prompt(`, not `askPrompt(`). */
export const barePrompt = /(?<![\w$.])prompt\s*\(|(?<![\w$])window\.prompt\s*\(/;
/** Any call of the in-app confirm/prompt helpers or the OS confirm wrappers, `ui.ask(` included. */
export const dialogCall = /(?<![\w$])(?:ask|confirmAction|askText|askOrOs|confirmDialog|pluginConfirm)\s*\(/g;
/** Directly before such a call: `await`, or `.then(` (the call is the argument). */
export const awaitedBefore = /(?:\bawait|\.then\()\s*$/;
/** Directly before such a call: the body of an `async` arrow, `async (x) => ask(` / `async x => ask(`. */
export const asyncArrowBefore = /\basync\s*(?:\([^()]*\)|[\w$]+)\s*=>\s*$/;
/** A helper imported / exported under another name. */
export const aliasImport = /(?<![\w$])(ask|askText|askOrOs|confirmAction|confirmDialog|confirm)\s+as\s+([\w$]+)/;

/** Files allowed to define (not call) the helpers. */
const DEFINITIONS = ["src/lib/ui/dialogs.svelte.ts", "src/lib/confirm.ts"];
/** The one allowed alias: api.ts imports the plugin's `confirm` as `pluginConfirm` and wraps it. */
const ALIAS_OK = { file: "src/lib/api.ts", name: "confirm", alias: "pluginConfirm" };
/** Files not scanned at all: this guard's own test, which spells out the bad patterns. */
const SKIP = ["src/lib/checkConfirm.test.ts"];

/**
 * The line with its `/* ... *\/` spans removed.
 * @param {string} line
 */
export function stripBlockComments(line) {
  return line.replace(/\/\*.*?\*\//g, " ");
}

/**
 * True for lines that are entirely comment (after removing `/* ... *\/` spans): blank, or
 * starting with `//`, `/*` or `*`.
 * @param {string} line
 */
export function isCommentLine(line) {
  const rest = stripBlockComments(line).trim();
  return rest === "" ? line.trim() !== "" : /^(?:\/\/|\/\*|\*)/.test(rest);
}

/**
 * The problem with one source line, or null.
 * @param {string} line
 * @param {boolean} [skipAwait] skip the await and alias checks (files that define the helpers,
 *   and tests, which hold the Promise on purpose)
 * @param {string} [file] the file's path from the repo root (for the api.ts alias exception)
 * @returns {string | null}
 */
export function lineProblem(line, skipAwait = false, file = "") {
  if (isCommentLine(line)) return null;
  const code = stripBlockComments(line);
  if (bareConfirm.test(code)) return "bare confirm()";
  if (barePrompt.test(code)) return "prompt()";
  if (skipAwait) return null;
  const alias = code.match(aliasImport);
  if (
    alias &&
    !(file === ALIAS_OK.file && alias[1] === ALIAS_OK.name && alias[2] === ALIAS_OK.alias)
  ) {
    return `aliased ${alias[1]}`;
  }
  for (const m of code.matchAll(dialogCall)) {
    // The text before the call, without a member prefix (`ui.` in `await ui.ask(`).
    const before = code.slice(0, m.index).replace(/(?:[\w$]+\s*\??\.\s*)+$/, "");
    // `function ask(` / `export async function confirmDialog(` aren't calls of it.
    if (/\bfunction\s*$/.test(before)) continue;
    if (awaitedBefore.test(before) || asyncArrowBefore.test(before)) continue;
    if (/\breturn\s*$/.test(before) && /\basync\b/.test(code)) continue;
    return `un-awaited ${m[0].replace(/\s*\($/, "")}()`;
  }
  return null;
}

function* files(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* files(p);
    else if (/\.(svelte|ts|js)$/.test(name)) yield p;
  }
}

function main() {
  const root = resolve(fileURLToPath(new URL("..", import.meta.url)));
  const src = join(root, "src");
  const hits = [];
  for (const f of files(src)) {
    const rel = relative(root, f).replace(/\\/g, "/");
    if (SKIP.includes(rel)) continue;
    const skipAwait = DEFINITIONS.includes(rel) || /\.test\.ts$/.test(rel);
    readFileSync(f, "utf8")
      .split("\n")
      .forEach((line, i) => {
        const why = lineProblem(line, skipAwait, rel);
        if (why) hits.push(`${rel}:${i + 1}: ${why}: ${line.trim()}`);
      });
  }
  if (hits.length) {
    console.error(
      "Unsafe confirmation found. A bare confirm() returns a Promise under tauri-plugin-dialog, so Cancel " +
        "is ignored (#177); prompt() is replaced by askText(); an un-awaited ask()/askText()/askOrOs()/" +
        "confirmDialog() is always truthy; and importing one under another name hides it from this check.\n" +
        "Use `await ask({...})` / `await askText({...})` from src/lib/ui, or `await askOrOs({...})` from " +
        "src/lib/confirm.ts before the app shell mounts:\n  " +
        hits.join("\n  "),
    );
    process.exit(1);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) main();
