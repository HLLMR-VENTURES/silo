// What the header status chip, the Play menu's first line and the Library status strip say,
// derived from data App already holds. Pure, so it unit-tests without the app.
//   libraryStatusCounts({ criticalConflicts: 1, warningConflicts: 2, missingDeps: 0, ... })
//     -> { problems: 1, review: 2 }
// Health findings (missing dependencies, unreadable mods, mods the game skips) are problems only
// for active mods: an inactive one can't break the next run, so it's something to review.
import { formatCount, plural } from "../plural";

export interface LibraryStatusInput {
  /** Conflicts with severity "critical" (the game may crash). */
  criticalConflicts: number;
  /** Conflicts with severity "warning". Info-level conflicts never count. */
  warningConflicts: number;
  /** Active mods whose dependencies aren't in the library (they fail to load). */
  missingDeps: number;
  /** Active mods Silo couldn't read (corrupt or unreadable). */
  corrupt: number;
  /** Active mods the game skips because their name starts with a digit. */
  ignored: number;
  /** The same three findings on inactive mods (each finding counts once). */
  inactiveFindings: number;
  /** Files at a managed name that aren't Silo's (left untouched; the user decides). */
  foreign: number;
}

/**
 * Problems (things that stop a mod loading or may crash the game) and items to review.
 * @param i the counts App derives from its conflicts and health data
 * @return problem and review counts for the status chip
 */
export function libraryStatusCounts(i: LibraryStatusInput): { problems: number; review: number } {
  return {
    problems: i.criticalConflicts + i.missingDeps + i.corrupt + i.ignored,
    review: i.warningConflicts + i.foreign + i.inactiveFindings,
  };
}

/**
 * The Play menu's heading line for the active set.
 * @param exactLoadoutName the loadout whose mods match the active set exactly, or null
 * @return "Harvest 2026, unchanged" or "No mod set"
 */
export function activeSetLine(exactLoadoutName: string | null): string {
  return exactLoadoutName ? `${exactLoadoutName}, unchanged` : "No mod set";
}

/**
 * The left side of the Library status strip. It only says "Ready to play" when Play would work:
 * the game was found, Play isn't blocked and there's no critical conflict.
 * @param s scan state, whether Play is ready, active and total counts, and the matching loadout's
 *   name if any
 * @return e.g. "Ready to play · 142 of 737 active · Harvest 2026", or "142 of 737 active"
 */
export function stripLeft(s: {
  scanning: boolean;
  scanned: boolean;
  /** The game was found, Play isn't blocked and no critical conflict is open. */
  ready: boolean;
  active: number;
  total: number;
  setName: string | null;
}): string {
  if (!s.scanned) return s.scanning ? "Reading your library…" : "Your library hasn't been read yet";
  const parts: string[] = [];
  if (s.scanning) parts.push("Rescanning");
  else if (s.ready) parts.push("Ready to play");
  parts.push(`${formatCount(s.active)} of ${formatCount(s.total)} active`);
  if (s.setName) parts.push(s.setName);
  return parts.join(" · ");
}

export interface StripFacts {
  criticalConflicts: number;
  /** Loose .zip mods Organize would file (0 when auto-file is on: it files them itself). */
  unfiled: number;
  /** Mods with an update available after a check. */
  updates: number;
}

export interface StripItemSpec {
  id: "critical" | "unfiled" | "updates";
  text: string;
  action: string;
}

/**
 * The strip's items, most important first (StatusStrip shows the first two).
 * @param f the counts
 * @return item ids, text and link-button labels; the caller attaches each `run`
 */
export function stripItemSpecs(f: StripFacts): StripItemSpec[] {
  const out: StripItemSpec[] = [];
  if (f.criticalConflicts > 0)
    out.push({
      id: "critical",
      text: `${formatCount(f.criticalConflicts)} critical ${plural(f.criticalConflicts, { one: "conflict", other: "conflicts" })}`,
      action: "See why",
    });
  if (f.unfiled > 0)
    out.push({
      id: "unfiled",
      text:
        f.unfiled === 1
          ? "1 new mod isn't filed"
          : `${formatCount(f.unfiled)} new mods aren't filed`,
      action: "Review",
    });
  if (f.updates > 0)
    out.push({
      id: "updates",
      text: `${formatCount(f.updates)} ${plural(f.updates, { one: "update", other: "updates" })}`,
      action: "See updates",
    });
  return out;
}
