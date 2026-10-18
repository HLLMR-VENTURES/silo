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
    onClose,
  }: {
    rows: { s: Savegame; st: SaveStat }[];
    /** Why Back up is off (a library write is running), or "". */
    lockReason: string;
    onConfigure: (s: Savegame) => void;
    onBackup: (s: Savegame) => void;
    onLoadout: (s: Savegame) => void;
    onClose: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="loadouts-panel saves">
  <div class="lp-head"><span>Savegames</span></div>
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
        title={lockReason || "Back up this savegame (a safe copy — original untouched)"}
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
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .loadouts-panel {
    position: fixed;
    z-index: 50;
    top: 66px;
    right: 20px;
    width: 320px;
    max-height: 70vh;
    overflow-y: auto;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    padding: 8px;
    scrollbar-width: thin;
  }
  .loadouts-panel.saves {
    width: 380px;
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
