<script lang="ts">
  // Diagnose: Last run (crash & log triage, with guided bisection), Conflicts, Problems (library
  // health). Snippets App renders; the tab lives in nav.tab.diagnose.
  //   <DiagnoseView conflictsBadge={2} problemsBadge={1}>{#snippet lastRun()}...{/snippet}...</DiagnoseView>
  // Leaving the Last run tab (or the view) closes the log triage, as closing its panel did: a
  // running diagnosis keeps its snapshot and Silo offers to restore it.
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import { nav } from "../state/nav.svelte";
  import type { TabItem } from "../ui/types";

  type Tab = "lastRun" | "conflicts" | "problems";

  interface Props {
    lastRun: Snippet;
    conflicts: Snippet;
    problems: Snippet;
    /** Conflicts that aren't info-level; 0 hides the badge. */
    conflictsBadge?: number;
    /** Library health issues; 0 hides the badge. */
    problemsBadge?: number;
  }

  let { lastRun, conflicts, problems, conflictsBadge = 0, problemsBadge = 0 }: Props = $props();

  const tabs: TabItem<Tab>[] = $derived([
    { id: "lastRun", label: "Last run" },
    { id: "conflicts", label: "Conflicts", badge: conflictsBadge > 0 ? conflictsBadge : undefined },
    { id: "problems", label: "Problems", badge: problemsBadge > 0 ? problemsBadge : undefined },
  ]);
  const panels: Record<Tab, Snippet> = $derived({ lastRun, conflicts, problems });
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
      <div class="tab-body">{@render panels[id as Tab]?.()}</div>
    {/snippet}
  </ViewHeader>
</ViewBoundary>

<style>
  .tab-body {
    padding-top: var(--sp-4);
  }
</style>
