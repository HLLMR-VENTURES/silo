// One message model for the whole app.
//   Notices: standing conditions in the notice area ("Farming Simulator 25 is running").
//     setNotice({ key: "game-running", level: "blocking", title: "...", dismissible: false });
//     clearNotice("game-running");
//   Toasts: one-off results. success("Mod set saved"), info("..."), error(e, { op, retry }).
//   reportError(e, op): error() plus a line in Silo's log file.
//   action(op, fn): runs fn as a tracked op, reports a failure, returns undefined on error.
//   Activity: everything above lands in `activity` (re-exported from ./activity.svelte).
// Copy for errors comes from describeError() in src/lib/errorCopy.ts.

import { untrack } from "svelte";
import { logFrontendError } from "../api";
import { describeError, formatDetail } from "../errorCopy";
import type { IconName } from "../ui/icons";
import { toast, type ToastAction, type ToastHandle } from "../ui/toasts.svelte";
import { pushActivity, type ActivityLevel } from "./activity.svelte";
import { start, type OpClass } from "./ops.svelte";

export {
  activity,
  clearActivity,
  markActivitySeen,
  unseenErrorCount,
  ACTIVITY_LIMIT,
  type ActivityEntry,
  type ActivityKind,
  type ActivityLevel,
} from "./activity.svelte";

// ── Notices ──

export type NoticeLevel = "blocking" | "warning" | "info";

export interface NoticeAction {
  label: string;
  run: () => void | Promise<void>;
  primary?: boolean;
  /** Off for now (e.g. a library write is running); `title` says why. */
  disabled?: boolean;
  title?: string;
}

export interface Notice {
  /** Identifies the condition; setting the same key again updates it in place. */
  key: string;
  level: NoticeLevel;
  title: string;
  body?: string;
  progress?: { done: number; total: number; label: string };
  actions?: NoticeAction[];
  dismissible: boolean;
  icon?: IconName;
  /** Always shown, never folded under "+N more" (like blocking notices and ones with progress). */
  pinned?: boolean;
  /** Called after the user dismisses it (its close button), e.g. to remember that. */
  onDismiss?: () => void;
}

export interface NoticeEntry extends Notice {
  /** When it was raised (kept across updates unless the level changes), epoch ms. */
  at: number;
  /** Tie-break for notices raised in the same millisecond. */
  seq: number;
}

const RANK: Record<NoticeLevel, number> = { blocking: 3, warning: 2, info: 1 };
const NOTICE_ACTIVITY: Record<NoticeLevel, ActivityLevel> = {
  blocking: "error",
  warning: "warning",
  info: "info",
};

let noticeSeq = 0;
/** Raised notices in the order they were first set. Reactive; change through setNotice/clearNotice. */
export const notices: NoticeEntry[] = $state([]);

/**
 * Raise a notice, or update the one with the same key in place.
 * @param n the notice; an update keeps its place and time unless its level changes
 */
export function setNotice(n: Notice): void {
  const i = notices.findIndex((x) => x.key === n.key);
  if (i === -1) {
    notices.push({ ...n, at: Date.now(), seq: ++noticeSeq });
    return;
  }
  const old = notices[i];
  const fresh = old.level !== n.level;
  notices[i] = {
    ...n,
    at: fresh ? Date.now() : old.at,
    seq: fresh ? ++noticeSeq : old.seq,
  };
}

/**
 * Take a notice down (the condition is over). Logged to activity. No-op if it isn't up.
 * @param key the notice's key
 */
export function clearNotice(key: string): void {
  const i = notices.findIndex((x) => x.key === key);
  if (i === -1) return;
  const [n] = notices.splice(i, 1);
  pushActivity({ kind: "notice", level: NOTICE_ACTIVITY[n.level], message: n.title });
}

/**
 * The user dismissed a notice (its close button): clearNotice, then its `onDismiss`.
 * @param key the notice's key
 */
export function dismissNotice(key: string): void {
  const n = notices.find((x) => x.key === key);
  clearNotice(key);
  n?.onDismiss?.();
}

/**
 * Raise, update or clear a notice from an `$effect` that tracks the condition behind it. The
 * notice list itself isn't tracked, so the effect reruns only when its own condition changes.
 *   $effect(() => syncNotice("cloud", cloudNoticeOpen ? { level: "warning", ... } : null));
 * @param key the notice's key
 * @param n the notice to show, or null when the condition is over
 */
export function syncNotice(key: string, n: Omit<Notice, "key"> | null): void {
  untrack(() => (n ? setNotice({ ...n, key }) : clearNotice(key)));
}

function byPriority(a: NoticeEntry, b: NoticeEntry): number {
  return RANK[b.level] - RANK[a.level] || b.at - a.at || b.seq - a.seq;
}

