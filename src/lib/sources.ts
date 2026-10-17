// Where a mod came from (#196): pure helpers for saved source pages. Silo never requests these
// pages; the user's browser opens them.
import type { SourceRow, UpdateRow } from "./types";

/** A mod the update check couldn't answer, which the user can check on its page instead. */
export interface CheckYourselfRow {
  techName: string;
  title: string;
  url: string;
  host: string;
  lastChecked: number | null;
}

/**
 * The site name to show for a saved page ("www." dropped).
 * @param url a saved page address
 * @returns the host, or the text itself when it doesn't parse
 */
export function sourceHost(url: string): string {
  try {
    return new URL(url).host.replace(/^www\./i, "");
  } catch {
    return url;
  }
}

/**
 * A unix-seconds date as the user's local short date, e.g. "Oct 17, 2026".
 * @param secs unix seconds
 */
export function checkedDate(secs: number): string {
  return new Date(secs * 1000).toLocaleDateString(undefined, {
    year: "numeric",
    month: "short",
    day: "numeric",
  });
}

/**
 * The "Check these yourself" list: library mods that have a saved source page but got no
 * automatic answer in this check (no catalog match and no GitHub result). Never-checked
 * pages come first, then the longest since checked, then by title.
 * @param mods the library
 * @param sources saved pages by tech name
 * @param autoResults rows from the automatic update check
 */
export function checkYourselfList(
  mods: { techName: string; title?: string | null }[],
  sources: Record<string, SourceRow>,
  autoResults: UpdateRow[],
): CheckYourselfRow[] {
  const answered = new Set(
    autoResults.filter((r) => typeof r.hasUpdate === "boolean").map((r) => r.techName),
  );
  const out: CheckYourselfRow[] = [];
  for (const m of mods) {
    const s = sources[m.techName];
    if (!s || answered.has(m.techName)) continue;
    out.push({
      techName: m.techName,
      title: m.title || m.techName,
      url: s.url,
      host: sourceHost(s.url),
      lastChecked: s.lastChecked ?? null,
    });
  }
  const when = (r: CheckYourselfRow) => r.lastChecked ?? -1;
  return out.sort((a, b) => when(a) - when(b) || a.title.localeCompare(b.title));
}
