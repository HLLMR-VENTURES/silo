<script lang="ts">
  // Test-only: ViewHeader with actions and tabs.
  import ViewHeader from "../ViewHeader.svelte";
  import type { TabItem } from "../../../ui/types";

  const tabs: TabItem[] = [
    { id: "lastRun", label: "Last run" },
    { id: "conflicts", label: "Conflicts", badge: 2 },
  ];
  let { tab = $bindable() }: { tab?: string } = $props();
</script>

<ViewHeader title="Diagnose" description="Find what breaks the game." {tabs} bind:tab>
  {#snippet actions()}<button type="button">Run check</button>{/snippet}
  {#snippet panel(id)}<p>Panel {id}</p>{/snippet}
</ViewHeader>
