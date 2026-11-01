#!/usr/bin/env node
// Release version guard (#359). The release workflow's `guard` job runs this on the tagged commit
// and refuses to build unless:
//  - the tag has no `-` (release.yml publishes a stable, non-prerelease draft, so `v1.0.4-rc.1`
//    would ship as Latest);
//  - package.json, src-tauri/Cargo.toml and src-tauri/tauri.conf.json all carry the same plain
//    `X.Y.Z` version;
//  - the tag is exactly `v<version>` (a tag pushed before the version bump would make a release
//    whose installers and latest.json still say the old version, so the updater offers nothing).
// Exits 1 with one line per problem.
//
//   node scripts/check-release-version.mjs v1.0.4          # check the repo this script lives in
//   node scripts/check-release-version.mjs v1.0.4 <root>   # check another copy of the repo
//
// The checks are exported for src/lib/releaseScripts.test.ts; they only run when this file is
// executed directly.
import { readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

/** A plain release version: digits only, no pre-release or build suffix. */
const PLAIN_VERSION = /^\d+\.\d+\.\d+$/;

/**
 * The `version` of Cargo.toml's `[package]` table, or null when it has none (or inherits it with
 * `version.workspace = true`, which this check doesn't follow).
 * @param {string} toml Cargo.toml text
 * @returns {string | null}
 */
export function cargoPackageVersion(toml) {
  let inPackage = false;
  for (const raw of toml.split(/\r?\n/)) {
    const line = raw.trim();
    if (line.startsWith("[")) {
      inPackage = line === "[package]";
      continue;
    }
    if (!inPackage) continue;
    const m = /^version\s*=\s*"([^"]*)"\s*(?:#.*)?$/.exec(line);
    if (m) return m[1];
  }
  return null;
}

/**
 * The top-level `version` string of a JSON file's text, or null when it is missing, not a
 * string, or the text isn't JSON.
 * @param {string} text
 * @returns {string | null}
 */
export function jsonVersion(text) {
  try {
    const v = JSON.parse(text)?.version;
    return typeof v === "string" ? v : null;
  } catch {
    return null;
  }
}

/**
 * Every reason `tag` can't be released from these files; empty when it can.
 * @param {{ tag: string, packageJson: string, cargoToml: string, tauriConf: string }} input
 *   the tag name and the three files' text
 * @returns {string[]}
 */
export function releaseVersionProblems({ tag, packageJson, cargoToml, tauriConf }) {
  const problems = [];
  if (!tag || !tag.startsWith("v")) problems.push(`Tag '${tag}' doesn't start with 'v'.`);
  if (tag.includes("-")) {
    problems.push(
      `Tag '${tag}' has a '-': this workflow makes a stable release, so pre-release tags aren't built.`,
    );
  }
  const found = {
    "package.json": jsonVersion(packageJson),
    "src-tauri/Cargo.toml": cargoPackageVersion(cargoToml),
    "src-tauri/tauri.conf.json": jsonVersion(tauriConf),
  };
  for (const [file, v] of Object.entries(found)) {
    if (v === null) problems.push(`${file} has no version.`);
    else if (!PLAIN_VERSION.test(v)) problems.push(`${file} version '${v}' isn't a plain X.Y.Z version.`);
  }
  const distinct = [...new Set(Object.values(found).filter((v) => v !== null))];
  if (distinct.length > 1) {
    const list = Object.entries(found)
      .map(([file, v]) => `${file}=${v ?? "none"}`)
      .join(", ");
    problems.push(`The version files disagree: ${list}.`);
  } else if (distinct.length === 1 && tag !== `v${distinct[0]}`) {
    problems.push(`Tag '${tag}' doesn't match the app version ${distinct[0]}: tag v${distinct[0]}, or bump the version files first.`);
  }
  return problems;
}

/** The repo this script lives in. */
const REPO_ROOT = resolve(fileURLToPath(new URL("..", import.meta.url)));

/**
 * The three version files' text under `root` (empty text for a missing file).
 * @param {string} [root] repo root; defaults to the repo this script lives in
 * @returns {{ packageJson: string, cargoToml: string, tauriConf: string }}
 */
export function readVersionFiles(root = REPO_ROOT) {
  const read = (rel) => {
    try {
      return readFileSync(join(root, rel), "utf8");
    } catch {
      return "";
    }
  };
  return {
    packageJson: read("package.json"),
    cargoToml: read(join("src-tauri", "Cargo.toml")),
    tauriConf: read(join("src-tauri", "tauri.conf.json")),
  };
}

function main() {
  const [tag = "", rootArg] = process.argv.slice(2);
  const problems = releaseVersionProblems({ tag, ...readVersionFiles(rootArg ? resolve(rootArg) : REPO_ROOT) });
  if (problems.length) {
    const prefix = process.env.GITHUB_ACTIONS === "true" ? "::error::" : "";
    for (const p of problems) console.error(prefix + p);
    process.exit(1);
  }
  console.log(`Release version check passed: ${tag} matches package.json, Cargo.toml and tauri.conf.json.`);
}

if (process.argv[1] && resolve(process.argv[1]) === resolve(fileURLToPath(import.meta.url))) main();
