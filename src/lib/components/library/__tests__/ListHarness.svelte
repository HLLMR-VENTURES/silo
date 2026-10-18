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
    oninner,
  }: {
    items: Item[];
    rowHeight?: number;
    onopen?: (it: Item) => void;
    ontoggle?: (it: Item) => void;
    onselect?: (it: Item, intent: SelectIntent) => void;
    onmenu?: (it: Item, anchor: HTMLElement) => void;
    oninner?: (it: Item) => void;
  } = $props();

  let list = $state<{ focusKey: (key: string) => boolean }>();
  export function focusKey(key: string): boolean {
    return list?.focusKey(key) ?? false;
  }
</script>

<div style="height: 600px">
  <VirtualList
    bind:this={list}
    {items}
    getKey={(it) => it.id}
    {rowHeight}
    label="Things"
    {onopen}
    {ontoggle}
    {onselect}
    {onmenu}
  >
    {#snippet row(it)}
      <span>{it.name}</span>
      <button type="button" tabindex="-1" onclick={() => oninner?.(it)}>Inner {it.name}</button>
    {/snippet}
  </VirtualList>
</div>
