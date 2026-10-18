// What the header status chip says. Callers pass problem and review counts only: info-level
// findings never count here.
import { formatCount, plural } from "../../plural";

export type StatusTone = "ok" | "problem" | "review";

export interface StatusChipModel {
  tone: StatusTone;
  /** Visible text: "All good", "1 problem", "3 to review". */
  text: string;
  /** Accessible name: "Library status: 1 problem". */
  label: string;
}

/**
 * Problems outrank items to review; with neither, "All good".
 * @param problems error-level findings
 * @param review warnings the user should look at
 */
export function statusChipModel(problems: number, review: number): StatusChipModel {
  let tone: StatusTone = "ok";
  let text = "All good";
  if (problems > 0) {
    tone = "problem";
    text = `${formatCount(problems)} ${plural(problems, { one: "problem", other: "problems" })}`;
  } else if (review > 0) {
    tone = "review";
    text = `${formatCount(review)} to review`;
  }
  return { tone, text, label: `Library status: ${text}` };
}
