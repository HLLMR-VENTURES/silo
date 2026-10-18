<script lang="ts" generics="T">
  // The library list: a one-column VirtualGrid with fixed-height rows.
  //   <VirtualList bind:this={list} items={mods} getKey={(m) => m.path} rowHeight={56} label="Mods"
  //     onopen={open} ontoggle={toggle} onselect={select} onmenu={menu}>
  //     {#snippet row(mod, ctx)}<ModListRow {mod} />{/snippet}
  //   </VirtualList>
  // role="list" with role="listitem" rows (aria-setsize / aria-posinset), one tab stop; ↑/↓ Home
  // End PageUp PageDown move, Enter → onopen, Space → ontoggle (row focused, not a control in it),
  // Shift+↑/↓ → onselect(range), Ctrl+Space → onselect(toggle), Shift+F10 / Menu key → onmenu.
  // list.focusKey(key) puts focus back on a row by key.
  import type { Snippet } from "svelte";
  import VirtualGrid, { type ItemContext } from "./VirtualGrid.svelte";
  import type { SelectIntent } from "./roving";

  interface Props {
    items: T[];
    getKey: (item: T) => string;
    /** Fixed row height for the density, px. */
    rowHeight: number;
    overscan?: number;
    bottomPad?: number;
    label: string;
    row: Snippet<[T, ItemContext]>;
    onopen?: (item: T) => void;
    ontoggle?: (item: T) => void;
    onselect?: (item: T, intent: SelectIntent) => void;
    onmenu?: (item: T, anchor: HTMLElement) => void;
  }

  let {
    items,
    getKey,
    rowHeight,
    overscan = 6,
    bottomPad = 0,
    label,
    row,
    onopen,
    ontoggle,
    onselect,
    onmenu,
  }: Props = $props();

  let grid = $state<{ focusKey: (key: string) => boolean }>();
  const one = () => 1;

  /**
   * Move focus to the row with this key (scrolling it into view).
   * @returns false when no row has that key
   */
  export function focusKey(key: string): boolean {
    return grid?.focusKey(key) ?? false;
  }
</script>

<VirtualGrid
  bind:this={grid}
  {items}
  {getKey}
  itemHeight={rowHeight}
  gap={0}
  columns={one}
  twoD={false}
  {overscan}
  {bottomPad}
  {label}
  item={row}
  {onopen}
  {ontoggle}
  {onselect}
  {onmenu}
/>
