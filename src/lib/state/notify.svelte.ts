// One message model for the whole app.
//   Notices: standing conditions in the notice area ("Farming Simulator 25 is running").
//     setNotice({ key: "game-running", level: "blocking", title: "...", dismissible: false });
//     clearNotice("game-running");
//   Toasts: one-off results. success("Mod set saved"), info("..."), error(e, { op, retry }).
//   reportError(e, op): error() plus a line in Silo's log file.
//   action(op, fn): runs fn as a tracked op, reports a failure, returns undefined on error.
//   Activity: everything above lands in `activity` (re-exported from ./activity.svelte).
// Copy for errors comes from describeError() in src/lib/errorCopy.ts.

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

/** The user dismissed a notice (its close button). Same as clearNotice. */
export const dismissNotice = clearNotice;

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

/**
 * An error toast: the copy from describeError, "Try again" when `retry` is given and retrying can
 * help, and "Copy details". Errors stay until dismissed; a repeat of one on screen bumps its
 * count. Silent codes (STILL_LOADING) show nothing.
 * @param e the thrown value
 * @param opts `op` = what Silo was doing ("Moving mods"); `retry` reruns it
 * @returns the toast's handle, or null when nothing was shown
 */
export function error(
  e: unknown,
  opts: { op?: string; retry?: () => void | Promise<void> } = {},
): ToastHandle | null {
  const d = describeError(e, opts.op);
  if (d.silent) {
    console.info("[Silo] silent error", d.code, d.detail.message);
    return null;
  }
  const retry = opts.retry;
  return toast({
    message: d.title,
    tone: "danger",
    sticky: true,
    detail: formatDetail(d.detail),
    dedupeKey: `${d.code}\u0000${d.title}`,
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
 * @param opts `retry` offers "Try again"; `quiet` logs it and adds an activity entry, no toast
 */
export function reportError(
  e: unknown,
  op: string,
  opts: { retry?: () => void | Promise<void>; quiet?: boolean } = {},
): void {
  const d = describeError(e, op);
  console.error(`[Silo] ${op} failed:`, e);
  void logFrontendError("error", `${op}: ${d.detail.message}`, d.detail.code);
  if (d.silent) return;
  if (opts.quiet) {
    pushActivity({ kind: "toast", level: "error", message: d.title, detail: formatDetail(d.detail) });
    return;
  }
  error(e, { op, retry: opts.retry });
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
