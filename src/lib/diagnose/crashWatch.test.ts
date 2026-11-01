// Crash detection (#260): when the log is read, when a crash is announced, and how polling after
// a launch starts and stops.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  createCrashWatch,
  decideLog,
  FIRST_LOOK_MAX_AGE_MS,
  LAUNCH_GRACE_MS,
  POLL_MS,
  type CrashWatchDeps,
} from "./crashWatch";
import type { LogReport, LogStatus } from "../types";

const NOW = 1_800_000_000_000;

function status(over: Partial<LogStatus> = {}): LogStatus {
  return { path: "C:/fs25/log.txt", modifiedMs: NOW - 1000, gameRunning: false, lastSeenMs: NOW - 5000, ...over };
}

function report(crashed: boolean): LogReport {
  return {
    engineVersion: "1.4.0.0",
    modCount: 3,
    cleanExit: !crashed,
    crashed,
    mods: [],
    unattributed: 0,
    totalFindings: 0, groundPileCap: null, groundPileDisabled: [], fillTypeCap: null, droppedFillTypes: [], truncated: false, siloFolders: [],
  };
}

describe("decideLog", () => {
  it("skips while the game runs: its log has no exit marker yet", () => {
    expect(decideLog(status({ gameRunning: true }), NOW)).toBe("skip");
  });
  it("skips when there's no log", () => {
    expect(decideLog(status({ modifiedMs: null }), NOW)).toBe("skip");
  });
  it("skips an unchanged log, so one crash is announced once", () => {
    expect(decideLog(status({ modifiedMs: 42, lastSeenMs: 42 }), NOW)).toBe("skip");
  });
  it("reads a log that changed since the last look", () => {
    expect(decideLog(status({ modifiedMs: 43, lastSeenMs: 42 }), NOW)).toBe("read");
  });
  it("on the very first look, reads a recent log but only remembers an old one", () => {
    expect(decideLog(status({ lastSeenMs: null, modifiedMs: NOW - 60_000 }), NOW)).toBe("read");
    expect(decideLog(status({ lastSeenMs: null, modifiedMs: NOW - FIRST_LOOK_MAX_AGE_MS }), NOW)).toBe("remember");
  });
});

function setup(over: Partial<CrashWatchDeps> = {}, st: () => LogStatus = () => status()) {
  const base = {
    status: vi.fn(async () => st()),
    scan: vi.fn(async () => report(true)),
    markSeen: vi.fn(async (_ms: number) => {}),
    quiet: vi.fn(() => false),
    onRun: vi.fn(),
    now: () => NOW,
  };
  // Overrides are plain functions or mocks; the test reads the defaults as mocks.
  const deps = { ...base, ...over } as typeof base;
  return { deps, watch: createCrashWatch(deps) };
}

