<script lang="ts">
  import type { UpdateRow } from "../../types";
  import { checkedDate, type CheckYourselfRow } from "../../sources";

  let {
    updateResults,
    availableUpdates,
    updateChecking,
    busy,
    checkYourself,
    onInstall,
    onOpenSource,
    onMarkChecked,
    onClose,
  }: {
    updateResults: UpdateRow[];
    availableUpdates: UpdateRow[];
    updateChecking: boolean;
    busy: string | null;
    /** Mods with a saved source page that the automatic check couldn't answer. */
    checkYourself: CheckYourselfRow[];
    onInstall: (row: UpdateRow) => void;
    onOpenSource: (row: CheckYourselfRow) => void;
    onMarkChecked: (row: CheckYourselfRow) => void;
    onClose: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="conflicts-panel">
  <div class="lp-head">
    <span>Mod updates</span>
    <span class="lp-sub tnum">
      {updateChecking ? "checking…" : `${availableUpdates.length} available`}
    </span>
  </div>
  {#if updateResults.length === 0 && !updateChecking}
    <div class="lp-empty">
      No matches in the catalog. Mods on GitHub can also be linked to their repo in
      the detail panel.
    </div>
  {/if}
  {#each availableUpdates as r (r.techName)}
    <div class="hz-row">
      <div class="up-row">
        <div>
          <div class="hz-name">
            {r.title}
            {#if r.source}<span class="up-src">{r.source}</span>{/if}
          </div>
          <div class="hz-detail">{r.current} → <b class="up-new">{r.latest}</b></div>
        </div>
        {#if r.assetUrl}
          <button class="sg-make" onclick={() => onInstall(r)} disabled={!!busy}>Install</button>
        {/if}
      </div>
    </div>
  {/each}
  {#if !updateChecking && updateResults.length > 0}
    <div class="hz-group">
      Up to date ({updateResults.filter((r) => r.hasUpdate === false).length}) ·
      Errors ({updateResults.filter((r) => r.error).length})
    </div>
  {/if}
  {#if !updateChecking && checkYourself.length > 0}
    <div class="hz-group">Check these yourself ({checkYourself.length})</div>
    <div class="up-note">
      Silo can't check these automatically. Open the page you saved to see if there's a newer
      version.
    </div>
    {#each checkYourself as s (s.techName)}
      <div class="hz-row">
        <div class="up-row">
          <div>
            <div class="hz-name">{s.title}</div>
            <div class="hz-detail">
              {s.host} · {s.lastChecked ? `last checked ${checkedDate(s.lastChecked)}` : "not checked yet"}
            </div>
          </div>
          <div class="up-acts">
            <button class="sg-make" onclick={() => onOpenSource(s)}>Open page ↗</button>
            <button class="sg-make" onclick={() => onMarkChecked(s)}>I checked it</button>
          </div>
        </div>
      </div>
    {/each}
  {/if}
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
  .hz-group {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    padding: 12px 8px 6px;
  }
  .hz-row {
    padding: 7px 10px;
    border-radius: var(--radius-sm);
  }
  .hz-row:hover {
    background: color-mix(in srgb, var(--primary) 6%, transparent);
  }
  .hz-name {
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .hz-detail {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
    font-variant-numeric: tabular-nums;
  }
  .up-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .up-new {
    color: var(--accent-text);
  }
  .up-acts {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .up-note {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    padding: 0 8px 6px;
    line-height: 1.5;
  }
  .up-src {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 1px 6px;
    margin-left: 6px;
    vertical-align: middle;
  }
  .sg-make {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--primary-text);
    padding: 7px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .sg-make:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
  }
  .sg-make:disabled {
    opacity: 0.5;
    cursor: default;
    color: var(--text-muted);
  }
</style>
