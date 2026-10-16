// Build guard for #177: tauri-plugin-dialog replaces `window.confirm` with an async function,
// so a bare `confirm(...)` returns an always-truthy Promise and the guarded action runs no
// matter what the user clicks. Use `await confirmDialog(...)` from src/lib/api.ts instead.
// Fails (exit 1) if any bare `confirm(` / `window.confirm(` appears under src/.

import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";

const root = new URL("..", import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1");
const src = join(root, "src");
const bare = /(?<![\w$])(?:window\.)?confirm\s*\(/;

function* files(dir) {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) yield* files(p);
    else if (/\.(svelte|ts|js)$/.test(name)) yield p;
  }
}

const hits = [];
for (const f of files(src)) {
  readFileSync(f, "utf8")
    .split("\n")
    .forEach((line, i) => {
      if (bare.test(line)) hits.push(`${relative(root, f)}:${i + 1}: ${line.trim()}`);
    });
}

if (hits.length) {
  console.error(
    "Bare confirm() found. It returns a Promise under tauri-plugin-dialog, so Cancel is ignored (#177).\n" +
      "Use `await confirmDialog(...)` from src/lib/api.ts:\n  " +
      hits.join("\n  "),
  );
  process.exit(1);
}