describe("check (window focus)", () => {
  it("reads a changed log, remembers it, then reports the run", async () => {
    const { deps, watch } = setup();
    await watch.check();
    expect(deps.scan).toHaveBeenCalledTimes(1);
    expect(deps.markSeen).toHaveBeenCalledWith(NOW - 1000, "C:/fs25/log.txt");
    expect(deps.onRun).toHaveBeenCalledWith({ report: report(true), modifiedMs: NOW - 1000 });
  });

  it("reads and marks the very log the status named, and tells two profiles' logs apart (#387)", async () => {
    let st = status({ path: "F:/prof/log.txt" });
    const { deps, watch } = setup({}, () => st);
    await watch.check();
    expect(deps.scan).toHaveBeenCalledWith("F:/prof/log.txt");
    expect(deps.markSeen).toHaveBeenCalledWith(NOW - 1000, "F:/prof/log.txt");
    // Another profile's log with the same change time is still news.
    st = status({ path: "C:/fs25/log.txt" });
    await watch.check();
    expect(deps.scan).toHaveBeenCalledTimes(2);
    expect(deps.scan).toHaveBeenLastCalledWith("C:/fs25/log.txt");
  });

  it("a look of the old log announces nothing once the log changed, and the next look runs anew (#387)", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    let st = status({ path: "C:/fs25/log.txt" });
    const scan = vi.fn(async (p: string | null) => {
      if (p === "C:/fs25/log.txt") await gate;
      return report(true);
    });
    const { deps, watch } = setup({ scan }, () => st);
    const old = watch.check();
    await Promise.resolve();
    // The profile changes while the old log is being read.
    watch.logChanged();
    st = status({ path: "F:/prof/log.txt" });
    const fresh = watch.check();
    release();
    await old;
    await fresh;
    expect(scan).toHaveBeenCalledWith("C:/fs25/log.txt");
    expect(scan).toHaveBeenLastCalledWith("F:/prof/log.txt");
    // Only the new profile's run is announced.
    expect(deps.onRun).toHaveBeenCalledTimes(1);
    // The old log's run isn't remembered either: back on that log, it's still news.
    expect(deps.markSeen).not.toHaveBeenCalledWith(NOW - 1000, "C:/fs25/log.txt");
    watch.logChanged();
    st = status({ path: "C:/fs25/log.txt" });
    await watch.check();
    expect(deps.onRun).toHaveBeenCalledTimes(2);
  });

  it("a look queued behind an old one doesn't start once a switch paused looking (#387)", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const scan = vi.fn(async () => {
      await gate;
      return report(true);
    });
    const { deps, watch } = setup({ scan });
    const old = watch.check();
    await Promise.resolve();
    watch.logChanged();
    const queued = watch.check();
    watch.pause();
    release();
    await old;
    await queued;
    // Only the first look read anything, and neither announced (both logs are stale now).
    expect(deps.status).toHaveBeenCalledTimes(1);
    expect(deps.onRun).not.toHaveBeenCalled();
  });

  it("nested pauses resume only when the last one ends (#387)", async () => {
    const { deps, watch } = setup();
    watch.pause(); // a switch
    watch.pause(); // a refresh inside it
    watch.resume();
    await watch.check();
    expect(deps.status).not.toHaveBeenCalled();
    watch.resume();
    await watch.check();
    expect(deps.status).toHaveBeenCalledTimes(1);
  });

  it("remembering the current log skips a log that changed or a switch under way (#387)", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const status = vi.fn(async () => {
      await gate;
      return statusOf();
    });
    const statusOf = () => status_({ lastSeenMs: null });
    const status_ = (o: Partial<LogStatus>) => ({
      path: "F:/prof/log.txt",
      modifiedMs: NOW - 1000,
      gameRunning: false,
      lastSeenMs: NOW - 5000,
      ...o,
    });
    const { deps, watch } = setup({ status });
    const marking = watch.markCurrentSeen();
    watch.pause();
    release();
    await marking;
    expect(deps.markSeen).not.toHaveBeenCalled();
    // And while paused, it doesn't even look.
    await watch.markCurrentSeen();
    expect(status).toHaveBeenCalledTimes(1);
  });

  it("looks nothing up while a mods-folder switch runs, and looks again after (#387)", async () => {
    const { deps, watch } = setup();
    watch.pause();
    await watch.check();
    expect(deps.status).not.toHaveBeenCalled();
    expect(deps.markSeen).not.toHaveBeenCalled();
    watch.resume();
    await watch.check();
    expect(deps.onRun).toHaveBeenCalledTimes(1);
  });

  it("a run being remembered when the log changes was already announced (#387)", async () => {
    let release!: () => void;
    const gate = new Promise<void>((r) => (release = r));
    const markSeen = vi.fn(async (_ms: number, _p: string | null) => gate);
    const { deps, watch } = setup({ markSeen });
    const look = watch.check();
    // Let the scan land; the marker write is pending.
    for (let i = 0; i < 5; i++) await Promise.resolve();
    watch.logChanged();
    release();
    await look;
    // Never "remembered but not announced".
    expect(markSeen).toHaveBeenCalledTimes(1);
    expect(deps.onRun).toHaveBeenCalledTimes(1);
  });

  it("does nothing while the game runs", async () => {
    const { deps, watch } = setup({}, () => status({ gameRunning: true }));
    await watch.check();
    expect(deps.scan).not.toHaveBeenCalled();
    expect(deps.markSeen).not.toHaveBeenCalled();
    expect(deps.onRun).not.toHaveBeenCalled();
  });

  it("does nothing for a log it already looked at", async () => {
    const { deps, watch } = setup({}, () => status({ modifiedMs: 7, lastSeenMs: 7 }));
    await watch.check();
    expect(deps.scan).not.toHaveBeenCalled();
    expect(deps.onRun).not.toHaveBeenCalled();
  });

  it("while a guided diagnosis runs, only remembers the log (the flow reads it itself)", async () => {
    const { deps, watch } = setup({ quiet: () => true });
    await watch.check();
    expect(deps.scan).not.toHaveBeenCalled();
    expect(deps.markSeen).toHaveBeenCalledWith(NOW - 1000, "C:/fs25/log.txt");
    expect(deps.onRun).not.toHaveBeenCalled();
  });

  it("a failed read isn't remembered, so the next look tries again", async () => {
    const { deps, watch } = setup({ scan: vi.fn(async () => Promise.reject(new Error("locked"))) });
    await watch.check();
    expect(deps.markSeen).not.toHaveBeenCalled();
    expect(deps.onRun).not.toHaveBeenCalled();
  });

  it("shares a look that's already running instead of reading twice", async () => {
    const { deps, watch } = setup();
    await Promise.all([watch.check(), watch.check()]);
    expect(deps.scan).toHaveBeenCalledTimes(1);
  });

  it("markCurrentSeen remembers the current log without reading or announcing it", async () => {
    const { deps, watch } = setup();
    await watch.markCurrentSeen();
    expect(deps.markSeen).toHaveBeenCalledWith(NOW - 1000, "C:/fs25/log.txt");
    expect(deps.scan).not.toHaveBeenCalled();
    expect(deps.onRun).not.toHaveBeenCalled();
  });
});

