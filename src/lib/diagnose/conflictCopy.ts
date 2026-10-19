// Conflicts in plain words (Diagnose → Conflicts, #260). The wire severities stay
// critical / warning / info; people see Problem / Review / Note. Pure.
//   conflictTitle(c)            -> "Two mods claim the same exclusive slot"
//   conflictHeadline(list)      -> { problems: 0, review: 12, notes: 19 }
import { formatCount, plural } from "../plural";
import type { Conflict } from "../types";

export type SeverityLabel = "Problem" | "Review" | "Note";

/** critical → Problem, warning → Review, info → Note. */
export function severityLabel(s: Conflict["severity"]): SeverityLabel {
  return s === "critical" ? "Problem" : s === "warning" ? "Review" : "Note";
}

/** "Two mods", "3 mods". */
function modsWord(n: number): string {
  return n === 2 ? "Two mods" : `${formatCount(n)} mods`;
}

/** What each kind of clash is, in words ("add the same control action"). */
const WHAT: Record<string, string> = {
  action: "add the same control action",
  brand: "add the same brand",
  storeCategory: "add the same shop category",
  script: "ship a script with the same file name",
  fillType: "define the same fill type",
  uniqueType: "claim the same exclusive slot",
};

/**
 * A conflict's plain title.
 * @param c the conflict
 * @return e.g. "Two mods ship a script with the same file name", "More than one map is active"
 */
export function conflictTitle(c: Pick<Conflict, "kind" | "mods">): string {
  if (c.kind === "map") return "More than one map is active";
  const n = Math.max(c.mods.length, 2);
  return `${modsWord(n)} ${WHAT[c.kind] ?? "clash"}`;
}

/** A conflict that a "Park one…" menu can fix: two or more named mods, and not the maps note. */
export function canParkOne(c: Pick<Conflict, "kind" | "techNames">): boolean {
  return c.kind !== "map" && c.techNames.length >= 2;
}

export interface ConflictCounts {
  problems: number;
  review: number;
  notes: number;
}

/** How many conflicts are problems, to review, and notes. */
export function conflictCounts(list: Pick<Conflict, "severity">[]): ConflictCounts {
  const c = { problems: 0, review: 0, notes: 0 };
  for (const x of list) {
    if (x.severity === "critical") c.problems++;
    else if (x.severity === "warning") c.review++;
    else c.notes++;
  }
  return c;
}

/**
 * The Conflicts headline's parts: "0 problems", "12 to review", "19 notes hidden" (or "19
 * notes" when they show). Notes are left out when there are none.
 */
export function conflictHeadline(c: ConflictCounts, notesShown: boolean): string[] {
  if (c.problems + c.review + c.notes === 0) return ["No conflicts"];
  const parts = [
    `${formatCount(c.problems)} ${plural(c.problems, { one: "problem", other: "problems" })}`,
    `${formatCount(c.review)} to review`,
  ];
  if (c.notes > 0)
    parts.push(
      `${formatCount(c.notes)} ${plural(c.notes, { one: "note", other: "notes" })}${notesShown ? "" : " hidden"}`,
    );
  return parts;
}
