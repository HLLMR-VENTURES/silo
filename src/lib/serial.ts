// Per-key serial queue for whole-row writes. Backend commands run on a worker pool, so two
// fire-and-forget writes to the same row can land out of order and the older one wins. Calls
// that share a key run one after another, in call order; different keys don't wait on each other.

const queue = new Map<string, Promise<unknown>>();

/**
 * Run `fn` after every earlier call with the same `key` has settled.
 * @param key what the write touches, e.g. `curation:<techName>`
 * @param fn the write to run
 * @returns `fn`'s own result (a failure is still reported to this caller, but never blocks the next call)
 */
export function serial<T>(key: string, fn: () => Promise<T>): Promise<T> {
  const p = (queue.get(key) ?? Promise.resolve()).catch(() => {}).then(fn);
  queue.set(key, p);
  const forget = () => {
    if (queue.get(key) === p) queue.delete(key);
  };
  p.then(forget, forget);
  return p;
}

/** True while a call with `key` is queued or running (for tests). */
export function serialPending(key: string): boolean {
  return queue.has(key);
}
