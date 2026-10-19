<script lang="ts" module>
  /** What the item snippet gets besides the item. */
  export interface ItemContext {
    index: number;
    /** This item holds the list's roving focus (the one tab stop). */
    focused: boolean;
  }
</script>

<script lang="ts" generics="T">
  // Virtualized, keyed collection with one tab stop: the library's card grid, and (one column, via
  // VirtualList) its list.
  //   <VirtualGrid items={mods} getKey={(m) => m.path} itemHeight={176} label="Mods"
  //     onopen={open} ontoggle={toggle} onselect={select} onmenu={menu}>
  //     {#snippet item(mod, ctx)}<ModCard {mod} />{/snippet}
  //   </VirtualGrid>
  // Items are keyed by getKey, so scrolling never remounts an item that stays on screen. The
  // focused item stays rendered even when scrolled away, so focus is never lost to <body>.
  // Keys: arrows move (←/→ by one, ↑/↓ by a row; ←/→ only with `twoD`), Home/End, PageUp/Down;
  // Enter opens and Space toggles (only when the item itself has focus, not a control in it);
  // Shift+arrows and Ctrl+Space select; Shift+F10 or the Menu key opens the item's menu.
  // `focusKey(key)` moves focus to an item by key (the drawer hands focus back with it);
  // `showKey(key)` makes it the tab stop and scrolls to it without taking focus; `currentKey()`
  // and `focusCurrent()` read and restore the tab stop (a list ↔ cards switch, a closing bar).
  // When the focused item leaves `items` (filtered out, removed) while focus is in the grid, focus
  // moves to the item now at its index (or the last one), else to the grid itself. Items sharing
  // a key are dropped after the first (with a warning in dev), so a keyed each can't throw.
  import { tick, untrack, type Snippet } from "svelte";
  import { TILE_GAP, tileColumns } from "../../views/tileGrid";
  import { keyAction, scrollToReveal, type SelectIntent } from "./roving";

  interface Props {
    items: T[];
    /** A stable key per item (the mod's path). */
    getKey: (item: T) => string;
    /** Height of one item, px. */
    itemHeight: number;
    /** Gap between items (both axes), px. */
    gap?: number;
    /** Columns for a measured width; the default fits landscape cards (tileColumns). */
    columns?: (width: number) => number;
    /** Rows rendered beyond the viewport on each side. */
    overscan?: number;
    /** Extra scrollable space below the last row, so a floating footer doesn't hide it. */
    bottomPad?: number;
    /** ←/→ move by one item. */
    twoD?: boolean;
    /** Accessible name for the list. */
    label: string;
    /** Accessible name for an item ("Courseplay, active"); without it, its content names it. */
    itemLabel?: (item: T) => string;
    item: Snippet<[T, ItemContext]>;
    onopen?: (item: T) => void;
    ontoggle?: (item: T) => void;
    onselect?: (item: T, intent: SelectIntent) => void;
    /** Open the item's menu at `anchor` (the item element). */
    onmenu?: (item: T, anchor: HTMLElement) => void;
    /** Ctrl+A / Cmd+A on an item: select every item shown. */
    onselectall?: () => void;
  }

  let {
    items,
    getKey,
    itemHeight,
    gap = TILE_GAP,
    columns = tileColumns,
    overscan = 6,
    bottomPad = 0,
    twoD = true,
    label,
    itemLabel,
    item,
    onopen,
    ontoggle,
    onselect,
    onmenu,
    onselectall,
  }: Props = $props();

  /** Room kept above/below an item scrolled into view (matches the CSS scroll-padding). */
  const REVEAL_PAD = 8;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewportH = $state(0);
  let viewportW = $state(0);
  let focusedKey = $state<string | null>(null);

  /** `items` with repeated keys dropped (the first one stays). */
  const list = $derived.by(() => {
    const seen = new Set<string>();
    let dup: string | null = null;
    const out: T[] = [];
    for (const it of items) {
      const k = getKey(it);
      if (seen.has(k)) {
        dup ??= k;
        continue;
      }
      seen.add(k);
      out.push(it);
    }
    if (dup !== null && import.meta.env.DEV) console.warn(`[Silo] VirtualGrid "${label}": duplicate key ${dup}`);
    return dup === null ? items : out;
  });

  // jsdom (and the first frame) measure 0: assume a typical window so something renders.
  const vh = $derived(viewportH > 0 ? viewportH : 600);
  const cols = $derived(Math.max(1, Math.floor(columns(viewportW)) || 1));
  const stride = $derived(itemHeight + gap);
  const count = $derived(list.length);
  const rowCount = $derived(Math.ceil(count / cols));
  const pageRows = $derived(Math.max(1, Math.floor(vh / stride)));

  const keyIndex = $derived.by(() => {
    const m = new Map<string, number>();
    list.forEach((it, i) => m.set(getKey(it), i));
    return m;
  });
  const focusIndex = $derived(focusedKey === null ? -1 : (keyIndex.get(focusedKey) ?? -1));

  const firstRow = $derived(Math.max(0, Math.floor(scrollTop / stride) - overscan));
  const endRow = $derived(Math.min(rowCount, Math.ceil((scrollTop + vh) / stride) + overscan));

  // The one tab stop: the focused item, else the first item on screen.
  const tabIndexAt = $derived(
    focusIndex >= 0 ? focusIndex : Math.min(count - 1, Math.floor(scrollTop / stride) * cols),
  );

  const rendered = $derived.by(() => {
    const out: { it: T; index: number; key: string }[] = [];
    const from = firstRow * cols;
    const to = Math.min(count, endRow * cols);
    for (let i = from; i < to; i++) out.push({ it: list[i], index: i, key: getKey(list[i]) });
    if (focusIndex >= 0 && (focusIndex < from || focusIndex >= to)) {
      out.push({ it: list[focusIndex], index: focusIndex, key: focusedKey as string });
    }
    return out;
  });

  // The focused item left the list: before its element goes (pre-effect), note whether focus was
  // in the grid; if so, hand it to the item now at that index (clamped), or the grid itself.
  let lastIndex = -1;
  $effect.pre(() => {
    const key = focusedKey;
    const index = key === null ? undefined : keyIndex.get(key);
    if (key === null) return;
    if (index !== undefined) {
      lastIndex = index;
      return;
    }
    untrack(() => {
      const inside = !!viewport && viewport.contains(document.activeElement);
      const at = lastIndex;
      focusedKey = null;
      lastIndex = -1;
      if (!inside) return;
      if (count === 0) {
        void tick().then(() => viewport?.focus({ preventScroll: true }));
        return;
      }
      void moveTo(Math.min(Math.max(0, at), count - 1));
    });
  });

  function onScroll() {
    if (viewport) scrollTop = viewport.scrollTop;
  }

  function itemEl(key: string): HTMLElement | null {
    if (!viewport) return null;
    for (const el of viewport.querySelectorAll<HTMLElement>(".vg-item")) {
      if (el.dataset.key === key) return el;
    }
    return null;
  }

  function reveal(index: number) {
    if (!viewport) return;
    const row = Math.floor(index / cols);
    const next = scrollToReveal(row, stride, itemHeight, viewport.scrollTop, vh, REVEAL_PAD);
    if (next === null) return;
    viewport.scrollTop = next;
    scrollTop = next; // render the destination now, without waiting for the scroll event
  }

  async function moveTo(index: number) {
    const target = list[index];
    if (target === undefined) return;
    const key = getKey(target);
    focusedKey = key;
    reveal(index);
    await tick();
    itemEl(key)?.focus({ preventScroll: true });
  }

  /** The key of the item that holds the tab stop (the focused one), or null. */
  export function currentKey(): string | null {
    return focusedKey;
  }

  /**
   * Make the item with this key the tab stop and scroll it into view, without taking focus.
   * @returns false when no item has that key
   */
  export function showKey(key: string): boolean {
    const index = keyIndex.get(key);
    if (index === undefined) return false;
    focusedKey = key;
    reveal(index);
    return true;
  }

  /**
   * Focus the tab stop: the focused item, else the first one on screen.
   * @returns false when the list is empty
   */
  export function focusCurrent(): boolean {
    if (count === 0) return false;
    void moveTo(Math.max(0, tabIndexAt));
    return true;
  }

  /**
   * Move focus to the item with this key (scrolling it into view).
   * @param key the item's getKey value
   * @returns false when no item has that key (filtered out, removed)
   */
  export function focusKey(key: string): boolean {
    const index = keyIndex.get(key);
    if (index === undefined) return false;
    void moveTo(index);
    return true;
  }

  function onKey(e: KeyboardEvent, it: T, index: number) {
    const act = keyAction(e, {
      index,
      count,
      columns: cols,
      pageRows,
      onItem: e.target === e.currentTarget,
      twoD,
    });
    if (!act) return;
    e.preventDefault();
    switch (act.kind) {
      case "move":
        if (act.to === index) return;
        void moveTo(act.to);
        if (act.extend) onselect?.(list[act.to], { shift: true, ctrl: false, range: true, from: getKey(it) });
        return;
      case "open":
        onopen?.(it);
        return;
      case "toggle":
        ontoggle?.(it);
        return;
      case "select":
        onselect?.(it, { shift: false, ctrl: true });
        return;
      case "menu":
        onmenu?.(it, e.currentTarget as HTMLElement);
        return;
      case "selectAll":
        onselectall?.();
        return;
    }
  }

  // Position as a share of the list's width, so columns need no measured px.
  function leftOf(index: number): string {
    const col = index % cols;
    if (col === 0) return "0px";
    return `calc((100% - ${(cols - 1) * gap}px) / ${cols} * ${col} + ${col * gap}px)`;
  }
  const width = $derived(cols === 1 ? "100%" : `calc((100% - ${(cols - 1) * gap}px) / ${cols})`);
