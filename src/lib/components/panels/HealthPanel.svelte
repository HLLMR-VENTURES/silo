<script lang="ts">
  import type { ModEntry } from "../../types";

  let {
    health,
    healthCount,
    onResolveForeign,
    onClose,
  }: {
    health: {
      missingDeps: { mod: ModEntry; missing: string[] }[];
      corrupt: ModEntry[];
      ignored: ModEntry[];
      foreign: {
        techName: string;
        fileName: string;
        kind: string;
        flatVersion: string | null;
        managedVersion: string | null;
      }[];
    };
    healthCount: number;
    onResolveForeign: (fileName: string, action: "adopt" | "restore") => Promise<void>;
    onClose: () => void;
  } = $props();

  let busy = $state<string | null>(null);
  async function resolve(fileName: string, action: "adopt" | "restore") {
    busy = fileName;
    try {
      await onResolveForeign(fileName, action);
    } finally {
      busy = null;
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="conflicts-panel">
  <div class="lp-head">
    <span>Library health</span>
    <span class="lp-sub tnum">{healthCount} issue{healthCount === 1 ? "" : "s"}</span>
  </div>

  {#if healthCount === 0}
    <div class="lp-empty">Everything looks healthy — no problems found.</div>
  {/if}

  {#if health.missingDeps.length > 0}
    <div class="hz-group">Missing dependencies ({health.missingDeps.length})</div>
    {#each health.missingDeps as d (d.mod.techName)}
      <div class="hz-row">
        <div class="hz-name">{d.mod.title ?? d.mod.techName}</div>
        <div class="hz-detail">
          needs {#each d.missing as dep, i (dep)}<span class="hz-dep">{dep}</span>{#if i < d.missing.length - 1}, {/if}{/each}
          — not in your library
        </div>
      </div>
    {/each}
  {/if}

  {#if health.ignored.length > 0}
    <div class="hz-group">Ignored by the game — name starts with a digit ({health.ignored.length})</div>
    {#each health.ignored as m (m.techName)}
      <div class="hz-row">
        <div class="hz-name">{m.title ?? m.techName}</div>
        <div class="hz-detail">
          <span class="tnum">{m.techName}</span> — FS won't load a mod whose name starts with a number.
        </div>
      </div>
    {/each}
  {/if}

  {#if health.corrupt.length > 0}
    <div class="hz-group">Corrupt / unreadable ({health.corrupt.length})</div>
    {#each health.corrupt as m (m.techName)}
      <div class="hz-row">
        <div class="hz-name">{m.title ?? m.techName}</div>
        <div class="hz-detail">{m.error}</div>
      </div>
    {/each}
  {/if}

  {#if health.foreign.length > 0}
    <div class="hz-group">Foreign files in your mods folder ({health.foreign.length})</div>
    {#each health.foreign as f (f.fileName)}
      <div class="hz-row">
        <div class="hz-name tnum">{f.fileName}</div>
        <div class="hz-detail">
          The file in your mods folder isn't the one Silo manages. Pick which build to keep —
          the other is moved to <span class="tnum">backups/</span>, so nothing is lost.
        </div>
        <div class="hz-vers">
          <span class="hz-ver">Mods folder: <b class="tnum">{f.flatVersion ? `v${f.flatVersion}` : "unknown"}</b></span>
          <span class="hz-ver">Silo's copy: <b class="tnum">{f.managedVersion ? `v${f.managedVersion}` : "unknown"}</b></span>
        </div>
        <div class="hz-actions">
          <button
            class="hz-btn"
            disabled={busy === f.fileName}
            title="Adopt the file in your mods folder as the mod's managed version"
            onclick={() => resolve(f.fileName, "adopt")}
          >
            Keep the mods-folder file{f.flatVersion ? ` (v${f.flatVersion})` : ""}
          </button>
          <button
            class="hz-btn"
            disabled={busy === f.fileName}
            title="Restore Silo's managed copy over the file in your mods folder"
            onclick={() => resolve(f.fileName, "restore")}
          >
            Keep Silo's copy{f.managedVersion ? ` (v${f.managedVersion})` : ""}
          </button>
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
  }
  .hz-dep {
    color: var(--warn-text);
    font-weight: 600;
  }
  .hz-vers {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .hz-ver b {
    color: var(--text);
  }
  .hz-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .hz-btn {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 5px 10px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .hz-btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
  }
  .hz-btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
