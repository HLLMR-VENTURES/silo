<script lang="ts" generics="T">
  // The library list: a one-column VirtualGrid with fixed-height rows.
  //   <VirtualList bind:this={list} items={mods} getKey={(m) => m.path} rowHeight={56} label="Mods"
  //     onopen={open} ontoggle={toggle} onselect={select} onmenu={menu}>
  //     {#snippet row(mod, ctx)}<ModListRow {mod} />{/snippet}
  //   </VirtualList>
  // role="list" with role="listitem" rows (aria-setsize / aria-posinset), one tab stop; ↑/↓ Home
  // End PageUp PageDown move, Enter → onopen, Space → ontoggle (row focused, not a control in it),
  // Shift+↑/↓ → onselect(range), Ctrl+Space → onselect(toggle), Shift+F10 / Menu key → onmenu.
  // list.focusKey(key) puts focus back on a row by key; showKey / currentKey / focusCurrent as in
  // VirtualGrid. `itemLabel` names each row ("Courseplay, active").
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
    itemLabel?: (item: T) => string;
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
    itemLabel,
    row,
    onopen,
    ontoggle,
    onselect,
    onmenu,
  }: Props = $props();

  let grid = $state<{
    focusKey: (key: string) => boolean;
    showKey: (key: string) => boolean;
    currentKey: () => string | null;
    focusCurrent: () => boolean;
  }>();
  const one = () => 1;

  /**
   * Move focus to the row with this key (scrolling it into view).
   * @returns false when no row has that key
   */
  export function focusKey(key: string): boolean {
    return grid?.focusKey(key) ?? false;
  }
  /** Make the row with this key the tab stop and scroll to it, without taking focus. */
  export function showKey(key: string): boolean {
    return grid?.showKey(key) ?? false;
  }
  /** The key of the row that holds the tab stop, or null. */
  export function currentKey(): string | null {
    return grid?.currentKey() ?? null;
  }
  /** Focus the tab stop row; false when the list is empty. */
  export function focusCurrent(): boolean {
    return grid?.focusCurrent() ?? false;
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
  {itemLabel}
  item={row}
  {onopen}
  {ontoggle}
  {onselect}
  {onmenu}
/>
