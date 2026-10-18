<script lang="ts">
  // The organize summary (first run step 3 and the Organize dialog): how many mods go into how
  // many categories, the biggest categories, and what stays the same.
  //   <OrganizeSummary {plan} activeCount={142} folderMods={3} />
  import Icon from "../../ui/Icon.svelte";
  import type { PlannedMove } from "../../types";
  import { groupPlan, nMods } from "../../firstRun/firstRun";
  import { formatCount, plural } from "../../plural";

  interface Props {
    plan: PlannedMove[];
    /** Active mods now (they stay active). */
    activeCount: number;
    /** Unzipped folder mods, which organizing leaves where they are. */
    folderMods?: number;
    /** How many categories to name before "…". */
    shown?: number;
    /** Heading level for the summary line (the setup flow uses its own h2). */
    heading?: boolean;
    /** The mods folder is cloud-synced: active mods are copied back, not linked. */
    cloudSynced?: boolean;
  }

  let { plan, activeCount, folderMods = 0, shown = 6, heading = true, cloudSynced = false }: Props =
    $props();

  const groups = $derived(groupPlan(plan));
  const title = $derived(
    `Ready to organize ${nMods(plan.length)} into ${formatCount(groups.length)} ${plural(groups.length, { one: "category", other: "categories" })}`,
  );
</script>

<div class="summary">
  {#if heading}<h3 class="title">{title}</h3>{:else}<p class="title">{title}</p>{/if}
  <p class="cats">
    {#each groups.slice(0, shown) as g, i (g.category)}{#if i > 0}<span class="sep" aria-hidden="true"> · </span>{/if}<span
        class="cat">{g.category} <span class="tnum">{formatCount(g.moves.length)}</span></span
      >{/each}{#if groups.length > shown}<span class="sep" aria-hidden="true"> · </span><span class="more"
        >and {formatCount(groups.length - shown)} more</span
      >{/if}
  </p>
  <ul class="facts">
    <li>
      <Icon name="ok" size={16} />
      <span>
        {#if activeCount > 0}
          {activeCount === 1 ? "Your active mod stays" : `All ${formatCount(activeCount)} active mods stay`} active. Silo
          {cloudSynced ? "copies" : "links"} {activeCount === 1 ? "it" : "them"} back into your mods folder.
        {:else}
          Mods you switch on later load from your mods folder as usual.
        {/if}
      </span>
    </li>
    <li>
      <Icon name="ok" size={16} />
      <span>
        Nothing is deleted.{#if folderMods > 0}{" "}Folder mods (unzipped) are left where they are.{/if}
      </span>
    </li>
    <li>
      <Icon name="ok" size={16} />
      <span>Changed your mind? Settings → Maintenance → Undo organizing puts every mod back.</span>
    </li>
  </ul>
</div>

<style>
  .summary {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 600;
    color: var(--text);
  }
  .cats {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .cat {
    white-space: nowrap;
  }
  .facts {
    list-style: none;
    margin: var(--sp-2) 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    font-size: var(--fs-sm);
  }
  .facts li {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
  }
  .facts :global(svg) {
    flex: 0 0 auto;
    margin-top: 2px;
    color: var(--success, var(--accent));
  }
</style>
