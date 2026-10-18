// This session's activity log: every toast shown, every notice cleared or dismissed, every
// finished operation. Shown by ActivityPanel. Kept in its own module so the toast service, the
// notice store and the ops store can all write to it without importing each other.
// Views import it through ./notify.svelte (which re-exports it).

export type ActivityKind = "toast" | "notice" | "op";
export type ActivityLevel = "error" | "warning" | "info" | "success";

export interface ActivityEntry {
  id: number;
  /** When it happened, epoch ms. */
  at: number;
  kind: ActivityKind;
  level: ActivityLevel;
  message: string;
  /** Text behind "Copy details" (formatDetail output), for errors. */
  detail?: string;
  /** How many times it happened in a row (duplicate toasts bump this). */
  count: number;
  /** For finished ops: how long they ran. */
  durationMs?: number;
}

/** Most entries kept; older ones fall off the end. */
export const ACTIVITY_LIMIT = 100;

let nextId = 1;
/** Newest first. Reactive; change it only through the functions below. */
export const activity: ActivityEntry[] = $state([]);
let seenUpTo = $state(0);

/**
 * Add an entry at the top.
 * @returns the new entry's id (for bumpActivity)
 */
export function pushActivity(entry: Omit<ActivityEntry, "id" | "at" | "count"> & { at?: number }): number {
  const id = nextId++;
  activity.unshift({ ...entry, id, at: entry.at ?? Date.now(), count: 1 });
  if (activity.length > ACTIVITY_LIMIT) activity.length = ACTIVITY_LIMIT;
  return id;
}

/** Count one more occurrence of an entry and move it to the top (no-op if it fell off). */
export function bumpActivity(id: number): void {
  const i = activity.findIndex((a) => a.id === id);
  if (i === -1) return;
  const [e] = activity.splice(i, 1);
  activity.unshift({ ...e, count: e.count + 1, at: Date.now() });
}

/** Empty the log (the panel's "Clear"). */
export function clearActivity(): void {
  activity.length = 0;
}

/** Mark everything logged so far as seen (the panel was opened). */
export function markActivitySeen(): void {
  seenUpTo = nextId - 1;
}

/** Error entries logged since the panel was last opened. Reactive. */
export function unseenErrorCount(): number {
  return activity.filter((a) => a.level === "error" && a.id > seenUpTo).length;
}
