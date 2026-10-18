<script lang="ts">
  import type { Conflict } from "../../types";

  let {
    conflicts,
    criticalCount,
    activeCount,
    onClose,
  }: {
    conflicts: Conflict[];
    criticalCount: number;
    activeCount: number;
    onClose: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="conflicts-panel">
  <div class="lp-head">
    <span>Conflicts in the active set</span>
    <span class="lp-sub tnum">{criticalCount} critical</span>
  </div>
  {#if conflicts.length === 0}
    <div class="lp-empty">
      No conflicts in the {activeCount} active mod(s). Activate more and Silo re-checks automatically.
    </div>
  {/if}
  {#each conflicts as c (c.severity + c.kind + c.name)}
    <div class="cf-row" class:crit={c.severity === "critical"} class:info={c.severity === "info"}>
      <div class="cf-top">
        <span class="cf-sev">{c.severity}</span>
        <span class="cf-kind">{c.kind}</span>
        <span class="cf-name">{c.name}</span>
      </div>
      <div class="cf-mods">{c.mods.join("  ·  ")}</div>
      <div class="cf-why">{c.explanation}</div>
    </div>
  {/each}
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .conflicts-panel {
    position: fixed;
    z-index: 50;
    top: 120px;
    left: 50%;
    transform: translateX(-50%);
    width: 560px;
    max-width: calc(100vw - 40px);
    max-height: 70vh;
    overflow-y: auto;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    padding: 10px;
    scrollbar-width: thin;
  }
  .lp-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 6px 8px 10px;
    font-family: var(--font-display);
    font-weight: 600;
  }
  .lp-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    font-family: var(--font-ui);
  }
  .lp-empty {
    padding: 10px 8px 14px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .cf-row {
    padding: 10px 10px 12px;
    border-radius: var(--radius-sm);
    border-left: 3px solid var(--warn-text);
    background: color-mix(in srgb, var(--warn-text) 6%, transparent);
    margin-bottom: 8px;
  }
  .cf-row.crit {
    border-left-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 6%, transparent);
  }
  .cf-row.info {
    border-left-color: var(--info);
    background: color-mix(in srgb, var(--info) 5%, transparent);
  }
  .cf-row.info .cf-sev {
    color: var(--info);
  }
  .cf-top {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .cf-sev {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--warn-text);
  }
  .cf-row.crit .cf-sev {
    color: var(--danger);
  }
  .cf-kind {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .cf-name {
    font-weight: 600;
    font-family: var(--font-display);
  }
  .cf-mods {
    margin-top: 5px;
    font-size: var(--fs-xs);
    color: var(--text);
  }
  .cf-why {
    margin-top: 5px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
</style>
