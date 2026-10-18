<script lang="ts">
  // Every planned move, grouped by category (largest first). Rows that failed are marked.
  //   <OrganizeFiles {plan} failed={new Set(["FS25_Courseplay"])} dest={(mv) => mv.relTo} />
  import type { PlannedMove } from "../../types";
  import { groupPlan } from "../../firstRun/firstRun";
  import { formatCount } from "../../plural";

  interface Props {
    plan: PlannedMove[];
    /** Mod IDs that couldn't be filed. */
    failed?: Set<string>;
    /** The destination to show for a move (adopting shows the path inside the archive). */
    dest?: (mv: PlannedMove) => string;
  }

  let { plan, failed = new Set(), dest = (mv) => mv.relTo }: Props = $props();

  const groups = $derived(groupPlan(plan));
</script>

<div class="groups">
  {#each groups as g (g.category)}
    <section class="grp" aria-label="{g.category}, {g.moves.length} files">
      <div class="grp-head">
        <span class="grp-name">{g.category}</span>
        <span class="grp-count tnum">{formatCount(g.moves.length)}</span>
      </div>
      <ul class="files">
        {#each g.moves as mv (mv.techName)}
          <li class:failed={failed.has(mv.techName)} title={dest(mv)}>
            <span class="fn">{mv.fileName}</span>
            <span class="arrow" aria-hidden="true">→</span>
            <span class="dest mono">{dest(mv)}</span>
            {#if failed.has(mv.techName)}<span class="tag">Not moved</span>{/if}
          </li>
        {/each}
      </ul>
    </section>
  {/each}
</div>

<style>
  .groups {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .grp {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised, var(--bg));
    overflow: hidden;
  }
  .grp-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 11px;
    border-bottom: 1px solid var(--border);
  }
  .grp-name {
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .grp-count {
    font-size: var(--fs-xs);
    color: var(--muted);
    font-weight: 600;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    max-height: 168px;
    overflow-y: auto;
  }
  .files li {
    display: flex;
    align-items: center;
    gap: 7px;
    padding: 3px 11px;
    font-size: var(--fs-xs);
  }
  .files li.failed {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
  }
  .fn {
    flex: 0 1 auto;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .arrow {
    color: var(--muted);
    flex: 0 0 auto;
  }
  .dest {
    flex: 1 1 auto;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .mono {
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
  }
  .tag {
    flex: 0 0 auto;
    color: var(--danger);
    font-weight: 600;
  }
</style>
