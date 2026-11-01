#!/usr/bin/env node
// Release asset digests (#359). The release workflow attests each build job's bundles from the
// files the job just built, and the `checksums` job later re-downloads the draft release to write
// SHA256SUMS.txt. This script ties the two together, so a release asset swapped on the draft in
// between fails the release instead of getting a checksum line and an attestation:
//
//   node scripts/release-assets.mjs record <out.sha256>
//     In a build job: hashes every file in ARTIFACT_PATHS (tauri-action's `artifactPaths` output,
//     a JSON array; directories such as Silo.app are skipped, tauri-action uploads their
//     .tar.gz) and writes `<sha256>  <file name>` lines, the format attest-build-provenance's
//     `subject-checksums` reads. Lines end with the OS line ending, which is what it splits on.
//
//   node scripts/release-assets.mjs verify <digests dir> <assets dir> <owner/repo> <tag>
//     In `checksums`: every downloaded asset must match a recorded file by digest and name, every
//     recorded file must be on the release, and latest.json must name this tag's version and
//     point only at verified assets whose .sig matches its signature. Exits 1 with one line per
//     problem.
//
// Asset names are the built file names, except that tauri-action uploads the macOS updater
// archive `Silo.app.tar.gz` (and its .sig) as `Silo_<arch>.app.tar.gz`.
// The checks are exported for src/lib/releaseScripts.test.ts; the commands only run when this
// file is executed directly.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { EOL } from "node:os";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Asset suffixes tauri-action renames to `<stem>_<arch><suffix>`; longest first. */
const ARCH_RENAMED = [".app.tar.gz.sig", ".app.tar.gz"];

/** The release's updater manifest, generated and merged by tauri-action, checked on its own. */
export const LATEST_JSON = "latest.json";

/**
 * The sha256 of a file, as lowercase hex.
 * @param {string} path
 * @returns {string}
 */
export function sha256File(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}

/**
 * Whether `path` is a regular file (false for a directory); throws when it doesn't exist.
 * @param {string} path
 * @returns {boolean}
 */
export function isFile(path) {
  return statSync(path).isFile();
}

/**
 * `<sha256>  <name>` lines for the regular files among `paths` (directories are skipped).
 * Throws when a path doesn't exist or no file is left, so a build that produced nothing fails.
 * @param {string[]} paths
 * @param {(path: string) => string} [hash]
 * @param {(path: string) => boolean} [fileCheck] defaults to `isFile`
 * @returns {string[]}
 */
export function digestLines(paths, hash = sha256File, fileCheck = isFile) {
  const lines = [];
  for (const p of paths) {
    if (!fileCheck(p)) continue;
    lines.push(`${hash(p)}  ${basename(p)}`);
  }
  if (lines.length === 0) throw new Error("No build output files to record.");
  return lines;
}

/**
 * Parses `<sha256>  <name>` lines (`sha256sum` output; `*name` binary marks allowed).
 * Throws on a malformed line rather than skipping it.
 * @param {string} text
 * @returns {{ digest: string, name: string }[]}
 */
export function parseDigests(text) {
  const entries = [];
  for (const raw of text.split(/\r?\n/)) {
    if (!raw.trim()) continue;
    const m = /^([0-9a-f]{64}) [ *](.+)$/.exec(raw);
    if (!m) throw new Error(`Malformed digest line: ${raw}`);
    entries.push({ digest: m[1], name: m[2] });
  }
  return entries;
}

