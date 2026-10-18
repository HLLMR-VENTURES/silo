<script lang="ts">
  import type { Savegame } from "../../types";

  interface SaveStat {
    total: number;
    present: { modName: string }[];
    missing: { modName: string }[];
  }

  let {
    rows,
    lockReason,
    onConfigure,
    onBackup,
    onLoadout,
  }: {
    rows: { s: Savegame; st: SaveStat }[];
    /** Why Back up is off (a library write is running), or "". */
    lockReason: string;
    onConfigure: (s: Savegame) => void;
    onBackup: (s: Savegame) => void;
    onLoadout: (s: Savegame) => void;
  } = $props();
</script>

<div class="loadouts-panel saves">
  <div class="lp-head"><span>Savegames</span></div>
  {#if rows.length === 0}
    <div class="sg-empty">No savegames found in your Farming Simulator 25 folder yet.</div>
  {/if}
  {#each rows as { s, st } (s.folder)}
    <div class="sg-row">
      <div class="sg-info">
        <div class="sg-name">{s.name}</div>
        <div class="sg-meta">
          slot {s.index}{s.mapTitle ? ` · ${s.mapTitle}` : ""} ·
          <span class="tnum">{st.present.length}</span>/{st.total} mods in library{#if st.missing.length > 0}, <span
              class="sg-missing tnum">{st.missing.length} missing</span
            >{/if}
        </div>
      </div>
      <button
        class="sg-make"
        title="Edit this savegame's difficulty and gameplay settings"
        onclick={() => onConfigure(s)}
      >
        Configure
      </button>
      <button
        class="sg-make"
        title={lockReason || "Back up this savegame (a safe copy, the original stays untouched)"}
        onclick={() => onBackup(s)}
        disabled={!!lockReason}
      >
        Back up
      </button>
      <button
        class="sg-make"
        title={st.missing.length
          ? `Build a loadout from the ${st.present.length} mods you have (${st.missing.length} missing)`
          : "Build a loadout from this save's mods"}
        onclick={() => onLoadout(s)}
        disabled={st.present.length === 0}
      >
        → Loadout
      </button>
    </div>
  {/each}
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
  .sg-empty {
    padding: 10px 8px 14px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .lp-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 6px 8px 10px;
    font-family: var(--font-display);
    font-weight: 600;
  }
  .sg-row {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    border-radius: var(--radius-sm);
  }
  .sg-row:hover {
    background: color-mix(in srgb, var(--primary) 8%, transparent);
  }
  .sg-info {
    flex: 1 1 auto;
    min-width: 0;
  }
  .sg-name {
    font-weight: 600;
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sg-meta {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
  }
  .sg-missing {
    color: var(--warn-text);
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
