// Diagnose → Last run (#260): how a parsed log.txt is grouped and worded. Pure.
//   const g = groupLogMods(report);   // { causes, worth, cosmetic }
//   lastRunVerdict(report, g.causes.length) -> { tone: "bad", title: "Your last game ended unexpectedly.", ... }
import { formatCount, plural } from "../plural";
import type { LogModHealth, LogReport } from "../types";

/** Real warnings: everything but the cosmetic (missing translation) ones. */
export function realWarnings(m: LogModHealth): number {
  return Math.max(0, m.warnings - m.benign);
}

/**
 * Mods with real errors (most likely causes, worst first as the report sorts them), mods with
 * real warnings only (worth a look), and mods with cosmetic warnings only (safe to ignore).
 */
export function groupLogMods(report: LogReport): {
  causes: LogModHealth[];
  worth: LogModHealth[];
  cosmetic: LogModHealth[];
} {
  const causes: LogModHealth[] = [];
  const worth: LogModHealth[] = [];
  const cosmetic: LogModHealth[] = [];
  for (const m of report.mods) {
    if (m.errors > 0) causes.push(m);
    else if (realWarnings(m) > 0) worth.push(m);
    else cosmetic.push(m);
  }
  return { causes, worth, cosmetic };
}

export interface Verdict {
  tone: "bad" | "warn" | "good";
  /** The bold lead. */
  title: string;
  /** The rest of the sentence, or "". */
  body: string;
}

/**
 * The verdict card: a crash, a clean run with errors, or healthy.
 * @param report the parsed log
 * @param causes how many mods logged real errors
 */
export function lastRunVerdict(report: LogReport, causes: number): Verdict {
  // A log too big to read whole only vouches for the parts Silo read (#351).
  const inRead = report.truncated ? " in the parts Silo read" : "";
  if (report.crashed) {
    return {
      tone: "bad",
      title: "Your last game ended unexpectedly.",
      body:
        causes > 0
          ? "The log stops without a clean exit. The mods below logged errors during that run, most first."
          : `The log stops without a clean exit, and no mod logged an error${inRead}, so the log can't name the cause.`,
    };
  }
  if (causes > 0) {
    return {
      tone: "warn",
      title: "Your last game closed normally,",
      body: `but ${formatCount(causes)} ${plural(causes, { one: "mod", other: "mods" })} logged errors worth a look.`,
    };
  }
  return {
    tone: "good",
    title: "Healthy.",
    body: `Your last game closed normally and no mod logged a real error${inRead}.`,
  };
}

/** "14 errors · 3 warnings" (cosmetic warnings left out; "" when there's nothing). */
export function countsLine(m: LogModHealth): string {
  const parts: string[] = [];
  if (m.errors > 0) parts.push(`${formatCount(m.errors)} ${plural(m.errors, { one: "error", other: "errors" })}`);
  const w = realWarnings(m);
  if (w > 0) parts.push(`${formatCount(w)} ${plural(w, { one: "warning", other: "warnings" })}`);
  if (m.benign > 0) parts.push(`${formatCount(m.benign)} cosmetic`);
  return parts.join(" · ");
}

/** Where the sample is in log.txt: "line 120", or "near the end" when its number isn't known (#351). */
export function sampleWhere(m: Pick<LogModHealth, "sampleLine">): string {
  return m.sampleLine === null ? "near the end" : `line ${m.sampleLine}`;
}

/** The text "Copy details" puts on the clipboard for one mod. */
export function logModDetails(m: LogModHealth, title: string): string {
  const name = title === m.modName ? m.modName : `${title} (${m.modName})`;
  return [name, countsLine(m), `log.txt ${sampleWhere(m)}: ${m.sample}`].join("\n");
}

/**
 * When the log was written, for the meta line: "9:42 PM" today, else "Oct 12, 9:42 PM".
 * @param ms the log's change time (epoch ms)
 * @param now epoch ms
 * @param locale for tests; the user's locale by default
 */
export function logTimeLabel(ms: number, now: number, locale?: string): string {
  const d = new Date(ms);
  const sameDay = new Date(now).toDateString() === d.toDateString();
  const time = new Intl.DateTimeFormat(locale, { hour: "numeric", minute: "2-digit" }).format(d);
  if (sameDay) return time;
  const day = new Intl.DateTimeFormat(locale, { month: "short", day: "numeric" }).format(d);
  return `${day}, ${time}`;
}

/** "FS25 1.4.0.0 · 737 mods loaded · log from 9:42 PM" (parts it doesn't know are left out), plus a
 *  note when the log was too big to read whole (#351). */
export function lastRunMeta(report: LogReport, logTime: string | null): string {
  const parts: string[] = [];
  if (report.engineVersion) parts.push(`FS25 ${report.engineVersion}`);
  parts.push(`${formatCount(report.modCount)} ${plural(report.modCount, { one: "mod", other: "mods" })} loaded`);
  if (logTime) parts.push(`log from ${logTime}`);
  if (report.truncated) parts.push("very long log: Silo read only its start and end");
  return parts.join(" · ");
}
