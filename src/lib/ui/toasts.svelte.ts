// Transient notices, rendered by <ToastRegion /> (mounted once in App.svelte):
//   toast({ message: "Mod set saved", tone: "success" });
//   const t = toast({ message: "Moved 12 mods", action: { label: "Undo", run: undo } });
//   t.dismiss();
// Toasts never take focus, and their buttons are inert while a dialog is open. Their timer pauses
// while hovered or focused and while the window is in the background, so nobody misses one by
// alt-tabbing. At most MAX_VISIBLE_TOASTS show at once; the rest wait their turn (FIFO). A toast
// with the same `dedupeKey` as one already waiting or showing bumps its count ("×3") instead.
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

/** Most toasts on screen at once. */
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

/** The toasts on screen now (the first MAX_VISIBLE_TOASTS). Reactive. */
export function visibleToasts(): ToastItem[] {
  return toasts.slice(0, MAX_VISIBLE_TOASTS);
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
      bumpActivity(same.activityId);
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
    duration: opts.duration ?? (opts.action ? 10_000 : 6_000),
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
