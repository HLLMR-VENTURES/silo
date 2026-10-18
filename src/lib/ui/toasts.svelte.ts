// Transient notices, rendered by <ToastRegion /> (mounted once in App.svelte):
//   toast({ message: "Mod set saved", tone: "success" });
//   const t = toast({ message: "Moved 12 mods", action: { label: "Undo", run: undo } });
//   t.dismiss();
// Toasts never take focus, and their buttons are inert while a dialog is open. Their timer pauses
// while hovered or focused and while the window is in the background, so nobody misses one by
// alt-tabbing.

export type ToastTone = "neutral" | "success" | "warn" | "danger";

export interface ToastAction {
  label: string;
  /** The button's accessible name. Default: "<label>: <message>" ("Undo: Switched to Harvest."). */
  ariaLabel?: string;
  run: () => void | Promise<void>;
}

export interface ToastOptions {
  message: string;
  tone?: ToastTone;
  action?: ToastAction;
  /** Milliseconds on screen (time paused doesn't count). Default 6000, or 10000 with an action. */
  duration?: number;
}

export interface ToastItem {
  id: number;
  message: string;
  tone: ToastTone;
  action?: ToastAction;
  duration: number;
}

export interface ToastHandle {
  dismiss(): void;
}

let nextId = 1;

/** Toasts on screen, oldest first. Reactive; mutate only through `toast()` / `dismissToast()`. */
export const toasts: ToastItem[] = $state([]);

/**
 * Show a toast.
 * @param opts message, tone, optional action and duration
 * @returns a handle to dismiss it early
 */
export function toast(opts: ToastOptions): ToastHandle {
  const id = nextId++;
  toasts.push({
    id,
    message: opts.message,
    tone: opts.tone ?? "neutral",
    action: opts.action,
    duration: opts.duration ?? (opts.action ? 10_000 : 6_000),
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
