/**
 * The info banner's text with `more` added after what it already says, so a later message
 * (a scan's notices) doesn't wipe an earlier one (a database restored at startup). Blank and
 * repeated lines are dropped; null when there is nothing to say.
 */
export function appendInfo(current: string | null, more: string[]): string | null {
  const parts = [current ?? "", ...more].map((s) => s.trim()).filter((s) => s !== "");
  return parts.length ? [...new Set(parts)].join(" ") : null;
}
