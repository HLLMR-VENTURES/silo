<script lang="ts">
  // Test-only: VirtualGrid with a fixed column count.
  import VirtualGrid from "../VirtualGrid.svelte";

  type Item = { id: string; name: string };
  let { items, cols = 3 }: { items: Item[]; cols?: number } = $props();
</script>

<VirtualGrid {items} getKey={(it) => it.id} itemHeight={100} columns={() => cols} label="Cards">
  {#snippet item(it)}<span>{it.name}</span>{/snippet}
</VirtualGrid>
