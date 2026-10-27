// Transient notices, rendered by <ToastRegion /> (mounted once in App.svelte):
//   toast({ message: "Mod set saved", tone: "success" });
//   const t = toast({ message: "Moved 12 mods", action: { label: "Undo", run: undo } });
//   toast({ message: "Removed 2 mods from 3 sets.", secondaryAction: { label: "Park them too", run }, action: undoAction });
//   t.dismiss();
// Toasts never take focus, and their buttons are inert while a dialog is open. Their timer pauses
// while hovered or focused and while the window is in the background, so nobody misses one by
// alt-tabbing. At most MAX_VISIBLE_TOASTS errors show at once, plus at least one other toast (the
// newest always shows); the rest wait their turn (FIFO). A toast with the same `dedupeKey` as one
// already waiting or showing bumps its count ("×3") instead.
// Every toast is also written to the activity log. Errors go through `error()` in
// src/lib/state/notify.svelte.ts, which sets `sticky`, `detail` and `dedupeKey`.

import { bumpActivity, pushActivity, type ActivityLevel } from "../state/activity.svelte";

export type ToastTone = "neutral" | "success" | "warn" | "danger";

export interface ToastAction {
  label: string;
  /** The button's accessible name. Default: "<label>: <message>" ("Undo: Switched to Harvest."). */
  ariaLabel?: string;
  run: () => void | Promise<void>;
}

export interface ToastOptions {
  message: string;
  /** A second line under the message (an error's own words, a list of reasons). */
  body?: string;
  tone?: ToastTone;
  action?: ToastAction;
  /** A second action, shown before `action` ("Park them too" beside "Undo"). */
  secondaryAction?: ToastAction;
  /** Milliseconds on screen (time paused doesn't count). Default 6000, or 10000 with an action. */
  duration?: number;
  /** Stays until dismissed (errors). */
  sticky?: boolean;
  /** Technical text; adds a "Copy details" button. */
  detail?: string;
  /** Toasts with the same key merge into one with a count. */
  dedupeKey?: string;
}

export interface ToastItem {
  id: number;
  message: string;
  body?: string;
  tone: ToastTone;
  action?: ToastAction;
  secondaryAction?: ToastAction;
  duration: number;
  sticky: boolean;
  detail?: string;
  dedupeKey?: string;
  /** How many times it was raised while on screen or waiting. */
  count: number;
  /** Its entry in the activity log. */
  activityId: number;
}

export interface ToastHandle {
  dismiss(): void;
}

/**
 * Most sticky (error) toasts on screen at once. Non-sticky toasts fill the slots errors leave
 * free, and always get at least one, so up to MAX_VISIBLE_TOASTS + 1 can show.
 */
export const MAX_VISIBLE_TOASTS = 3;

const LEVEL: Record<ToastTone, ActivityLevel> = {
  neutral: "info",
  success: "success",
  warn: "warning",
  danger: "error",
};

let nextId = 1;

/** Toasts on screen or waiting, oldest first. Reactive; mutate only through `toast()` / `dismissToast()`. */
export const toasts: ToastItem[] = $state([]);

/**
 * The toasts on screen now, oldest first: the first MAX_VISIBLE_TOASTS sticky ones, and
 * non-sticky ones in the slots left (at least one). The newest non-sticky toast is always among
 * them, so errors on screen never hide an Undo. Reactive.
 */
export function visibleToasts(): ToastItem[] {
  const sticky = toasts.filter((t) => t.sticky).slice(0, MAX_VISIBLE_TOASTS);
  const plain = toasts.filter((t) => !t.sticky);
  const slots = Math.max(1, MAX_VISIBLE_TOASTS - sticky.length);
  let shownPlain = plain.slice(0, slots);
  const newest = plain.at(-1);
  if (newest && !shownPlain.includes(newest)) shownPlain = [...shownPlain.slice(0, slots - 1), newest];
  const shown = new Set([...sticky, ...shownPlain]);
  return toasts.filter((t) => shown.has(t));
}

/**
 * Show a toast.
 * @param opts message, tone, optional action and duration
 * @returns a handle to dismiss it early
 */
export function toast(opts: ToastOptions): ToastHandle {
  if (opts.dedupeKey !== undefined) {
    const same = toasts.find((t) => t.dedupeKey === opts.dedupeKey);
    if (same) {
      same.count += 1;
      same.activityId = bumpActivity(same.activityId);
      const sameId = same.id;
      return { dismiss: () => dismissToast(sameId) };
    }
  }
  const id = nextId++;
  const tone = opts.tone ?? "neutral";
  const activityId = pushActivity({
    kind: "toast",
    level: LEVEL[tone],
    message: opts.body ? `${opts.message} ${opts.body}` : opts.message,
    detail: opts.detail,
  });
  toasts.push({
    id,
    message: opts.message,
    body: opts.body,
    tone,
    action: opts.action,
    secondaryAction: opts.secondaryAction,
    duration: opts.duration ?? (opts.action || opts.secondaryAction ? 10_000 : 6_000),
    sticky: opts.sticky ?? false,
    detail: opts.detail,
    dedupeKey: opts.dedupeKey,
    count: 1,
    activityId,
  });
  return { dismiss: () => dismissToast(id) };
}

/**
 * Remove a toast (no-op if it's already gone).
 * @param id the toast's id
 */
export function dismissToast(id: number): void {
  const i = toasts.findIndex((t) => t.id === id);
  if (i !== -1) toasts.splice(i, 1);
}
