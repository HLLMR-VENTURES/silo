// Build guard for confirmations (#177, #226). Fails (exit 1) when, anywhere under src/:
//  - a bare `confirm(` / `window.confirm(` appears: tauri-plugin-dialog replaces `window.confirm`
//    with an async function, so it returns an always-truthy Promise and the guarded action runs
//    no matter what the user clicks;
//  - `prompt(` / `window.prompt(` appears: views ask for text with `await askText(...)`;
//  - an `ask(` / `confirmAction(` / `askText(` / `askOrOs(` call isn't awaited (or returned, or
//    chained with `.then(`, or the direct body of an arrow `=> ask(`): an un-awaited one is a
//    Promise, always truthy, so a `!ask(...)` guard never blocks.
// Use `await ask({...})` from src/lib/ui (or `await askOrOs({...})` from src/lib/confirm.ts in
// code that can run before the app shell mounts). Comment lines (`//`, ` *`, `/*`) are skipped.
// The checks are simple line regexes, exported for src/lib/checkConfirm.test.ts; the scan only
// runs when this file is executed directly.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** A bare `confirm(` / `window.confirm(` call (not `askConfirm(`). */
export const bareConfirm = /(?<![\w$])(?:window\.)?confirm\s*\(/;
/** A `prompt(` / `window.prompt(` call (not `x.prompt(`, not `askPrompt(`). */
export const barePrompt = /(?<![\w$.])prompt\s*\(|(?<![\w$])window\.prompt\s*\(/;
/** Any call of the in-app confirm/prompt helpers. */
export const dialogCall = /(?<![\w$.])(?:ask|confirmAction|askText|askOrOs)\s*\(/g;
/** What may directly precede such a call: `await`, `return`, `.then(`, or an arrow `=>`. */
export const allowedBefore = /(?:\bawait|\breturn|\.then\(|=>)\s*$/;

/** Files allowed to define (not call) the helpers. */
const DEFINITIONS = ["src/lib/ui/dialogs.svelte.ts", "src/lib/confirm.ts"];
/** Files not scanned at all: this guard's own test, which spells out the bad patterns. */
const SKIP = ["src/lib/checkConfirm.test.ts"];

/**
 * True for comment lines the scan skips.
 * @param {string} line
 */
export function isCommentLine(line) {
  return /^\s*(?:\/\/|\/\*|\*)/.test(line);
}

/**
 * The problem with one source line, or null.
 * @param {string} line
 * @param {boolean} [skipAwait] skip the await check (files that define the helpers, and tests,
 *   which hold the Promise on purpose)
 * @returns {string | null}
 */
export function lineProblem(line, skipAwait = false) {
  if (isCommentLine(line)) return null;
  if (bareConfirm.test(line)) return "bare confirm()";
  if (barePrompt.test(line)) return "prompt()";
  if (skipAwait) return null;
  for (const m of line.matchAll(dialogCall)) {
    // `function ask(` / `export const confirmAction = ask;` aren't calls of it.
    const before = line.slice(0, m.index);
    if (/\bfunction\s*$/.test(before)) continue;
    if (!allowedBefore.test(before)) return `un-awaited ${m[0].replace(/\s*\($/, "")}()`;
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
        const why = lineProblem(line, skipAwait);
        if (why) hits.push(`${rel}:${i + 1}: ${why}: ${line.trim()}`);
      });
  }
  if (hits.length) {
    console.error(
      "Unsafe confirmation found. A bare confirm() returns a Promise under tauri-plugin-dialog, so Cancel " +
        "is ignored (#177); prompt() is replaced by askText(); and an un-awaited ask()/askText()/askOrOs() " +
        "is always truthy.\n" +
        "Use `await ask({...})` / `await askText({...})` from src/lib/ui, or `await askOrOs({...})` from " +
        "src/lib/confirm.ts before the app shell mounts:\n  " +
        hits.join("\n  "),
    );
    process.exit(1);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) main();
