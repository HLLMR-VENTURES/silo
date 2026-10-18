// Keyboard model shared by the library's VirtualList and VirtualGrid: one tab stop, arrows move
// the focused item, Enter opens, Space toggles, Shift+arrows extend the selection. Pure, so the
// key table unit-tests without a DOM.
//   keyAction({ key: "ArrowDown" }, { index: 3, count: 10, columns: 1, pageRows: 5, onItem: true })
//     -> { kind: "move", to: 4, extend: false }

/** What a selection gesture asks for; the selection model (Task 2 of #242) interprets it. */
export interface SelectIntent {
  /** Shift was held (extend from the anchor). */
  shift: boolean;
  /** Ctrl or Cmd was held (toggle this one). */
  ctrl: boolean;
  /** The gesture came from Shift+arrow: select the range from the anchor to this item. */
  range?: boolean;
}

export type KeyAction =
  | { kind: "move"; to: number; extend: boolean }
  | { kind: "open" }
  | { kind: "toggle" }
  | { kind: "select" }
  | { kind: "menu" };

/** The parts of a KeyboardEvent the table reads. */
export interface KeyLike {
  key: string;
  shiftKey?: boolean;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

export interface KeyContext {
  /** The focused item's index. */
  index: number;
  count: number;
  /** Items per row: 1 for a list. */
  columns: number;
  /** Whole rows that fit the viewport, for PageUp/PageDown (at least 1). */
  pageRows: number;
  /** The item itself has focus (not a control inside it): only then Enter and Space act. */
  onItem: boolean;
  /** ←/→ move by one (grids). Lists leave them alone. */
  twoD?: boolean;
}

const clamp = (n: number, max: number) => Math.max(0, Math.min(n, max));

/**
 * Map a key press to a list action.
 * @param e the key (and modifiers)
 * @param ctx where focus is and the list's shape
 * @returns the action, or null when the key isn't the list's (let it through)
 */
export function keyAction(e: KeyLike, ctx: KeyContext): KeyAction | null {
  const { index, count, onItem } = ctx;
  if (count === 0 || e.altKey) return null;
  const cols = Math.max(1, Math.floor(ctx.columns));
  const last = count - 1;
  const mod = !!(e.ctrlKey || e.metaKey);
  const shift = !!e.shiftKey;

  if ((e.key === "F10" && shift && !mod) || e.key === "ContextMenu") return { kind: "menu" };

  const move = (to: number, extend = false): KeyAction => ({ kind: "move", to: clamp(to, last), extend });
  switch (e.key) {
    case "ArrowDown":
      return mod ? null : move(index + cols, shift);
    case "ArrowUp":
      return mod ? null : move(index - cols, shift);
    case "ArrowRight":
      return ctx.twoD && !mod ? move(index + 1, shift) : null;
    case "ArrowLeft":
      return ctx.twoD && !mod ? move(index - 1, shift) : null;
    case "Home":
      return shift ? null : move(0);
    case "End":
      return shift ? null : move(last);
    case "PageDown":
      return shift || mod ? null : move(index + Math.max(1, ctx.pageRows) * cols);
    case "PageUp":
      return shift || mod ? null : move(index - Math.max(1, ctx.pageRows) * cols);
    case "Enter":
      return onItem && !shift && !mod ? { kind: "open" } : null;
    case " ":
    case "Spacebar":
      if (!onItem) return null;
      if (mod && !shift) return { kind: "select" };
      return !mod && !shift ? { kind: "toggle" } : null;
  }
  return null;
}

/**
 * The scrollTop that brings item row `row` fully into view (with `pad` px to spare), or null
 * when it already is.
 * @param row the item's row index
 * @param rowStride px per row (item height plus gap)
 * @param itemHeight px of the item itself
 * @param scrollTop the viewport's current scrollTop
 * @param viewportH the viewport's height
 * @param pad breathing room kept above/below, px
 */
export function scrollToReveal(
  row: number,
  rowStride: number,
  itemHeight: number,
  scrollTop: number,
  viewportH: number,
  pad = 8,
): number | null {
  const top = row * rowStride;
  const bottom = top + itemHeight;
  const up = Math.max(0, top - pad);
  if (up < scrollTop) return up;
  const down = bottom + pad - viewportH;
  if (down > scrollTop) return down;
  return null;
}

/** Where an item's menu opens: under its ⋯ button or row (anchor), or at the pointer (point). */
export interface MenuTarget {
  anchor?: HTMLElement | null;
  point?: { x: number; y: number } | null;
}