</script>

<!-- tabindex -1: focus lands here when the focused item leaves an emptied list. -->
<div
  class="vg-viewport"
  tabindex="-1"
  bind:this={viewport}
  bind:clientHeight={viewportH}
  bind:clientWidth={viewportW}
  onscroll={onScroll}
>
  <div
    class="vg-sizer"
    role="list"
    aria-label={label}
    style:height="{rowCount * stride + bottomPad}px"
  >
    {#each rendered as entry (entry.key)}
      <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
      <div
        class="vg-item"
        role="listitem"
        aria-label={itemLabel?.(entry.it)}
        data-key={entry.key}
        tabindex={entry.index === tabIndexAt ? 0 : -1}
        aria-setsize={count}
        aria-posinset={entry.index + 1}
        style:top="{Math.floor(entry.index / cols) * stride}px"
        style:left={leftOf(entry.index)}
        style:width={width}
        style:height="{itemHeight}px"
        onkeydown={(e) => onKey(e, entry.it, entry.index)}
        onfocusin={() => (focusedKey = entry.key)}
      >
        {@render item(entry.it, { index: entry.index, focused: entry.key === focusedKey })}
      </div>
    {/each}
  </div>
</div>

<style>
  .vg-viewport {
    height: 100%;
    overflow-y: auto;
    overflow-x: hidden;
    scrollbar-width: thin;
    scroll-padding-block: 8px; /* REVEAL_PAD */
  }
  .vg-sizer {
    position: relative;
    width: 100%;
  }
  .vg-viewport:focus {
    outline: none;
  }
  .vg-item {
    /* Rows are controls: a drag or Ctrl+A doesn't paint them as selected text. */
    -webkit-user-select: none;
    user-select: none;
    position: absolute;
    box-sizing: border-box;
    border-radius: var(--r-sm);
  }
  /* Inset, so the ring isn't clipped by the neighbouring rows or the viewport edge. */
  .vg-item:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: -2px;
  }
</style>
