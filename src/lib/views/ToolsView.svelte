<script lang="ts">
  // Tools: Library stats, Controls map (input bindings), Fill type bridge, Graphics settings
  // (game.xml). Snippets App renders; the tab lives in nav.tab.tools.
  //   <ToolsView>{#snippet stats()}<StatsPanel ... />{/snippet}...</ToolsView>
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { nav, type ToolsTab } from "../state/nav.svelte";
  import type { TabItem } from "../ui/types";

  interface Props {
    stats: Snippet;
    controls: Snippet;
    bridge: Snippet;
    graphics: Snippet;
  }

  let { stats, controls, bridge, graphics }: Props = $props();

  const TABS: TabItem<ToolsTab>[] = [
    { id: "stats", label: "Library stats" },
    { id: "controls", label: "Controls map" },
    { id: "bridge", label: "Fill type bridge" },
    { id: "graphics", label: "Graphics settings" },
  ];
  const panels: Record<ToolsTab, Snippet> = $derived({ stats, controls, bridge, graphics });
</script>

<ViewBoundary name="Tools">
  <ViewHeader title="Tools" tabs={TABS} bind:tab={nav.tab.tools}>
    {#snippet panel(id)}
      <div class="tab-body">{@render panels[id as ToolsTab]?.()}</div>
    {/snippet}
  </ViewHeader>
</ViewBoundary>

<style>
  .tab-body {
    padding-top: var(--sp-4);
  }
</style>
