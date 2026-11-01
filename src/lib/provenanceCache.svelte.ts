// Session cache of integrity verdicts, keyed by a mod's file identity (path + size + mtime)
// so a changed file invalidates automatically. Reactive (SvelteMap) so the drawer remembers
// its last verdict and the library rows can badge verified/modified mods at a glance.
// In-memory for the session; a DB-backed cache + auto-verify-all sweep are follow-ups.

import { SvelteMap, SvelteSet } from "svelte/reactivity";
import type { ModEntry, VerifyResult } from "./types";

const cache = new SvelteMap<string, VerifyResult>();

export function provKey(m: ModEntry): string {
  return `${m.path}|${m.size}|${m.mtimeMs}`;
}

export function getVerdict(m: ModEntry): VerifyResult | undefined {
  return cache.get(provKey(m));
}

export function setVerdict(m: ModEntry, r: VerifyResult): void {
  cache.set(provKey(m), r);
}

/** Files being verified now (by provKey): the drawer shows the run, and doesn't start another,
 *  when it comes back to a mod mid-run. */
const verifying = new SvelteSet<string>();

/** Is this file's integrity check running? Reactive. */
export function isVerifying(m: ModEntry): boolean {
  return verifying.has(provKey(m));
}

/**
 * Mark this file's check as running or finished.
 * @param m the mod being hashed
 * @param on true when the check starts, false when it ends (either way)
 */
export function setVerifying(m: ModEntry, on: boolean): void {
  if (on) verifying.add(provKey(m));
  else verifying.delete(provKey(m));
}

/** The last check of a file that failed, and why (by provKey): shown until it's checked again,
 *  even after the drawer showed other mods meanwhile (#358). */
const failures = new SvelteMap<string, string>();

/** Why this file's last check failed, or undefined. Reactive. */
export function getFailure(m: ModEntry): string | undefined {
  return failures.get(provKey(m));
}

/**
 * Record (or, with null, clear) why this file's check failed.
 * @param m the mod that was hashed
 * @param msg what to show, or null when a new check starts
 */
export function setFailure(m: ModEntry, msg: string | null): void {
  if (msg === null) failures.delete(provKey(m));
  else failures.set(provKey(m), msg);
}
