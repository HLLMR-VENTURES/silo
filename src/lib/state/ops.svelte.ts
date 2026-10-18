// Running operations: what Silo is doing right now, for the activity button, progress and
// "Silo is busy" reasons on disabled controls.
//   const op = start("Moving mods", { exclusive: "library-write", progress: true });
//   op.progress(3, 12);
//   op.end();                 // idempotent; ends only this op
//   blocked("library-write")  // { blocked: true, reason: "Silo is moving mods." }
//   tryStart("Scanning", { exclusive: "library-write" }) // null while the class is held

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
      pushActivity({ kind: "op", level: "info", message: op.label, durationMs: now - op.startedAt, at: now });
    },
  };
}

/**
 * Start an op unless its class is already held.
 * @returns the token, or null when another op holds `opts.exclusive` (scans use this to skip)
 */
export function tryStart(label: string, opts: StartOptions = {}): OpToken | null {
  if (opts.exclusive && blocked(opts.exclusive).blocked) return null;
  return start(label, opts);
}

/**
 * Whether an op class is held, and the reason to show on a disabled control.
 * @returns `{ blocked, reason }`; reason is "" when not blocked
 */
export function blocked(cls: OpClass): { blocked: boolean; reason: string } {
  for (let i = running.length - 1; i >= 0; i--) {
    const op = running[i];
    if (op.exclusive === cls) return { blocked: true, reason: `Silo is ${lowerFirst(op.label)}.` };
  }
  return { blocked: false, reason: "" };
}

/** The op to show: the most recent exclusive one, else the most recent; null when idle. Reactive. */
export function current(): RunningOp | null {
  for (let i = running.length - 1; i >= 0; i--) if (running[i].exclusive) return running[i];
  return running.at(-1) ?? null;
}
