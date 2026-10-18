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
  // `focusKey(key)` moves focus to an item by key (the drawer hands focus back with it).
  import { tick, type Snippet } from "svelte";
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
    item: Snippet<[T, ItemContext]>;
    onopen?: (item: T) => void;
    ontoggle?: (item: T) => void;
    onselect?: (item: T, intent: SelectIntent) => void;
    /** Open the item's menu at `anchor` (the item element). */
    onmenu?: (item: T, anchor: HTMLElement) => void;
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
    item,
    onopen,
    ontoggle,
    onselect,
    onmenu,
  }: Props = $props();

  /** Room kept above/below an item scrolled into view (matches the CSS scroll-padding). */
  const REVEAL_PAD = 8;

  let viewport = $state<HTMLDivElement>();
  let scrollTop = $state(0);
  let viewportH = $state(0);
  let viewportW = $state(0);
  let focusedKey = $state<string | null>(null);

  // jsdom (and the first frame) measure 0: assume a typical window so something renders.
  const vh = $derived(viewportH > 0 ? viewportH : 600);
  const cols = $derived(Math.max(1, Math.floor(columns(viewportW)) || 1));
  const stride = $derived(itemHeight + gap);
  const count = $derived(items.length);
  const rowCount = $derived(Math.ceil(count / cols));
  const pageRows = $derived(Math.max(1, Math.floor(vh / stride)));

  const keyIndex = $derived.by(() => {
    const m = new Map<string, number>();
    items.forEach((it, i) => m.set(getKey(it), i));
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
    for (let i = from; i < to; i++) out.push({ it: items[i], index: i, key: getKey(items[i]) });
    if (focusIndex >= 0 && (focusIndex < from || focusIndex >= to)) {
      out.push({ it: items[focusIndex], index: focusIndex, key: focusedKey as string });
    }
    return out;
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
    const target = items[index];
    if (target === undefined) return;
    const key = getKey(target);
    focusedKey = key;
    reveal(index);
    await tick();
    itemEl(key)?.focus({ preventScroll: true });
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
        if (act.extend) onselect?.(items[act.to], { shift: true, ctrl: false, range: true });
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

<div
  class="vg-viewport"
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
  .vg-item {
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
