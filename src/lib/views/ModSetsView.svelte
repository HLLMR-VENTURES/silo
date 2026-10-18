<script lang="ts">
  // Mod sets: My sets, Open a shared set, Savegames. Each tab is a snippet App renders with its
  // own state and handlers:
  //   <ModSetsView>{#snippet setsTab()}<MySetsTab ... />{/snippet}...</ModSetsView>
  // The tab lives in nav.tab.modSets, so go("modSets", "open") lands on Open a shared set (where a
  // silo://collection link opens too).
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { nav, type ModSetsTab } from "../state/nav.svelte";
  import type { TabItem } from "../ui/types";

  interface Props {
    setsTab: Snippet;
    openTab: Snippet;
    savegamesTab: Snippet;
  }

  let { setsTab, openTab, savegamesTab }: Props = $props();

  const TABS: TabItem<ModSetsTab>[] = [
    { id: "sets", label: "My sets" },
    { id: "open", label: "Open a shared set" },
    { id: "savegames", label: "Savegames" },
  ];
  const panels: Record<ModSetsTab, Snippet> = $derived({
    sets: setsTab,
    open: openTab,
    savegames: savegamesTab,
  });
</script>

<ViewBoundary name="Mod sets">
  <ViewHeader
    title="Mod sets"
    description="Save and switch sets of mods, share them with friends, check you match a server, or start from a savegame."
    tabs={TABS}
    bind:tab={nav.tab.modSets}
  >
    {#snippet panel(id)}
      <div class="tab-body">{@render panels[id as ModSetsTab]?.()}</div>
    {/snippet}
  </ViewHeader>
</ViewBoundary>

<style>
  .tab-body {
    padding-top: var(--sp-4);
  }
</style>
