// What the header status chip says. Callers pass problem and review counts only (info-level
// findings never count here), and whether Silo has read the library: until a scan succeeds the
// counts mean nothing, so the chip never says "All good" on a library it hasn't read.
//   statusChipModel(0, 0, libraryReadState({ scanning, scanned, failed }))
import { formatCount, plural } from "../../plural";

export type StatusTone = "ok" | "problem" | "review" | "neutral" | "warn";

/** How far Silo got reading the library: reading it for the first time, not yet, failed, or read. */
export type LibraryReadState = "checking" | "unread" | "failed" | "read";

export interface StatusChipModel {
  tone: StatusTone;
  /** Visible text: "All good", "1 problem", "3 to review", "Checking…". */
  text: string;
  /** Accessible name: "Library status: 1 problem". */
  label: string;
}

/**
 * Where reading the library stands.
 * @param s `scanning`: a scan is running; `scanned`: a scan of this mods folder has succeeded;
 *   `failed`: the last scan failed (its notice is up)
 */
export function libraryReadState(s: { scanning: boolean; scanned: boolean; failed: boolean }): LibraryReadState {
  if (s.scanning && !s.scanned) return "checking";
  if (s.failed) return "failed";
  if (!s.scanned) return "unread";
  return "read";
}

/**
 * Until the library has been read: "Checking…" while the first scan runs, "Not read yet", or
 * "Couldn't read library" (warn) when the scan failed. After a successful scan problems outrank
 * items to review; with neither, "All good".
 * @param problems error-level findings
 * @param review warnings the user should look at
 * @param library where reading the library stands (default: read)
 */
export function statusChipModel(
  problems: number,
  review: number,
  library: LibraryReadState = "read",
): StatusChipModel {
  let tone: StatusTone = "ok";
  let text = "All good";
  if (library === "checking") {
    tone = "neutral";
    text = "Checking…";
  } else if (library === "unread") {
    tone = "neutral";
    text = "Not read yet";
  } else if (library === "failed") {
    tone = "warn";
    text = "Couldn't read library";
  } else if (problems > 0) {
    tone = "problem";
    text = `${formatCount(problems)} ${plural(problems, { one: "problem", other: "problems" })}`;
  } else if (review > 0) {
    tone = "review";
    text = `${formatCount(review)} to review`;
  }
  return { tone, text, label: `Library status: ${text}` };
}

/**
 * The header's mini-status while Find the problem mod runs.
 * @param round the current test round, or null before the first one is applied
 * @return "Finding the problem mod · round 2", or "Finding the problem mod"
 */
export function bisectStatusText(round: number | null): string {
  return round ? `Finding the problem mod · round ${round}` : "Finding the problem mod";
}
