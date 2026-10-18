<script lang="ts">
  // Mod sets: My sets (loadouts), Share (collections), Multiplayer (MP sync), Savegames. Each tab
  // is a snippet App renders with its own state and handlers:
  //   <ModSetsView>{#snippet setsTab()}<LoadoutsPanel ... />{/snippet}...</ModSetsView>
  // The tab lives in nav.tab.modSets, so go("modSets", "share") lands on Share.
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { nav, type ModSetsTab } from "../state/nav.svelte";
  import type { TabItem } from "../ui/types";

  interface Props {
    setsTab: Snippet;
    shareTab: Snippet;
    multiplayerTab: Snippet;
    savegamesTab: Snippet;
  }

  let { setsTab, shareTab, multiplayerTab, savegamesTab }: Props = $props();

  const TABS: TabItem<ModSetsTab>[] = [
    { id: "sets", label: "My sets" },
    { id: "share", label: "Share" },
    { id: "multiplayer", label: "Multiplayer" },
    { id: "savegames", label: "Savegames" },
  ];
  const panels: Record<ModSetsTab, Snippet> = $derived({
    sets: setsTab,
    share: shareTab,
    multiplayer: multiplayerTab,
    savegames: savegamesTab,
  });
</script>

<ViewBoundary name="Mod sets">
  <ViewHeader
    title="Mod sets"
    description="Save and switch sets of mods, share them, match a server, or start from a savegame."
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
