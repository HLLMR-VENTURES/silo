<script lang="ts">
  // Diagnose: Last run (crash & log triage, with guided bisection), Conflicts, Problems (library
  // health). Snippets App renders; the tab lives in nav.tab.diagnose.
  //   <DiagnoseView conflictsBadge={2} problemsBadge={1} bind:lastRunSlot>{#snippet conflictsTab()}...</DiagnoseView>
  // Last run's content can come two ways: a `lastRunTab` snippet (mounted with the tab), or from
  // outside through `lastRunSlot`, the element the tab shows. App does the second: it renders the
  // log triage in a KeepMounted docked into that slot, so a running guided diagnosis survives
  // switching tabs or views (it stays mounted, hidden, until it ends).
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { nav } from "../state/nav.svelte";
  import type { TabItem } from "../ui/types";

  type Tab = "lastRun" | "conflicts" | "problems";

  interface Props {
    /** Last run's content, mounted with the tab. Optional when the content docks into lastRunSlot. */
    lastRunTab?: Snippet;
    /** The Last run tab's slot element while that tab shows, else null (bindable). */
    lastRunSlot?: HTMLElement | null;
    conflictsTab: Snippet;
    problemsTab: Snippet;
    /** Conflicts that aren't info-level; 0 hides the badge. */
    conflictsBadge?: number;
    /** Library health issues; 0 hides the badge. */
    problemsBadge?: number;
  }

  let {
    lastRunTab,
    lastRunSlot = $bindable(null),
    conflictsTab,
    problemsTab,
    conflictsBadge = 0,
    problemsBadge = 0,
  }: Props = $props();

  const tabs: TabItem<Tab>[] = $derived([
    { id: "lastRun", label: "Last run" },
    { id: "conflicts", label: "Conflicts", badge: conflictsBadge > 0 ? conflictsBadge : undefined },
    { id: "problems", label: "Problems", badge: problemsBadge > 0 ? problemsBadge : undefined },
  ]);
  const panels: Record<Exclude<Tab, "lastRun">, Snippet> = $derived({
    conflicts: conflictsTab,
    problems: problemsTab,
  });
</script>

<ViewBoundary
  name="Diagnose"
  bodyText="Your mods are as they were before this view failed. If a diagnosis was running, Silo keeps your place."
>
  <ViewHeader
    title="Diagnose"
    description="Why the game crashed, which mods clash, and what in your library needs attention."
    {tabs}
    bind:tab={nav.tab.diagnose}
  >
    {#snippet panel(id)}
      {#if id === "lastRun"}
        <div class="tab-body" bind:this={lastRunSlot}>{@render lastRunTab?.()}</div>
      {:else}
        <div class="tab-body">{@render panels[id as Exclude<Tab, "lastRun">]?.()}</div>
      {/if}
    {/snippet}
  </ViewHeader>
</ViewBoundary>

<style>
  .tab-body {
    padding-top: var(--sp-4);
  }
</style>
