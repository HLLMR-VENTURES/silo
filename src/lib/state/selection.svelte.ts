// The Library's multi-select: which rows are selected (by key, the mod's path), and the anchor a
// Shift gesture extends from. Reactive (one $state.raw snapshot, replaced on every change), with
// no DOM or app state, so it unit-tests on its own.
//   const sel = new Selection();
//   sel.apply(key, { shift: false, ctrl: false }, orderedKeys)  // checkbox click: toggle
//   sel.apply(key, { shift: true, ctrl: false }, orderedKeys)   // Shift+click: range from anchor
//   sel.prune(visibleKeys)                                      // filters changed
// Gestures: a plain or Ctrl click (and Ctrl+Space) toggles one row and makes it the anchor;
// Shift+click and Shift+↑/↓ select from the anchor to the row. A range is added to what was
// selected when the anchor was set, so moving back with Shift+↑ shrinks it again.

import type { SelectIntent } from "../components/library/roving";

interface SelState {
  selected: ReadonlySet<string>;
  anchor: string | null;
  /** The selection when the anchor was set: a range is added to this. */
  base: ReadonlySet<string>;
}

const EMPTY: SelState = { selected: new Set(), anchor: null, base: new Set() };

export class Selection {
  #s = $state.raw<SelState>(EMPTY);

  /** The selected keys (a snapshot; replaced, never mutated). */
  get selected(): ReadonlySet<string> {
    return this.#s.selected;
  }
  get size(): number {
    return this.#s.selected.size;
  }
  /** Where a Shift gesture extends from, or null. */
  get anchor(): string | null {
    return this.#s.anchor;
  }
  has(key: string): boolean {
    return this.#s.selected.has(key);
  }

  /**
   * Select or unselect one key; it becomes the anchor.
   * @param key the row's key
   */
  toggle(key: string): void {
    const next = new Set(this.#s.selected);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    this.#s = { selected: next, anchor: key, base: next };
  }

  /**
   * Select every key from the anchor to `key` (inclusive) in list order, on top of what was
   * selected when the anchor was set. With no anchor in the list, `from` (or `key` itself)
   * becomes the anchor.
   * @param key the row the gesture landed on
   * @param ordered every key in display order
   * @param from the row focus moved from (Shift+arrow), used when there's no anchor yet
   */
  range(key: string, ordered: readonly string[], from?: string): void {
    const to = ordered.indexOf(key);
    if (to < 0) return;
    let { anchor, base } = this.#s;
    if (anchor === null || !ordered.includes(anchor)) {
      anchor = from !== undefined && ordered.includes(from) ? from : key;
      base = this.#s.selected;
    }
    const a = ordered.indexOf(anchor);
    const [lo, hi] = a <= to ? [a, to] : [to, a];
    const next = new Set(base);
    for (let i = lo; i <= hi; i++) next.add(ordered[i]);
    this.#s = { selected: next, anchor, base };
  }

  /**
   * Apply a row's selection gesture.
   * @param key the row's key
   * @param intent Shift → range, otherwise (plain or Ctrl) → toggle
   * @param ordered every key in display order
   */
  apply(key: string, intent: SelectIntent, ordered: readonly string[]): void {
    if (intent.shift) this.range(key, ordered, intent.from);
    else this.toggle(key);
  }

  /** Select exactly these keys (Select all shown); no anchor. */
  selectAll(keys: Iterable<string>): void {
    const next = new Set(keys);
    this.#s = { selected: next, anchor: null, base: next };
  }

  clear(): void {
    if (this.#s.selected.size === 0 && this.#s.anchor === null) return;
    this.#s = EMPTY;
  }

  /**
   * Drop keys that aren't in the list any more (filters changed, a rescan removed a mod).
   * Changes nothing (no new snapshot) when every selected key is still there.
   * @param keys the keys in the list now
   */
  prune(keys: Iterable<string>): void {
    const live = keys instanceof Set ? (keys as ReadonlySet<string>) : new Set(keys);
    const { selected, anchor, base } = this.#s;
    const anchorGone = anchor !== null && !live.has(anchor);
    let gone = anchorGone;
    for (const k of selected) if (!live.has(k)) gone = true;
    if (!gone) return;
    const keep = (s: ReadonlySet<string>) => new Set([...s].filter((k) => live.has(k)));
    this.#s = { selected: keep(selected), anchor: anchorGone ? null : anchor, base: keep(base) };
  }
}
