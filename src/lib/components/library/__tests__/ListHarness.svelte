<script lang="ts">
  // Test-only: VirtualList over plain items, each row with an inner button.
  import VirtualList from "../VirtualList.svelte";
  import type { SelectIntent } from "../roving";

  type Item = { id: string; name: string };
  let {
    items,
    rowHeight = 56,
    onopen,
    ontoggle,
    onselect,
    onmenu,
    onselectall,
    oninner,
    itemLabel,
  }: {
    items: Item[];
    rowHeight?: number;
    onopen?: (it: Item) => void;
    ontoggle?: (it: Item) => void;
    onselect?: (it: Item, intent: SelectIntent) => void;
    onmenu?: (it: Item, anchor: HTMLElement) => void;
    onselectall?: () => void;
    oninner?: (it: Item) => void;
    itemLabel?: (it: Item) => string;
  } = $props();

  let list = $state<{
    focusKey: (key: string) => boolean;
    showKey: (key: string) => boolean;
    currentKey: () => string | null;
  }>();
  export function focusKey(key: string): boolean {
    return list?.focusKey(key) ?? false;
  }
  export function showKey(key: string): boolean {
    return list?.showKey(key) ?? false;
  }
  export function currentKey(): string | null {
    return list?.currentKey() ?? null;
  }
</script>

<div style="height: 600px">
  <VirtualList
    bind:this={list}
    {items}
    getKey={(it) => it.id}
    {rowHeight}
    label="Things"
    {itemLabel}
    {onopen}
    {ontoggle}
    {onselect}
    {onmenu}
    {onselectall}
  >
    {#snippet row(it)}
      <span>{it.name}</span>
      <button type="button" tabindex="-1" onclick={() => oninner?.(it)}>Inner {it.name}</button>
    {/snippet}
  </VirtualList>
</div>
