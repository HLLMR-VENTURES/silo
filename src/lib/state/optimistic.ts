// Optimistic edits to a keyed map (#358). The UI shows a change before its save returns. When the
// save fails, the entry goes back to what the store holds, but only when no newer edit to that
// key was made meanwhile (that edit has its own save, and its own failure). Writes to one key are
// queued (serial), so when the newest edit's save settles, every earlier one has too, and the
// ledger's stored value is the truth: two failed saves in a row end on the saved value, not on
// the first failed one.
//   const ledger = new SaveLedger<Row>();
//   ledger.load(rowsFromDb);
//   const t = ledger.begin(key); map = withEntry(map, key, next);
//   try { await save(next); ledger.saved(key, next); }
//   catch { const back = ledger.failed(key, t); if (back) map = withEntry(map, key, back.value); }

/** What each key's store holds, and which edit of it is the newest. */
export class SaveLedger<T> {
  #stored = new Map<string, T>();
  #latest = new Map<string, number>();
  #seq = 0;

  /**
   * What the store holds, as read from it (a key that isn't there has no entry).
   * @param entries key and value of every stored entry
   */
  load(entries: Iterable<readonly [string, T]>): void {
    this.#stored = new Map(entries);
  }

  /**
   * An edit to `key` is shown now and its save is about to start.
   * @returns the edit's token, for `failed`
   */
  begin(key: string): number {
    const token = ++this.#seq;
    this.#latest.set(key, token);
    return token;
  }

  /**
   * An edit's save worked: the store holds `value` now.
   * @param value what was written (`undefined`: the entry was removed)
   */
  saved(key: string, value: T | undefined): void {
    if (value === undefined) this.#stored.delete(key);
    else this.#stored.set(key, value);
  }

  /**
   * An edit's save failed.
   * @param token what `begin` returned for it
   * @returns what to show for `key` now (`value` undefined: no entry), or null when a newer edit
   *   was made meanwhile (leave it: it settles on its own)
   */
  failed(key: string, token: number): { value: T | undefined } | null {
    if (this.#latest.get(key) !== token) return null;
    return { value: this.#stored.get(key) };
  }
}

/** A copy of `map` with `key` set to `value`, or without `key` when `value` is undefined. */
export function withEntry<T>(map: Readonly<Record<string, T>>, key: string, value: T | undefined): Record<string, T> {
  const next = { ...map };
  if (value === undefined) delete next[key];
  else next[key] = value;
  return next;
}
