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
/** Failed looks in a row after which polling gives up (a minute; focus checks still cover it). */
export const MAX_FAILED_POLLS = 12;
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
  /** Read the log at `path` (the status's), so the report is of the log the status described
   *  even if the mods folder's FS25 profile changed in between (#387). */
  scan: (path: string | null) => Promise<LogReport>;
  /** Remember `modifiedMs` for the log at `path` (the status's), never another profile's. */
  markSeen: (modifiedMs: number, path: string | null) => Promise<void>;
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
  /** The log Silo watches changed (another FS25 profile, #387): a look still running for the
   *  old one announces nothing, and the next `check` doesn't share it. */
  logChanged: () => void;
  /** A mods-folder switch is starting (#387): no look until `resume`; one running announces
   *  nothing. */
  pause: () => void;
  /** The switch is done; the caller looks again (`check`). */
  resume: () => void;
}

/** @param deps the I/O and callbacks (injected so the timing and gating unit-test) */
export function createCrashWatch(deps: CrashWatchDeps): CrashWatch {
  const now = deps.now ?? (() => Date.now());
  let inFlight: Promise<void> | null = null;
  let timer: ReturnType<typeof setTimeout> | null = null;
  let pollStart = 0;
  let sawRunning = false;
  let failedPolls = 0;
  let active = false;
  let generation = 0;
  /** The log change time this session already handled: a status taken before the stored
   *  last-seen value caught up must not read (and announce) the same run again. */
  let handledMs: number | null = null;
  /** The log `handledMs` belongs to (each FS25 profile has its own log, #387). */
  let handledPath: string | null = null;

  function handled(s: LogStatus): boolean {
    return s.modifiedMs === handledMs && s.path === handledPath;
  }
  function setHandled(s: LogStatus, modifiedMs: number) {
    handledMs = modifiedMs;
    handledPath = s.path;
  }

  /** Moves on when the log being watched changes (another FS25 profile, #387): a look started
   *  before then never announces its run. */
  let logEpoch = 0;
  let inFlightEpoch = -1;

  async function evaluate(s: LogStatus, epoch: number): Promise<void> {
    const d = decideLog(s, now());
    // A status of a log Silo no longer watches (another FS25 profile, #387) decides nothing.
    if (epoch !== logEpoch) return;
    if (d === "skip" || s.modifiedMs == null || handled(s)) return;
    const modifiedMs = s.modifiedMs;
    if (d === "remember" || deps.quiet()) {
      await deps.markSeen(modifiedMs, s.path);
      setHandled(s, modifiedMs);
      return;
    }
    const report = await deps.scan(s.path);
    // The watched log changed while this one was read (#387): drop the run unannounced and
    // unremembered, so it's still news when Silo is back on that log.
    if (epoch !== logEpoch) return;
    // Announced first, then remembered (only after a good read, so a failed read is tried again
    // next time): a run is never remembered without having been announced. The time goes to
    // that log's own marker (`s.path`).
    setHandled(s, modifiedMs);
    if (!deps.quiet()) deps.onRun({ report, modifiedMs });
    await deps.markSeen(modifiedMs, s.path);
  }

  /** Run `work` unless a look of the current log is already running (then share it); a look of
   *  an earlier log is waited for, then this one runs. Failures stay quiet. */
  function once(work: () => Promise<void>): Promise<void> {
    if (inFlight && inFlightEpoch === logEpoch) return inFlight;
    const before = inFlight;
    inFlightEpoch = logEpoch;
    const mine: Promise<void> = (async () => {
      if (before) await before;
      await work();
    })()
      .catch(() => {
        /* background check: the next focus or poll tries again */
      })
      .finally(() => {
        if (inFlight === mine) inFlight = null;
      });
    inFlight = mine;
    return mine;
  }

  function check(): Promise<void> {
    if (paused) return Promise.resolve();
    return once(async () => {
      const epoch = logEpoch;
      await evaluate(await deps.status(), epoch);
    });
  }

  /** The log Silo watches changed (the mods folder's FS25 profile, #387): a look still running
   *  for the old one won't announce anything, and the next look starts afresh. */
  function logChanged() {
    logEpoch++;
  }

  /** While the mods folder (and so maybe its FS25 profile) is being switched, the backend may
   *  already read the new folder's log while the app still shows the old one: no look runs, and
   *  one already running announces nothing (#387). */
  let paused = false;
  function pause() {
    paused = true;
    logEpoch++;
  }
  function resume() {
    paused = false;
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
    if (!s) {
      // One failed look doesn't end the wait (a long game would lose its exit), a minute of
      // them does.
      if (++failedPolls >= MAX_FAILED_POLLS) {
        active = false;
        return;
      }
      schedule(gen);
      return;
    }
    failedPolls = 0;
    if (s.gameRunning) {
      sawRunning = true;
    } else if (sawRunning || now() - pollStart >= LAUNCH_GRACE_MS) {
      // The game Silo launched has exited (or the wait is over: it never showed up, or came and
      // went between ticks): stop polling and look now. An unchanged log is skipped. A look
      // already running (a focus check) may have started while the game still ran, so let it
      // finish, then take a fresh status rather than reuse it or this tick's.
      active = false;
      if (inFlight) await inFlight;
      await check();
      return;
    }
    schedule(gen);
  }

  function afterLaunch() {
    clearTimer();
    active = true;
    pollStart = now();
    sawRunning = false;
    failedPolls = 0;
    schedule(generation);
  }

  async function markCurrentSeen() {
    try {
      const s = await deps.status();
      if (s.modifiedMs != null && s.modifiedMs !== s.lastSeenMs) {
        await deps.markSeen(s.modifiedMs, s.path);
        setHandled(s, s.modifiedMs);
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
    logChanged,
    pause,
    resume,
  };
}
