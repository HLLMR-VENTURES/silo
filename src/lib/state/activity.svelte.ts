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

/** Most entries kept; past it the oldest non-error entry falls off first, so errors survive. */
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
  trim();
  return id;
}

/** Drop entries past ACTIVITY_LIMIT: the oldest non-error ones first, then the oldest errors. */
function trim(): void {
  let over = activity.length - ACTIVITY_LIMIT;
  for (let i = activity.length - 1; i >= 0 && over > 0; i--) {
    if (activity[i].level !== "error") {
      activity.splice(i, 1);
      over--;
    }
  }
  if (over > 0) activity.length = ACTIVITY_LIMIT;
}

/**
 * Count one more occurrence of an entry and move it to the top. It gets a new id, so a repeated
 * error counts as unseen again (the bell lights).
 * @returns the entry's new id, or the old one if it fell off the log
 */
export function bumpActivity(id: number): number {
  const i = activity.findIndex((a) => a.id === id);
  if (i === -1) return id;
  const [e] = activity.splice(i, 1);
  const fresh = nextId++;
  activity.unshift({ ...e, id: fresh, count: e.count + 1, at: Date.now() });
  return fresh;
}

/** Empty the log (the panel's "Clear"). */
export function clearActivity(): void {
  activity.length = 0;
}

/** Mark everything logged so far as seen (the panel was opened). */
export function markActivitySeen(): void {
  seenUpTo = nextId - 1;
}

/** Error entries logged since the panel was last opened. Reactive. A notice is logged when it
 *  goes away (its problem is over), so a blocking notice's entry isn't news for the bell. */
export function unseenErrorCount(): number {
  return activity.filter((a) => a.level === "error" && a.kind !== "notice" && a.id > seenUpTo).length;
}