describe("polling after a launch", () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  /** A clock the watch reads, advanced with the fake timers. */
  function clocked(states: LogStatus[]) {
    let t = NOW;
    let i = 0;
    const st = () => states[Math.min(i++, states.length - 1)];
    const s = setup({ now: () => t }, st);
    const advance = async (ms: number) => {
      t += ms;
      await vi.advanceTimersByTimeAsync(ms);
    };
    return { ...s, advance };
  }

  it("polls every 5 s while the game runs, then reads the log once it exits and stops", async () => {
    const { deps, watch, advance } = clocked([
      status({ gameRunning: false }), // Steam hasn't started it yet
      status({ gameRunning: true }),
      status({ gameRunning: true }),
      status({ gameRunning: false, modifiedMs: NOW + 20_000 }), // exited
    ]);
    watch.afterLaunch();
    expect(watch.polling).toBe(true);
    expect(deps.status).not.toHaveBeenCalled(); // first look after one interval
    await advance(POLL_MS);
    await advance(POLL_MS);
    await advance(POLL_MS);
    expect(deps.scan).not.toHaveBeenCalled();
    await advance(POLL_MS);
    expect(deps.scan).toHaveBeenCalledTimes(1);
    expect(deps.onRun).toHaveBeenCalledTimes(1);
    expect(watch.polling).toBe(false);
    // Stopped: no more looks.
    const calls = deps.status.mock.calls.length;
    await advance(POLL_MS * 3);
    expect(deps.status.mock.calls.length).toBe(calls);
  });

  it("gives up when the game never shows up", async () => {
    const { deps, watch, advance } = clocked([status({ gameRunning: false, modifiedMs: NOW - 5000 })]);
    watch.afterLaunch();
    await advance(LAUNCH_GRACE_MS + POLL_MS);
    expect(watch.polling).toBe(false);
    const calls = deps.status.mock.calls.length;
    await advance(POLL_MS * 3);
    expect(deps.status.mock.calls.length).toBe(calls);
    expect(deps.scan).not.toHaveBeenCalled();
  });

  it("a game that came and went between looks is still read when the wait ends", async () => {
    const { deps, watch, advance } = clocked([status({ gameRunning: false, modifiedMs: NOW + 60_000 })]);
    watch.afterLaunch();
    await advance(LAUNCH_GRACE_MS + POLL_MS);
    expect(watch.polling).toBe(false);
    expect(deps.scan).toHaveBeenCalledTimes(1);
    expect(deps.onRun).toHaveBeenCalledTimes(1);
  });

  it("one failed look during a long game doesn't stop the wait for its exit", async () => {
    const states: (LogStatus | "fail")[] = [
      status({ gameRunning: true }),
      ...Array(40).fill(status({ gameRunning: true })),
      "fail",
      status({ gameRunning: false, modifiedMs: NOW + 300_000 }),
    ];
    let t = NOW;
    let i = 0;
    const s = setup({ now: () => t }, () => {
      const v = states[Math.min(i++, states.length - 1)];
      if (v === "fail") throw new Error("log_status failed");
      return v;
    });
    s.watch.afterLaunch();
    for (let k = 0; k < 43; k++) {
      t += POLL_MS;
      await vi.advanceTimersByTimeAsync(POLL_MS);
    }
    expect(s.deps.scan).toHaveBeenCalledTimes(1);
    expect(s.watch.polling).toBe(false);
  });

  it("stop() ends polling, even mid-look", async () => {
    const { deps, watch, advance } = clocked([status({ gameRunning: true })]);
    watch.afterLaunch();
    await advance(POLL_MS);
    watch.stop();
    expect(watch.polling).toBe(false);
    const calls = deps.status.mock.calls.length;
    await advance(POLL_MS * 4);
    expect(deps.status.mock.calls.length).toBe(calls);
  });

  it("a focus check that finished first doesn't make the poll announce the same run again", async () => {
    // Before the stored last-seen value catches up, a poll status still carries the old one.
    let t = NOW;
    let tickStatus: (s: LogStatus) => void = () => {};
    const exited = status({ gameRunning: false, modifiedMs: NOW + 9_000, lastSeenMs: NOW - 5000 });
    let calls = 0;
    const s = setup({ now: () => t }, () => exited);
    s.deps.status.mockImplementation(async () => {
      calls++;
      if (calls === 1) return status({ gameRunning: true }); // first tick: running
      if (calls === 2) return new Promise<LogStatus>((r) => (tickStatus = r)); // slow tick
      return exited; // the focus check, and any later look
    });
    s.watch.afterLaunch();
    t += POLL_MS;
    await vi.advanceTimersByTimeAsync(POLL_MS); // tick 1: running
    t += POLL_MS;
    await vi.advanceTimersByTimeAsync(POLL_MS); // tick 2 waits on its status
    await s.watch.check(); // focus: reads and announces the exit
    expect(s.deps.onRun).toHaveBeenCalledTimes(1);
    tickStatus(exited); // the slow tick sees the exit with the old last-seen value
    await vi.advanceTimersByTimeAsync(0);
    expect(s.deps.scan).toHaveBeenCalledTimes(1);
    expect(s.deps.onRun).toHaveBeenCalledTimes(1);
    expect(s.watch.polling).toBe(false);
  });

  it("an exit seen while a focus check (taken while the game ran) is in flight is still read", async () => {
    let t = NOW;
    let focusStatus: (s: LogStatus) => void = () => {};
    let calls = 0;
    const s = setup({ now: () => t });
    s.deps.status.mockImplementation(async () => {
      calls++;
      if (calls === 1) return status({ gameRunning: true }); // tick 1
      if (calls === 2) return new Promise<LogStatus>((r) => (focusStatus = r)); // focus, slow
      if (calls === 3) return status({ gameRunning: false, modifiedMs: NOW + 9_000 }); // tick 2: exited
      return status({ gameRunning: false, modifiedMs: NOW + 9_000 }); // fresh look after the exit
    });
    s.watch.afterLaunch();
    t += POLL_MS;
    await vi.advanceTimersByTimeAsync(POLL_MS);
    const focus = s.watch.check();
    t += POLL_MS;
    await vi.advanceTimersByTimeAsync(POLL_MS); // tick 2 sees the exit, waits for the focus look
    focusStatus(status({ gameRunning: true })); // the focus look saw the game running: skip
    await focus;
    await vi.advanceTimersByTimeAsync(0);
    expect(s.deps.scan).toHaveBeenCalledTimes(1);
    expect(s.deps.onRun).toHaveBeenCalledTimes(1);
  });

  it("a second launch restarts the wait instead of running two pollers", async () => {
    const { deps, watch, advance } = clocked([status({ gameRunning: true })]);
    watch.afterLaunch();
    watch.afterLaunch();
    await advance(POLL_MS);
    expect(deps.status).toHaveBeenCalledTimes(1);
  });
});
