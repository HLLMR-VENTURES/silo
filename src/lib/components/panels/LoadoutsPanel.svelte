<script lang="ts">
  import Icon from "../../ui/Icon.svelte";
  import type { Loadout } from "../../types";

  let {
    loadouts,
    activeCount,
    activeLoadoutId,
    onApply,
    onOverwrite,
    onExport,
    onRemove,
    onSaveCurrent,
    onImport,
    lockReason = "",
  }: {
    loadouts: Loadout[];
    activeCount: number;
    activeLoadoutId: number | null;
    onApply: (l: Loadout) => void;
    onOverwrite: (l: Loadout) => void;
    onExport: (l: Loadout) => void;
    onRemove: (l: Loadout) => void;
    onSaveCurrent: () => void;
    onImport: () => void;
    /** Why loadouts can't be changed right now (a foreground library change); "" = they can. */
    lockReason?: string;
  } = $props();
</script>

<div class="loadouts-panel">
  <div class="lp-head">
    <span>Loadouts</span>
    <span class="lp-sub tnum">{activeCount} active</span>
  </div>
  {#if loadouts.length === 0}
    <div class="lp-empty">
      No loadouts yet. Activate the mods you want, then save them as a set.
    </div>
  {/if}
  {#each loadouts as l (l.id)}
    <div class="lp-row" class:active={l.id === activeLoadoutId}>
      <button class="lp-apply" onclick={() => onApply(l)} title="Apply this loadout">
        <span class="lp-dot" class:on={l.id === activeLoadoutId}></span>
        <span class="lp-name">{l.name}</span>
        <span class="lp-count tnum">{l.mods.length}</span>
      </button>
      <button
        class="lp-icon"
        title={lockReason || "Overwrite with current active set"}
        aria-label="Overwrite with current active set"
        disabled={!!lockReason}
        onclick={() => onOverwrite(l)}><Icon name="rescan" size={14} /></button
      >
      <button
        class="lp-icon"
        title="Export to a .silo file"
        aria-label="Export to a .silo file"
        onclick={() => onExport(l)}><Icon name="share" size={14} /></button
      >
      <button
        class="lp-icon danger"
        title={lockReason || "Delete loadout"}
        aria-label="Delete loadout"
        disabled={!!lockReason}
        onclick={() => onRemove(l)}><Icon name="close" size={14} /></button
      >
    </div>
  {/each}
  <button
    class="lp-save"
    onclick={onSaveCurrent}
    disabled={activeCount === 0 || !!lockReason}
    title={lockReason || undefined}
  >
    + Save current active set as a loadout
  </button>
  <button class="lp-import" onclick={onImport} disabled={!!lockReason} title={lockReason || undefined}><Icon name="download" size={13} /> Import a .silo file</button>
</div>

<style>
  /* A page section in its view (it was a popover under the old top bar). */
  .loadouts-panel {
    max-width: 48rem;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px;
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
  .lp-row {
    display: flex;
    align-items: center;
    gap: 2px;
    border-radius: var(--radius-sm);
  }
  .lp-row.active {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
  }
  .lp-apply {
    flex: 1 1 auto;
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
    border: none;
    background: transparent;
    color: var(--text);
    padding: 9px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
  }
  .lp-apply:hover {
    background: color-mix(in srgb, var(--primary) 10%, transparent);
  }
  .lp-dot {
    flex: 0 0 auto;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 2px solid var(--border);
  }
  .lp-dot.on {
    background: var(--primary);
    border-color: var(--primary);
  }
  .lp-name {
    flex: 1 1 auto;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    text-align: left;
    font-weight: 600;
  }
  .lp-count {
    flex: 0 0 auto;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .lp-icon {
    flex: 0 0 auto;
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 28px;
    height: 30px;
    border-radius: var(--radius-sm);
    display: inline-grid;
    place-items: center;
  }
  .lp-icon:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
  }
  .lp-icon.danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  .lp-save {
    display: block;
    width: 100%;
    margin-top: 6px;
    border: 1px dashed var(--border);
    background: transparent;
    color: var(--primary-text);
    padding: 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .lp-save:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 10%, transparent);
  }
  .lp-save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .lp-import {
    display: block;
    width: 100%;
    margin-top: 6px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 8px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .lp-import:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 8%, transparent);
    color: var(--text);
  }
  .lp-icon:disabled,
  .lp-import:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
