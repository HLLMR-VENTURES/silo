#!/usr/bin/env node
// Release asset digests (#359). The release workflow attests each build job's bundles from the
// files the job just built, and the `checksums` job later re-downloads the draft release to write
// SHA256SUMS.txt. This script ties the two together, so a release asset swapped on the draft in
// between fails the release instead of getting a checksum line and an attestation:
//
//   node scripts/release-assets.mjs record <out.sha256>
//     In a build job: hashes every file in ARTIFACT_PATHS (tauri-action's `artifactPaths` output,
//     a JSON array; directories such as Silo.app are skipped, tauri-action uploads their
//     .tar.gz) and writes `<sha256>  <asset name>` lines, the format attest-build-provenance's
//     `subject-checksums` reads. Lines end with the OS line ending, which is what it splits on.
//     RUST_TARGET is the job's `--target` (empty for a native build).
//
//   node scripts/release-assets.mjs verify <digests dir> <assets dir> <owner/repo> <tag>
//     In `checksums`: every downloaded asset must have exactly the name and digest a build job
//     (or the SBOM job) recorded, every recorded file must be on the release, and latest.json
//     must name this tag's version and give every updater platform the right kind of verified
//     asset with the signature from its verified .sig. Exits 1 with one line per problem.
//
// Asset names are the built file names, except that tauri-action uploads the macOS updater
// archive `Silo.app.tar.gz` (and its .sig) as `Silo_<arch>.app.tar.gz`, with arch `aarch64` or
// `x64`; the record step names them the same way, so the two Mac builds can't be swapped.
// The checks are exported for src/lib/releaseScripts.test.ts; the commands only run when this
// file is executed directly.
import { createHash } from "node:crypto";
import { readdirSync, readFileSync, statSync, writeFileSync } from "node:fs";
import { EOL } from "node:os";
import { basename, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** Asset suffixes tauri-action renames to `<stem>_<arch><suffix>`; longest first. */
const ARCH_RENAMED = [".app.tar.gz.sig", ".app.tar.gz"];

/** tauri-action's `<arch>` in those names, by the build's `--target`. */
const MAC_ARCH = { "aarch64-apple-darwin": "aarch64", "x86_64-apple-darwin": "x64" };

/** The release's updater manifest, generated and merged by tauri-action, checked on its own. */
export const LATEST_JSON = "latest.json";

/**
 * Every platform the release's latest.json must list, and the asset each must point at. Mirrors
 * release.yml's build matrix (macOS Apple Silicon and Intel, Linux x64, Windows x64) and the keys
 * tauri-action writes; `windows-x86_64` is the MSI because `updaterJsonPreferNsis` is off. Add a
 * row when the matrix gains a platform.
 */
export const UPDATER_PLATFORMS = {
  "darwin-aarch64": /_aarch64\.app\.tar\.gz$/,
  "darwin-aarch64-app": /_aarch64\.app\.tar\.gz$/,
  "darwin-x86_64": /_x64\.app\.tar\.gz$/,
  "darwin-x86_64-app": /_x64\.app\.tar\.gz$/,
  "linux-x86_64": /_amd64\.AppImage$/,
  "linux-x86_64-appimage": /_amd64\.AppImage$/,
  "linux-x86_64-deb": /_amd64\.deb$/,
  "linux-x86_64-rpm": /\.x86_64\.rpm$/,
  "windows-x86_64": /_x64_[A-Za-z-]+\.msi$/,
  "windows-x86_64-msi": /_x64_[A-Za-z-]+\.msi$/,
  "windows-x86_64-nsis": /_x64-setup\.exe$/,
};

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
 * The release asset name tauri-action uploads a built file as: its file name, with `_<arch>`
 * added to the macOS updater archive and its .sig. Throws for a Mac archive from a build whose
 * target has no known arch, rather than guessing.
 * @param {string} file the built file's name
 * @param {string} rustTarget the build's `--target`, or "" for a native build
 * @returns {string}
 */
export function uploadName(file, rustTarget) {
  const suffix = ARCH_RENAMED.find((s) => file.endsWith(s));
  if (!suffix) return file;
  const arch = Object.hasOwn(MAC_ARCH, rustTarget) ? MAC_ARCH[rustTarget] : null;
  if (!arch) throw new Error(`No macOS arch for target '${rustTarget}' to name ${file}.`);
  return `${file.slice(0, -suffix.length)}_${arch}${suffix}`;
}

/**
 * `<sha256>  <asset name>` lines for the regular files among `paths` (directories are skipped).
 * Throws when a path doesn't exist or no file is left, so a build that produced nothing fails.
 * @param {string[]} paths
 * @param {{ rustTarget?: string, hash?: (path: string) => string, fileCheck?: (path: string) => boolean }} [opts]
 *   the build's `--target` (default native), and the hash and file test (default the real ones)
 * @returns {string[]}
 */
export function digestLines(paths, { rustTarget = "", hash = sha256File, fileCheck = isFile } = {}) {
  const lines = [];
  for (const p of paths) {
    if (!fileCheck(p)) continue;
    lines.push(`${hash(p)}  ${uploadName(basename(p), rustTarget)}`);
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
  const platforms =
    manifest?.platforms && typeof manifest.platforms === "object" ? manifest.platforms : {};
  for (const platform of Object.keys(UPDATER_PLATFORMS)) {
    if (!Object.hasOwn(platforms, platform)) problems.push(`${LATEST_JSON} has no ${platform} entry.`);
  }
  const prefix = `https://github.com/${repo}/releases/download/${tag}/`;
  for (const [platform, entry] of Object.entries(platforms)) {
    const kind = Object.hasOwn(UPDATER_PLATFORMS, platform) ? UPDATER_PLATFORMS[platform] : null;
    if (!kind) {
      problems.push(`${LATEST_JSON} lists ${platform}, which this release doesn't build.`);
      continue;
    }
    const url = typeof entry?.url === "string" ? entry.url : "";
    if (!url.startsWith(prefix)) {
      problems.push(`${LATEST_JSON} ${platform}: url '${url}' isn't an asset of this release.`);
      continue;
    }
    const name = decodeURIComponent(url.slice(prefix.length));
    if (!kind.test(name)) {
      problems.push(`${LATEST_JSON} ${platform}: ${name} isn't the ${platform} build.`);
    } else if (!verified.has(name)) {
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
  const expected = new Map();
  for (const r of recorded) {
    if (expected.has(r.name)) problems.push(`${r.name} was recorded twice.`);
    expected.set(r.name, r.digest);
  }
  const verified = new Set();
  for (const [name, digest] of Object.entries(assets).sort(([a], [b]) => a.localeCompare(b))) {
    if (name === LATEST_JSON) continue;
    const want = expected.get(name);
    if (want === undefined) {
      problems.push(`${name} (sha256 ${digest}) isn't a file the build jobs recorded.`);
    } else if (want !== digest) {
      problems.push(`${name} (sha256 ${digest}) isn't what the build recorded (sha256 ${want}).`);
    } else {
      verified.add(name);
    }
  }
  for (const [name, digest] of expected) {
    if (!Object.hasOwn(assets, name)) {
      problems.push(`Recorded build output ${name} (sha256 ${digest}) isn't on the release.`);
    }
  }
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
  const lines = digestLines(paths, { rustTarget: process.env.RUST_TARGET ?? "" });
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
    "Usage: release-assets.mjs record <out.sha256>  (reads ARTIFACT_PATHS, RUST_TARGET)\n" +
      "       release-assets.mjs verify <digests dir> <assets dir> <owner/repo> <tag>",
  );
  process.exit(2);
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) main();
