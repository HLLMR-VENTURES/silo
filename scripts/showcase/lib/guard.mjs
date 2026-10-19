// Proof that a showcase run left the real profile alone: fingerprint (path, size, mtime) the real
// FS25 user folder and Silo's own data folders before the app starts, and compare afterwards.
// Read-only: it only lists and stats. The keychain isn't covered (a profile never opens it).
//   const before = fingerprintRealProfile();  ...run...  assertUnchanged(before);
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { execFileSync } from "node:child_process";

/** The user's Documents folder (it may be redirected, so ask the shell's own setting). */
function documentsDir() {
  if (process.platform === "win32") {
    try {
      const out = execFileSync("reg", ["query", "HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\Shell Folders", "/v", "Personal"], { encoding: "utf8" });
      const m = out.match(/Personal\s+REG_SZ\s+(.+)/);
      if (m) return m[1].trim();
    } catch {
      /* fall back below */
    }
  }
  return path.join(os.homedir(), "Documents");
}

/**
 * The real folders a showcase run must never change, with how deep to look in each.
 * @returns {{ path: string, depth: number }[]}
 */
export function realProfileRoots() {
  const home = os.homedir();
  const roots = [{ path: path.join(documentsDir(), "My Games", "FarmingSimulator2025"), depth: 2 }];
  if (process.platform === "win32") {
    roots.push({ path: path.join(process.env.APPDATA ?? path.join(home, "AppData", "Roaming"), "com.hllmr.silo"), depth: 3 });
    roots.push({ path: path.join(process.env.LOCALAPPDATA ?? path.join(home, "AppData", "Local"), "com.hllmr.silo"), depth: 2 });
  }
  return roots;
}

function walk(root, depth, out, rel = "") {
  let entries;
  try {
    entries = fs.readdirSync(path.join(root, rel), { withFileTypes: true });
  } catch {
    return;
  }
  for (const e of entries) {
    const r = path.join(rel, e.name);
    let st;
    try {
      st = fs.lstatSync(path.join(root, r));
    } catch {
      continue;
    }
    out.set(r, `${st.isDirectory() ? "d" : st.size}:${Math.round(st.mtimeMs)}`);
    if (e.isDirectory() && depth > 1) walk(root, depth - 1, out, r);
  }
}

/** @returns {Map<string, Map<string, string>>} root path -> (relative path -> "size:mtime") */
export function fingerprintRealProfile(roots = realProfileRoots()) {
  const all = new Map();
  for (const { path: p, depth } of roots) {
    const m = new Map();
    walk(p, depth, m);
    all.set(p, m);
  }
  return all;
}

/**
 * Differences between two fingerprints, as readable lines (empty when nothing changed).
 * @param {ReturnType<typeof fingerprintRealProfile>} before
 * @param {ReturnType<typeof fingerprintRealProfile>} after
 */
export function diffFingerprints(before, after) {
  const lines = [];
  for (const [root, b] of before) {
    const a = after.get(root) ?? new Map();
    for (const [k, v] of b) if (!a.has(k)) lines.push(`removed ${path.join(root, k)}`);
      else if (a.get(k) !== v) lines.push(`changed ${path.join(root, k)}`);
    for (const k of a.keys()) if (!b.has(k)) lines.push(`added   ${path.join(root, k)}`);
  }
  return lines;
}

/** Throw if the real profile changed since `before`. */
export function assertUnchanged(before) {
  const diff = diffFingerprints(before, fingerprintRealProfile());
  if (diff.length) throw new Error(`the real profile changed during the run:\n  ${diff.slice(0, 40).join("\n  ")}`);
}

/** Text that would identify the person running this: their home folder or user name. */
export function personalTextPattern(home = os.homedir(), user = os.userInfo().username) {
  const esc = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const homes = [home, home.replace(/\\/g, "/")].map(esc).join("|");
  // The user name on its own, but not inside a web address such as silo.hllmr.com.
  return new RegExp(`(${homes})|(?<![\\w.@/-])${esc(user)}(?![\\w-]|\\.com)`, "i");
}