/** Escapes a string for use inside a RegExp. */
function escapeRe(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Whether a release asset name is the upload name of a recorded build file: the same name, or
 * the macOS updater archive renamed to `<stem>_<arch><suffix>`.
 * @param {string} recorded the built file's name
 * @param {string} asset the release asset's name
 * @returns {boolean}
 */
export function uploadNameMatches(recorded, asset) {
  if (recorded === asset) return true;
  const suffix = ARCH_RENAMED.find((s) => recorded.endsWith(s));
  if (!suffix) return false;
  const stem = recorded.slice(0, -suffix.length);
  return new RegExp(`^${escapeRe(stem)}_[A-Za-z0-9]+${escapeRe(suffix)}$`).test(asset);
}

/**
 * Problems with latest.json, given the assets that already passed the digest check.
 * @param {string | null} latestJson its text, or null when the release has none
 * @param {Set<string>} verified asset names whose bytes matched a recorded build file
 * @param {Record<string, string>} sigText text of each .sig asset, by name
 * @param {string} repo owner/repo
 * @param {string} tag
 * @returns {string[]}
 */
export function latestJsonProblems(latestJson, verified, sigText, repo, tag) {
  if (latestJson === null) return [`The release has no ${LATEST_JSON}.`];
  let manifest;
  try {
    manifest = JSON.parse(latestJson);
  } catch {
    return [`${LATEST_JSON} isn't valid JSON.`];
  }
  const problems = [];
  if (manifest?.version !== tag.replace(/^v/, "")) {
    problems.push(`${LATEST_JSON} version '${manifest?.version}' doesn't match tag ${tag}.`);
  }
  const platforms = manifest?.platforms;
  if (!platforms || typeof platforms !== "object" || Object.keys(platforms).length === 0) {
    problems.push(`${LATEST_JSON} lists no platforms.`);
    return problems;
  }
  const prefix = `https://github.com/${repo}/releases/download/${tag}/`;
  for (const [platform, entry] of Object.entries(platforms)) {
    const url = typeof entry?.url === "string" ? entry.url : "";
    if (!url.startsWith(prefix)) {
      problems.push(`${LATEST_JSON} ${platform}: url '${url}' isn't an asset of this release.`);
      continue;
    }
    const name = decodeURIComponent(url.slice(prefix.length));
    if (!verified.has(name)) {
      problems.push(`${LATEST_JSON} ${platform}: ${name} isn't a verified asset.`);
    } else if (!verified.has(`${name}.sig`) || sigText[`${name}.sig`] !== entry.signature) {
      problems.push(`${LATEST_JSON} ${platform}: signature doesn't match the verified ${name}.sig.`);
    }
  }
  return problems;
}

/**
 * Every reason the downloaded release assets aren't exactly what the build jobs recorded; empty
 * when they are.
 * @param {{
 *   recorded: { digest: string, name: string }[],
 *   assets: Record<string, string>,
 *   latestJson: string | null,
 *   sigText: Record<string, string>,
 *   repo: string,
 *   tag: string,
 * }} input `assets` maps each downloaded asset name (latest.json included or not) to its sha256
 * @returns {string[]}
 */
export function releaseAssetProblems({ recorded, assets, latestJson, sigText, repo, tag }) {
  const problems = [];
  if (recorded.length === 0) problems.push("No recorded build digests were found.");
  const used = new Array(recorded.length).fill(false);
  const verified = new Set();
  for (const [name, digest] of Object.entries(assets).sort(([a], [b]) => a.localeCompare(b))) {
    if (name === LATEST_JSON) continue;
    const candidates = recorded
      .map((r, i) => ({ r, i }))
      .filter(({ r }) => r.digest === digest && uploadNameMatches(r.name, name));
    if (candidates.length === 0) {
      problems.push(`${name} (sha256 ${digest}) doesn't match any file the build jobs recorded.`);
      continue;
    }
    const free = candidates.find(({ i }) => !used[i]);
    if (!free) {
      problems.push(`${name} matches a recorded file another asset already matched.`);
      continue;
    }
    used[free.i] = true;
    verified.add(name);
  }
  recorded.forEach((r, i) => {
    if (!used[i]) problems.push(`Recorded build output ${r.name} (sha256 ${r.digest}) isn't on the release.`);
  });
  problems.push(...latestJsonProblems(latestJson, verified, sigText, repo, tag));
  return problems;
}

/** The regular files directly inside `dir`, by name. */
function filesIn(dir) {
  return readdirSync(dir).filter((n) => isFile(join(dir, n)));
}

function record(out) {
  const raw = process.env.ARTIFACT_PATHS ?? "";
  let paths;
  try {
    paths = JSON.parse(raw);
  } catch {
    throw new Error(`ARTIFACT_PATHS isn't a JSON array: ${raw}`);
  }
  if (!Array.isArray(paths)) throw new Error(`ARTIFACT_PATHS isn't a JSON array: ${raw}`);
  const lines = digestLines(paths);
  writeFileSync(out, lines.join(EOL) + EOL);
  console.log(lines.join("\n"));
}

function verify(digestsDir, assetsDir, repo, tag) {
  const recorded = filesIn(digestsDir)
    .filter((n) => n.endsWith(".sha256"))
    .flatMap((n) => parseDigests(readFileSync(join(digestsDir, n), "utf8")));
  const assets = {};
  const sigText = {};
  for (const n of filesIn(assetsDir)) {
    assets[n] = sha256File(join(assetsDir, n));
    if (n.endsWith(".sig")) sigText[n] = readFileSync(join(assetsDir, n), "utf8");
  }
  const latestJson = LATEST_JSON in assets ? readFileSync(join(assetsDir, LATEST_JSON), "utf8") : null;
  const problems = releaseAssetProblems({ recorded, assets, latestJson, sigText, repo, tag });
  if (problems.length) {
    const prefix = process.env.GITHUB_ACTIONS === "true" ? "::error::" : "";
    for (const p of problems) console.error(prefix + p);
    process.exit(1);
  }
  console.log(`All ${Object.keys(assets).length} release assets match what the build jobs recorded.`);
}

function main() {
  const [cmd, ...args] = process.argv.slice(2);
  if (cmd === "record" && args.length === 1) return record(args[0]);
  if (cmd === "verify" && args.length === 4) return verify(args[0], args[1], args[2], args[3]);
  console.error(
    "Usage: release-assets.mjs record <out.sha256>  (reads ARTIFACT_PATHS)\n" +
      "       release-assets.mjs verify <digests dir> <assets dir> <owner/repo> <tag>",
  );
  process.exit(2);
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) main();
