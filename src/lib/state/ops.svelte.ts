// Running operations: what Silo is doing right now, for the activity button, progress and
// "Silo is busy" reasons on disabled controls.
//   const op = start("Moving mods", { exclusive: "library-write", progress: true });
//   op.progress(3, 12);
//   op.end();                 // idempotent; ends only this op
//   blocked("library-write")  // { blocked: true, reason: "Silo is moving mods." }
//   tryStart("Scanning", { exclusive: "library-write" }) // null while the class is held
//   start("Moving your library", { exclusive: "library-write", background: true, quiet: true });
//   blockedForeground("library-write") // ignores background ops (navigation stays usable)
//   onIdle("library-write", rescan)     // once no foreground op holds the class
// `quiet` ops (scans, the archive move) stay out of the status line and the activity log.

import { lowerFirst } from "../errorCopy";
import { pushActivity } from "./activity.svelte";

/** Ops that must not overlap. Today only writes to the library. */
export type OpClass = "library-write";

export interface OpProgress {
  done: number;
  total: number;
  label?: string;
}

export interface RunningOp {
  id: number;
  label: string;
  exclusive?: OpClass;
  /** Set when started with `progress: true` or once progress() is called; null until then. */
  progress: OpProgress | null;
  /** Shows a progress bar (indeterminate until the first progress() call). */
  hasProgress: boolean;
  /** Holds its class for writes, but doesn't lock navigation or skip scans (the archive move). */
  background: boolean;
  /** Its progress shows elsewhere (scan bar, a notice): no status line, no activity entry. */
  quiet: boolean;
  startedAt: number;
}

export interface OpToken {
  id: number;
  progress(done: number, total: number, label?: string): void;
  setLabel(label: string): void;
  /** Finish the op. Safe to call more than once. */
  end(): void;
}

export interface StartOptions {
  exclusive?: OpClass;
  progress?: boolean;
  /** See RunningOp.background. */
  background?: boolean;
  /** See RunningOp.quiet. */
  quiet?: boolean;
}

let nextId = 1;
/** Oldest first. Reactive; change it only through tokens. */
export const running: RunningOp[] = $state([]);

function find(id: number): RunningOp | undefined {
  return running.find((o) => o.id === id);
}

/**
 * Start an op.
 * @param label what Silo is doing, as a phrase that fits "Silo is ...": "Moving mods"
 * @returns a token to report progress and end it
 */
export function start(label: string, opts: StartOptions = {}): OpToken {
  const id = nextId++;
  running.push({
    id,
    label,
    exclusive: opts.exclusive,
    progress: null,
    hasProgress: opts.progress ?? false,
    background: opts.background ?? false,
    quiet: opts.quiet ?? false,
    startedAt: Date.now(),
  });
  let ended = false;
  return {
    id,
    progress(done, total, progressLabel) {
      const op = find(id);
      if (!op) return;
      op.hasProgress = true;
      op.progress = { done, total, label: progressLabel };
    },
    setLabel(next) {
      const op = find(id);
      if (op) op.label = next;
    },
    end() {
      if (ended) return;
      ended = true;
      const i = running.findIndex((o) => o.id === id);
      if (i === -1) return;
      const [op] = running.splice(i, 1);
      const now = Date.now();
      if (!op.quiet && !op.background) {
        pushActivity({ kind: "op", level: "info", message: op.label, durationMs: now - op.startedAt, at: now });
      }
      if (op.exclusive) fireIdle(op.exclusive);
    },
  };
}

/**
 * Start an op unless its class is already held.
 * @returns the token, or null when another op holds `opts.exclusive` (scans use this to skip)
 */
export function tryStart(label: string, opts: StartOptions = {}): OpToken | null {
  // Any holder refuses, background included: two writes never overlap.
  if (opts.exclusive && blocked(opts.exclusive).blocked) return null;
  return start(label, opts);
}

function holder(cls: OpClass, foregroundOnly: boolean): { blocked: boolean; reason: string } {
  for (let i = running.length - 1; i >= 0; i--) {
    const op = running[i];
    if (op.exclusive !== cls || (foregroundOnly && op.background)) continue;
    return { blocked: true, reason: `Silo is ${lowerFirst(op.label)}.` };
  }
  return { blocked: false, reason: "" };
}

/**
 * Whether an op class is held by any op (background ones included): a write action must refuse.
 * @returns `{ blocked, reason }`; reason is "" when not blocked
 */
export function blocked(cls: OpClass): { blocked: boolean; reason: string } {
  return holder(cls, false);
}

/**
 * Whether a foreground op holds the class: what navigation, read-only panels and scans check.
 * Background ops (the archive move) don't count.
 * @returns `{ blocked, reason }`; reason is "" when not blocked
 */
export function blockedForeground(cls: OpClass): { blocked: boolean; reason: string } {
  return holder(cls, true);
}

const idleWaiters = new Map<OpClass, (() => void)[]>();

function fireIdle(cls: OpClass): void {
  if (holder(cls, true).blocked) return;
  const fns = idleWaiters.get(cls);
  if (!fns?.length) return;
  idleWaiters.delete(cls);
  // After the ending op's own follow-up (usually its rescan) has started.
  queueMicrotask(() => {
    for (const fn of fns) {
      try {
        fn();
      } catch (e) {
        console.error("[Silo] onIdle callback failed", e);
      }
    }
  });
}

/**
 * Run `fn` once no foreground op holds `cls`: on a microtask after the last such op ends, or on
 * the next microtask when the class is already free. One-shot.
 * @param cls the op class to wait for
 * @param fn what to run
 */
export function onIdle(cls: OpClass, fn: () => void): void {
  const fns = idleWaiters.get(cls) ?? [];
  fns.push(fn);
  idleWaiters.set(cls, fns);
  fireIdle(cls);
}

/** Tests only: forget pending onIdle callbacks. */
export function resetIdleWaiters(): void {
  idleWaiters.clear();
}

/** The op to show: the most recent exclusive one, else the most recent; null when idle. Reactive. */
export function current(): RunningOp | null {
  for (let i = running.length - 1; i >= 0; i--) if (running[i].exclusive) return running[i];
  return running.at(-1) ?? null;
}

/** The op the status line names: like current(), but leaving out `quiet` ops. Reactive. */
export function statusLineOp(): RunningOp | null {
  const shown = running.filter((o) => !o.quiet);
  return shown.findLast((o) => o.exclusive) ?? shown.at(-1) ?? null;
}
