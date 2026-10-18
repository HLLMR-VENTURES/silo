<script lang="ts">
  // Test-only: Tabs with a real panel snippet.
  import Tabs from "../Tabs.svelte";
  import type { TabItem } from "../types";

  let { tabs, selected = $bindable() }: { tabs: TabItem[]; selected?: string } = $props();
</script>

<Tabs {tabs} bind:selected ariaLabel="Details">
  {#snippet panel(id)}
    <p>Panel for {id}</p>
  {/snippet}
</Tabs>
