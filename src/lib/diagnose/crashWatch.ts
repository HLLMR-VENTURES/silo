// Crash detection (Diagnose, #260): notice when the game's last run ended without a clean exit.
// Silo looks at log.txt when its window regains focus, and every 5 s while a game it launched is
// running (stopping once the game exits). Looking is a cheap stat (`log_status`); the log is
// re-read (`scan_log`) only when it changed since the last look, and only once the game is
// closed: a running game's log has no exit marker yet, so it would read as a crash.
//   const watch = createCrashWatch({ status: logStatus, scan: scanLog, markSeen: markLogSeen,
//     quiet: () => bisecting, onRun: (run) => ... });
//   window focus → void watch.check();   after Play → watch.afterLaunch();
import type { LogReport, LogStatus } from "../types";

/** How often Silo looks while a game it launched runs. */
export const POLL_MS = 5_000;
/** After Play, how long to wait for the game's process to show up before giving up. */
export const LAUNCH_GRACE_MS = 3 * 60_000;
/** On the very first look (nothing stored yet), only a log this recent is news. */
export const FIRST_LOOK_MAX_AGE_MS = 24 * 60 * 60_000;

/** What to do with a log status: nothing, just remember it, or read the log. */
export type LogDecision = "skip" | "remember" | "read";

/**
 * Decide whether a log status is worth reading.
 * - The game is running, or there's no log: skip (nothing to say yet).
 * - Unchanged since the last look: skip.
 * - Never looked before and the log is older than a day: remember it without announcing (an old
 *   crash from before Silo watched isn't news).
 * - Otherwise: read it.
 * @param s the `log_status` result
 * @param now epoch ms
 */
export function decideLog(s: LogStatus, now: number): LogDecision {
  if (s.gameRunning || s.modifiedMs == null) return "skip";
  if (s.lastSeenMs != null) return s.modifiedMs === s.lastSeenMs ? "skip" : "read";
  return now - s.modifiedMs >= FIRST_LOOK_MAX_AGE_MS ? "remember" : "read";
}

/** A run Silo read: the report and the log's change time. */
export interface LogRun {
  report: LogReport;
  modifiedMs: number;
}

export interface CrashWatchDeps {
  status: () => Promise<LogStatus>;
  scan: () => Promise<LogReport>;
  markSeen: (modifiedMs: number) => Promise<void>;
  /** True while a guided diagnosis runs: it reads the log itself, so nothing is announced. */
  quiet: () => boolean;
  /** A new run was read (not while quiet). The caller announces a crash or clears an old one. */
  onRun: (run: LogRun) => void;
  now?: () => number;
}

export interface CrashWatch {
  /** Look once (window focus). Resolves when done; a look already running is reused. */
  check: () => Promise<void>;
  /** Silo just launched the game: poll every POLL_MS until it exits, then look. */
  afterLaunch: () => void;
  /** Remember the current log without announcing it (e.g. when a guided diagnosis ends). */
  markCurrentSeen: () => Promise<void>;
  /** True while polling after a launch. */
  readonly polling: boolean;
  /** Stop polling (the app is closing). */
  stop: () => void;
}

/** @param deps the I/O and callbacks (injected so the timing and gating unit-test) */
export function createCrashWatch(deps: CrashWatchDeps): CrashWatch {
  const now = deps.now ?? (() => Date.now());
  let inFlight: Promise<void> | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pollStart = 0;
  let sawRunning = false;
  let active = false;
  let generation = 0;
  /** The log change time this session already handled: a status taken before the stored
   *  last-seen value caught up must not read (and announce) the same run again. */
  let handledMs: number | null = null;

  async function evaluate(s: LogStatus): Promise<void> {
    const d = decideLog(s, now());
    if (d === "skip" || s.modifiedMs == null || s.modifiedMs === handledMs) return;
    const modifiedMs = s.modifiedMs;
    if (d === "remember" || deps.quiet()) {
      await deps.markSeen(modifiedMs);
      handledMs = modifiedMs;
      return;
    }
    const report = await deps.scan();
    // Remember it only after a good read, so a failed read is tried again next time.
    await deps.markSeen(modifiedMs);
    handledMs = modifiedMs;
    if (!deps.quiet()) deps.onRun({ report, modifiedMs });
  }

  /** Run `work` unless a look is already running (then share it). Failures stay quiet. */
  function once(work: () => Promise<void>): Promise<void> {
    if (inFlight) return inFlight;
    inFlight = work()
      .catch(() => {
        /* background check: the next focus or poll tries again */
      })
      .finally(() => (inFlight = null));
    return inFlight;
  }

  function check(): Promise<void> {
    return once(async () => evaluate(await deps.status()));
  }

  function clearTimer() {
    active = false;
    generation++;
    if (timer) clearTimeout(timer);
    timer = null;
  }

  function schedule(gen: number) {
    timer = setTimeout(() => void tick(gen), POLL_MS);
  }

  async function tick(gen: number) {
    timer = null;
    let s: LogStatus | null = null;
    try {
      s = await deps.status();
    } catch {
      /* try again next tick */
    }
    if (gen !== generation) return; // stopped, or a newer launch took over
    if (s?.gameRunning) {
      sawRunning = true;
    } else if (s && sawRunning) {
      // The game Silo launched has exited: stop polling and look now. A look already running
      // (a focus check) may have started while the game still ran, so let it finish, then take
      // a fresh status rather than reuse it or this tick's (possibly stale) one.
      active = false;
      if (inFlight) await inFlight;
      await check();
      return;
    } else if (now() - pollStart >= LAUNCH_GRACE_MS) {
      active = false; // it never started (or came and went between ticks; focus covers that)
      return;
    }
    schedule(gen);
  }

  function afterLaunch() {
    clearTimer();
    active = true;
    pollStart = now();
    sawRunning = false;
    schedule(generation);
  }

  async function markCurrentSeen() {
    try {
      const s = await deps.status();
      if (s.modifiedMs != null && s.modifiedMs !== s.lastSeenMs) {
        await deps.markSeen(s.modifiedMs);
        handledMs = s.modifiedMs;
      }
    } catch {
      /* the next look decides again */
    }
  }

  return {
    check,
    afterLaunch,
    markCurrentSeen,
    get polling() {
      return active;
    },
    stop: clearTimer,
  };
}