/** Notices, most important first (blocking > warning > info, then newest). Reactive. */
export function sortedNotices(): NoticeEntry[] {
  return [...notices].sort(byPriority);
}

/** The notice to show on top, or null. Reactive. */
export function topNotice(): NoticeEntry | null {
  return sortedNotices()[0] ?? null;
}

// ── Toasts ──

export interface MessageOptions {
  action?: ToastAction;
  duration?: number;
}

/** A success toast. */
export function success(message: string, opts: MessageOptions = {}): ToastHandle {
  return toast({ message, tone: "success", ...opts });
}

/** A neutral toast. */
export function info(message: string, opts: MessageOptions = {}): ToastHandle {
  return toast({ message, tone: "neutral", ...opts });
}

/** A short, stable hash of a string (FNV-1a, hex), so distinct raw messages dedupe apart. */
function hashText(text: string): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    h ^= text.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
  }
  return (h >>> 0).toString(16);
}

/**
 * An error toast: the copy from describeError, "Try again" when `retry` is given and retrying can
 * help, and "Copy details". Errors stay until dismissed; a repeat of the same error (same code,
 * copy and raw message) on screen bumps its count. Silent codes (STILL_LOADING) show nothing
 * when `background` is set; a user's own action still says "Silo is still loading...".
 * @param e the thrown value
 * @param opts `op` = what Silo was doing ("Moving mods"); `retry` reruns it; `background` = no
 *   one asked for this (startup loads, scans)
 * @returns the toast's handle, or null when nothing was shown
 */
export function error(
  e: unknown,
  opts: { op?: string; retry?: () => void | Promise<void>; background?: boolean } = {},
): ToastHandle | null {
  const d = describeError(e, opts.op);
  if (d.silent && opts.background) {
    console.info("[Silo] silent error", d.code, d.detail.message);
    return null;
  }
  const retry = opts.retry;
  return toast({
    message: d.title,
    body: d.body,
    tone: "danger",
    sticky: true,
    detail: formatDetail(d.detail),
    dedupeKey: `${d.code}\u0000${d.title}\u0000${d.body ?? ""}\u0000${hashText(d.detail.message)}`,
    action:
      retry && d.retryable
        ? { label: "Try again", ariaLabel: `Try again: ${d.title}`, run: retry }
        : undefined,
  });
}

/**
 * Report a failure: log it to Silo's log file and tell the user.
 * @param e the thrown value
 * @param op what Silo was doing ("Moving mods")
 * @param opts `retry` offers "Try again"; `quiet` logs it and adds an activity entry, no toast;
 *   `background` = no one asked for this, so silent codes (STILL_LOADING) say nothing
 */
export function reportError(
  e: unknown,
  op: string,
  opts: { retry?: () => void | Promise<void>; quiet?: boolean; background?: boolean } = {},
): void {
  const d = describeError(e, op);
  console.error(`[Silo] ${op} failed:`, e);
  void logFrontendError("error", `${op}: ${d.detail.message}`, d.detail.code);
  if (d.silent && (opts.background || opts.quiet)) return;
  if (opts.quiet) {
    pushActivity({
      kind: "toast",
      level: "error",
      message: d.body ? `${d.title} ${d.body}` : d.title,
      detail: formatDetail(d.detail),
    });
    return;
  }
  error(e, { op, retry: opts.retry, background: opts.background });
}

/**
 * For a failure a view shows in place (its own error line) instead of as a toast: logs it like
 * `reportError(e, op, { quiet: true })` (the activity list keeps the details) and returns the
 * copy to show. Empty for silent codes (views mostly load in the background).
 *   } catch (e) { error = inlineError(e, "Loading the catalog"); }
 * @param e the thrown value
 * @param op what Silo was doing ("Loading the catalog")
 */
export function inlineError(e: unknown, op: string): string {
  reportError(e, op, { quiet: true });
  const d = describeError(e, op);
  if (d.silent) return "";
  return d.body ? `${d.title} ${d.body}` : d.title;
}

/**
 * Run `fn` as a tracked op: shows in the activity panel while it runs, reports a failure, and
 * optionally confirms success.
 * @param op what Silo is doing, fits "Silo is ..." and "while ...": "Moving mods"
 * @param fn the work
 * @param opts `exclusive` op class; `retry` for the error toast; `success` toast text
 * @returns fn's result, or undefined when it failed
 */
export async function action<T>(
  op: string,
  fn: () => Promise<T>,
  opts: { exclusive?: OpClass; retry?: () => void | Promise<void>; success?: string } = {},
): Promise<T | undefined> {
  const token = start(op, { exclusive: opts.exclusive });
  try {
    const result = await fn();
    if (opts.success) success(opts.success);
    return result;
  } catch (e) {
    reportError(e, op, { retry: opts.retry });
    return undefined;
  } finally {
    token.end();
  }
}
