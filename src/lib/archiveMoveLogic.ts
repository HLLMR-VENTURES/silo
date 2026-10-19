// Pure decisions and message helpers for the library archive move (#166), kept apart from the
// reactive state in `archiveMove.svelte.ts` so they unit-test without Svelte or Tauri.

import type { ArchiveStatus, MoveProgress } from "./api";

/** Join the messages that are set, with a space; null when none are. */
export const joinProblems = (...ps: (string | null | undefined)[]): string | null =>
  ps.filter(Boolean).join(" ") || null;

/** Drop a trailing period (and spaces) so joined messages don't end up with "..". */
export const unDot = (s: string): string => s.replace(/[.\s]+$/, "");

/** End a message with exactly one period. */
export const sentence = (s: string): string => unDot(s) + ".";

/** "1 mod" / "N mods". */
export const modsCount = (n: number): string => (n === 1 ? "1 mod" : `${n} mods`);

/** Several problems as separate sentences, one per line. */
export const problemLines = (ps: string[]): string => ps.map(sentence).join("\n");

/** "a, b, c +N more" */
export const capList = (xs: string[], n = 3, sep = ", "): string =>
  xs.slice(0, n).join(sep) + (xs.length > n ? ` +${xs.length - n} more` : "");

/**
 * A move is copying right now — Pause applies, Launch is blocked. True while the backend says
 * so, or while this window still waits on a move invoke (`inFlight`).
 */
export function isMoveRunning(archive: ArchiveStatus | null, inFlight: number): boolean {
  return !!archive?.running || inFlight > 0;
}

/** A move is running or paused — library writes are refused until it finishes. */
export function isMoveUnfinished(archive: ArchiveStatus | null, running: boolean): boolean {
  return running || !!archive?.moving;
}

/** The move is heading back home: a paused cancel, a confirmed Cancel still in flight, or a
 *  running move whose progress says it turned around. */
export function isMoveCancelling(
  archive: ArchiveStatus | null,
  cancelInFlight: boolean,
  running: boolean,
  progress: MoveProgress | null,
): boolean {
  return (
    archive?.moving?.direction === "back" || cancelInFlight || (running && !!progress?.cancelling)
  );
}

/** A custom archive whose folder is missing (drive unplugged): nothing can be moved out of
 *  it, so Change… / Use default re-point Silo instead. */
export function isArchiveMissing(archive: ArchiveStatus | null): boolean {
  return !!archive?.path && !archive.available && !archive.isDefault;
}

/**
 * A poll saw a move stop that this window isn't waiting on (e.g. after a webview reload), so
 * this window has to settle it.
 */
export function pollShouldSettle(
  wasRunning: boolean,
  archive: ArchiveStatus | null,
  inFlight: number,
): boolean {
  return wasRunning && !archive?.running && inFlight === 0;
}

/** What a settle knows about a Cancel clicked on a running move. */
export interface CancelEvidence {
  /** The last progress event said the move was turning around. */
  lastProgressCancelling: boolean;
  /** An archive status showed the move heading back. */
  sawTurnAround: boolean;
  /** The archive path when Cancel was clicked. */
  cancelHome: string | null;
  /** The archive path now that the move stopped. */
  archivePath: string | null | undefined;
}

/**
 * The move finished before it saw the Cancel: nothing ever said it turned around, and the
 * library isn't back where it started.
 */
export function cancelWasLost(c: CancelEvidence): boolean {
  const landedElsewhere = !c.cancelHome || c.archivePath !== c.cancelHome;
  return !c.lastProgressCancelling && !c.sawTurnAround && landedElsewhere;
}

/** Explains a lost cancel: where the library is now, and how to move it again. */
export function lostCancelText(archivePath: string | null | undefined, homeDefault: boolean): string {
  return (
    `The move finished before it could be cancelled. Your library is now in ${archivePath ?? "its new folder"}. ` +
    (homeDefault
      ? "To move it back, open Settings → Library and choose Use default."
      : "To move it again, open Settings → Library and choose Change….")
  );
}

/** After a settle: offer to file in loose mods only once the move is fully done and clean. */
export function settleMayOfferAdopt(archive: ArchiveStatus | null, problem: string | null): boolean {
  return !archive?.moving && !archive?.running && !problem;
}
