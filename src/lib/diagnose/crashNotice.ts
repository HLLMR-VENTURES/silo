// The crash notice's words (Diagnose, #260), from a log report and the library. Pure.
//   const c = likelyCulprit(report, (t) => lib.get(t) ?? null);
//   const { title, body } = crashNoticeText(c, { suspects: 46 });
import { formatCount, plural } from "../plural";
import type { LogReport } from "../types";

/** The library's view of a mod the log names: its title and whether it's active. */
export type LibraryLookup = (techName: string) => { title: string; active: boolean } | null;

/** The mod the crash most likely came from. */
export interface CrashCulprit {
  /** The name the log uses (a tech name). */
  modName: string;
  /** The library title, or the log name when the library doesn't have it. */
  title: string;
  inLibrary: boolean;
  active: boolean;
  errors: number;
}

/**
 * The mod with the most errors in the log (the report is sorted worst first), matched to the
 * library by tech name. Null when no mod logged an error.
 * @param report the parsed log
 * @param lookup the library by tech name
 */
export function likelyCulprit(report: LogReport, lookup: LibraryLookup): CrashCulprit | null {
  const top = report.mods.find((m) => m.errors > 0);
  if (!top) return null;
  const lib = lookup(top.modName);
  return {
    modName: top.modName,
    title: lib?.title ?? top.modName,
    inLibrary: !!lib,
    active: lib?.active ?? false,
    errors: top.errors,
  };
}

/** About how many launches finding the problem mod takes for `n` suspects (halving). */
export function bisectRounds(n: number): number {
  return Math.max(1, Math.ceil(Math.log2(Math.max(n, 2))));
}

/** "14 errors" / "1 error". */
export function errorCount(n: number): string {
  return `${formatCount(n)} ${plural(n, { one: "error", other: "errors" })}`;
}

/**
 * Title and body of the crash notice.
 * @param c the likely culprit, or null when the log names no mod
 * @param o `parked`: the user parked the culprit from the notice; `suspects`: how many mods
 *   finding the problem mod would test (active mods that aren't maps)
 */
export function crashNoticeText(
  c: CrashCulprit | null,
  o: { parked?: boolean; suspects: number },
): { title: string; body: string } {
  if (!c) {
    return {
      title: "Your last game ended unexpectedly, and the log doesn't name a mod.",
      body:
        o.suspects >= 2
          ? `Silo can find the problem mod by testing half your mods at a time (about ${formatCount(bisectRounds(o.suspects))} ${plural(bisectRounds(o.suspects), { one: "launch", other: "launches" })}).`
          : "See what the log says in Diagnose.",
    };
  }
  const title = "Your last game ended unexpectedly.";
  if (o.parked) return { title, body: `You parked ${c.title}. Play again to check.` };
  const cause = `Most likely cause: ${c.title} (${errorCount(c.errors)} in the log).`;
  if (!c.inLibrary) return { title, body: `${cause} It isn't a mod in your library.` };
  if (!c.active) return { title, body: `${cause} It's parked now.` };
  return { title, body: cause };
}

/** What the crash notice offers: park the culprit (only an active library mod), or find it. */
export function crashNoticeOffers(
  c: CrashCulprit | null,
  o: { parked?: boolean; suspects: number },
): { park: boolean; find: boolean } {
  if (!c) return { park: false, find: o.suspects >= 2 };
  return { park: !o.parked && c.inLibrary && c.active, find: false };
}

export interface CrashNoticeInput {
  /** The culprit as detected, or null when the log names no mod. */
  culprit: CrashCulprit | null;
  /** The culprit's library entry now (title, active), or null. */
  live: { title: string; active: boolean } | null;
  /** The user parked the culprit from the notice. */
  parked: boolean;
  /** Mods Find the problem mod would test. */
  suspects: number;
  /** The library has been read (or failed to): until then Silo can't match the log's mod. */
  libraryKnown: boolean;
  /** Find the problem mod is running: it owns the crash until it ends. */
  bisecting: boolean;
  /** An interrupted search's mods are waiting to be restored: a new one can't start. */
  recoveryPending: boolean;
}

export interface CrashNoticeModel {
  title: string;
  body: string;
  /** The culprit's title when "Park <title>" is offered, else null. */
  park: string | null;
  /** "Find the problem mod" is offered. */
  find: boolean;
}

/**
 * What the crash notice shows now, or null when it should be hidden (the library isn't read
 * yet, or Find the problem mod is running). The culprit is matched to the library as it is now,
 * so a park undone elsewhere offers Park again.
 */
export function crashNoticeModel(i: CrashNoticeInput): CrashNoticeModel | null {
  if (!i.libraryKnown || i.bisecting) return null;
  const culprit: CrashCulprit | null = i.culprit && {
    ...i.culprit,
    title: i.live?.title ?? i.culprit.modName,
    inLibrary: !!i.live,
    active: i.live?.active ?? false,
  };
  const o = { parked: i.parked && !culprit?.active, suspects: i.suspects };
  const text = crashNoticeText(culprit, o);
  const offers = crashNoticeOffers(culprit, o);
  const find = offers.find && !i.recoveryPending;
  const body =
    offers.find && i.recoveryPending
      ? "Put your mods back from the interrupted search first, then Silo can find the problem mod."
      : text.body;
  return { title: text.title, body, park: offers.park && culprit ? culprit.title : null, find };
}
