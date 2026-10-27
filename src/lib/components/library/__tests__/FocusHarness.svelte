<script lang="ts">
  // Test-only: the BulkBar in place of a toolbar while something is selected, over a list, with
  // focus handed on (returnFocusWhenHidden) as App does.
  import BulkBar from "../BulkBar.svelte";
  import VirtualList from "../VirtualList.svelte";
  import { returnFocusWhenHidden } from "../focusReturn.svelte";

  type Item = { id: string; name: string };
  let { items, selected = false }: { items: Item[]; selected?: boolean } = $props();

  let bar = $state<HTMLElement | null>(null);
  let tool = $state<HTMLButtonElement | null>(null);
  let list = $state<{ focusCurrent: () => boolean }>();
  const noop = () => {};
  returnFocusWhenHidden(
    () => selected,
    () => bar,
    () => {
      if (!list?.focusCurrent()) tool?.focus();
    },
  );
</script>

{#if selected}
  <BulkBar
    bind:ref={bar}
    count={1}
    shownCount={items.length}
    onturnon={noop}
    onpark={noop}
    onfavorite={noop}
    onhide={noop}
    onmodsets={noop}
    onselectall={noop}
    onclear={noop}
  />
{:else}
  <button type="button" bind:this={tool}>First tool</button>
{/if}
<div style="height: 600px">
  <VirtualList bind:this={list} {items} getKey={(it) => it.id} rowHeight={56} label="Things">
    {#snippet row(it)}<span>{it.name}</span>{/snippet}
  </VirtualList>
</div>
