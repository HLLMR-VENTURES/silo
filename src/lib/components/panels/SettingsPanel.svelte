<script lang="ts">
  import { onMount } from "svelte";
  import type { GameInfo } from "../../types";
  import { openFolder, secretStorageSecure } from "../../api";
  import type { ModsRootStatus, ArchiveStatus } from "../../api";
  import GitHubAuth from "../GitHubAuth.svelte";

  // When the OS keychain is unavailable, connecting an account falls back to storing the
  // token in the local DB unencrypted — warn before the user connects rather than silently.
  let keychainOk = $state(true);
  onMount(async () => {
    try {
      keychainOk = await secretStorageSecure();
    } catch {
      keychainOk = true; // don't cry wolf if the probe itself errors
    }
  });

  let {
    theme,
    roots,
    gameInfo,
    userDir,
    appVer,
    autoFileNew,
    organizedCount,
    unorganizedCount,
    cloudSynced,
    busy,
    scanning,
    onSetTheme,
    onOpenGameGraphics,
    onExportReport,
    onSetAutoFile,
    onOrganize,
    onRebuild,
    onRestoreVanilla,
    onOptimizeLinks,
    onClose,
    modsStatus,
    onChangeModsFolder,
    onUseGameFolder,
    onSwitchModsFolder,
    archive,
    moveUnfinished,
    archiveLoadFailed,
    onChangeArchive,
    onArchiveDefault,
  }: {
    theme: "system" | "light" | "dark";
    roots: string[];
    gameInfo: GameInfo | null;
    userDir: string | null;
    appVer: string | null;
    autoFileNew: boolean;
    organizedCount: number;
    unorganizedCount: number;
    cloudSynced: boolean;
    busy: string | null;
    scanning: boolean;
    onSetTheme: (t: "system" | "light" | "dark") => void;
    onOpenGameGraphics: () => void;
    onExportReport: () => void;
    onSetAutoFile: (v: boolean) => void;
    onOrganize: () => void;
    onRebuild: () => void;
    onRestoreVanilla: () => void;
    onOptimizeLinks: () => void;
    onClose: () => void;
    modsStatus: ModsRootStatus | null;
    onChangeModsFolder: () => void;
    onUseGameFolder: () => void;
    onSwitchModsFolder: (path: string) => void;
    archive: ArchiveStatus | null;
    /** An archive move is running or paused — library writes are refused until it's done. */
    moveUnfinished: boolean;
    /** The startup status read gave up (App retries when Settings opens). */
    archiveLoadFailed: boolean;
    onChangeArchive: () => void;
    onArchiveDefault: () => void;
  } = $props();

  // Folder- and library-changing controls are off while busy, scanning, or mid-move.
  const locked = $derived(!!busy || scanning || moveUnfinished);
  /** Why a `locked` control is off, when it's the move. */
  const lockTitle = $derived(moveUnfinished ? "Finish or cancel the archive move first" : undefined);

  const others = $derived((modsStatus?.known ?? []).filter((k) => !k.current));
  function sourceLabel(s: ModsRootStatus["source"]): string {
    return s === "user"
      ? "Chosen in Silo"
      : s === "gameOverride"
        ? "Following FS25's setting"
        : "FS25 default";
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="loadouts-panel settings">
  <div class="lp-head"><span>Settings</span></div>

  <div class="set-section">
    <div class="set-label">Appearance</div>
    <div class="seg">
      {#each ["system", "light", "dark"] as t (t)}
        <button class="seg-btn" class:on={theme === t} onclick={() => onSetTheme(t as any)}>
          {t}
        </button>
      {/each}
    </div>
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Mods folder</div>
      {#if roots.length}
        <button class="set-link" onclick={() => openFolder(roots[0]).catch(() => {})}>
          Open ↗
        </button>
      {/if}
      <button class="set-link" disabled={locked} title={lockTitle} onclick={onChangeModsFolder}>
        Change…
      </button>
    </div>
    {#if modsStatus?.path}
      <div class="set-path" class:muted={!modsStatus.available}>{modsStatus.path}</div>
      <div class="set-hint">
        {sourceLabel(modsStatus.source)}{modsStatus.available ? "" : " · not available"}
      </div>
      {#if modsStatus.source === "user"}
        <button class="set-link" disabled={locked} title={lockTitle} onclick={onUseGameFolder}>
          Use the game's folder
        </button>
      {/if}
    {:else}
      <div class="set-path muted">No mods folder detected.</div>
    {/if}
    {#if modsStatus?.unresolvedOverride}
      <div class="set-hint">
        FS25's override points at <span class="tnum">{modsStatus.unresolvedOverride}</span>, which
        Silo can't map to a folder on this computer. Choose the folder with Change….
      </div>
    {/if}
    {#if modsStatus?.gameSettingProblem}
      <div class="set-hint">Silo can't update FS25's setting: {modsStatus.gameSettingProblem}.</div>
    {/if}
    {#if others.length}
      <div class="set-hint">Other folders Silo knows:</div>
      {#each others as k (k.path)}
        <button
          class="set-link known"
          disabled={locked}
          title={lockTitle}
          onclick={() => onSwitchModsFolder(k.path)}
        >
          {k.path} · {k.modCount} mods
        </button>
      {/each}
    {/if}
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Library archive</div>
      {#if archive?.path && archive.available}
        <button class="set-link" onclick={() => archive?.path && openFolder(archive.path).catch(() => {})}>
          Open ↗
        </button>
      {/if}
      <button class="set-link" disabled={locked || !archive?.path} title={lockTitle} onclick={onChangeArchive}>
        Change…
      </button>
    </div>
    {#if archive?.path}
      <div class="set-path" class:muted={!archive.available}>{archive.path}</div>
      <div class="set-hint">
        {archive.isDefault ? "Inside your mods folder (default)" : "Custom location"}{archive.available
          ? ""
          : " · not available"}{moveUnfinished ? " · move in progress" : ""}
      </div>
      {#if !archive.available && !archive.isDefault}
        <div class="set-hint">
          Reconnect its drive, or use Change… / Use default to point Silo elsewhere — nothing is moved.
        </div>
      {/if}
      {#if !archive.isDefault}
        <button class="set-link" disabled={locked} title={lockTitle} onclick={onArchiveDefault}
          >Use default</button
        >
      {/if}
    {:else}
      <div class="set-path muted">
        {archive
          ? "No mods folder selected."
          : archiveLoadFailed
            ? "Couldn't read the library archive — reopen Settings to retry."
            : "Loading…"}
      </div>
    {/if}
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Game</div>
      {#if gameInfo}
        <button class="set-link" onclick={() => gameInfo && openFolder(gameInfo.installDir).catch(() => {})}>
          Open ↗
        </button>
      {/if}
    </div>
    {#if gameInfo}
      <div class="set-path">{gameInfo.installDir}</div>
    {:else}
      <div class="set-path muted">
        Farming Simulator 25 install not found. The Launch button is hidden.
      </div>
    {/if}
  </div>

  {#if userDir}
    <div class="set-section">
      <div class="set-row">
        <div class="set-label">Graphics &amp; performance</div>
        <button class="set-link" onclick={onOpenGameGraphics}>Edit game.xml ↗</button>
      </div>
      <div class="set-hint">
        Tune graphics settings (presets: Performance / Balanced / Quality) without launching the game.
      </div>
    </div>
  {/if}

  {#if !keychainOk}
    <div class="set-section">
      <div class="keychain-warn">
        ⚠ No OS keychain is available on this machine, so a connected GitHub token
        would be stored <strong>unencrypted</strong> in Silo's local database. Connect an account
        only if you're comfortable with that; disconnecting removes the stored token.
      </div>
    </div>
  {/if}

  <div class="set-section">
    <div class="set-label">GitHub account</div>
    <GitHubAuth />
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Diagnostics</div>
      <button class="set-link" onclick={onExportReport}>Export report ↗</button>
    </div>
    <div class="set-hint">A shareable summary of your library, conflicts, and health issues.</div>
  </div>

  <div class="set-section">
    <div class="set-row">
      <div>
        <div class="set-label">Auto-file new mods</div>
        <div class="set-hint">
          On load, move newly-downloaded .zip mods into the library and keep them active.
        </div>
      </div>
      <button
        class="switch"
        class:on={autoFileNew}
        role="switch"
        aria-checked={autoFileNew}
        aria-label="Auto-file new mods"
        onclick={() => onSetAutoFile(!autoFileNew)}
      >
        <span class="knob"></span>
      </button>
    </div>
  </div>

  <div class="set-section">
    <div class="set-label">Library layout</div>
    <div class="set-hint">
      Organize sorts your <code>.zip</code> mods into <code>mods/archive/&lt;Category&gt;/</code>
      (subfolders the game ignores) and links the active set back into your mods folder.
      <b>Restore vanilla layout</b> puts every mod back. {#if organizedCount > 0}{organizedCount} mod(s) currently organized.{/if}
    </div>
    <div class="set-btnrow">
      <button class="set-btn" onclick={onOrganize} disabled={locked} title={lockTitle}>
        Organize {unorganizedCount > 0 ? unorganizedCount : "library"}
      </button>
      <button class="set-btn" onclick={onRebuild} disabled={!!busy || scanning}>
        ↻ Rebuild categories
      </button>
    </div>
    <div class="set-hint" style="margin-top:8px">
      Rebuild re-scans every mod from scratch — use it after a Silo update improves
      categorization, so existing mods pick up the better categories.
    </div>
    {#if organizedCount > 0 && cloudSynced}
      <div class="set-hint" style="margin-top:10px">
        ☁ Your mods folder is on a cloud-synced drive (OneDrive / Google Drive / Proton Drive),
        which doesn't support hardlinks — so Silo projects active mods as <b>copies</b>. That works,
        but uses more disk. Excluding this folder from cloud sync lets Silo use hardlinks instead
        (no duplication).
      </div>
    {:else if organizedCount > 0}
      <button
        class="set-btn"
        style="margin-top:10px;width:100%"
        onclick={onOptimizeLinks}
        disabled={locked}
        title={lockTitle}
      >
        ⛓ Optimize links (reclaim space)
      </button>
      <div class="set-hint" style="margin-top:8px">
        Collapses any active mod that was projected as a full copy (rather than a hardlink) back
        into a hardlink — frees the duplicated space and makes activating/deactivating instant.
        Safe: only touches files byte-identical to the archived copy, never a build you swapped in.
      </div>
    {/if}
    {#if organizedCount > 0}
      <button
        class="set-danger"
        style="margin-top:10px"
        onclick={onRestoreVanilla}
        disabled={locked}
        title={lockTitle}
      >
        Restore vanilla layout
      </button>
    {/if}
  </div>

  <div class="set-foot">Silo {appVer ? `v${appVer}` : "—"}</div>
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
  .loadouts-panel.settings {
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
  .set-section {
    padding: 10px 8px;
    border-top: 1px solid var(--border);
  }
  .set-section:first-of-type {
    border-top: none;
  }
  .set-label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 6px;
  }
  .set-hint {
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.5;
    margin-bottom: 8px;
  }
  .keychain-warn {
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text);
    background: color-mix(in srgb, var(--barn-500) 12%, var(--surface-raised));
    border: 1px solid color-mix(in srgb, var(--barn-500) 45%, var(--border));
    border-radius: var(--radius);
    padding: 10px 12px;
  }
  .set-foot {
    padding: 10px 4px 4px;
    text-align: center;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--text-muted);
    border-top: 1px solid var(--border);
    margin-top: 4px;
  }
  .set-link {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: 12px;
    font-weight: 600;
  }
  .set-link:hover {
    text-decoration: underline;
  }
  .seg {
    display: flex;
    gap: 4px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 3px;
  }
  .seg-btn {
    flex: 1 1 0;
    border: none;
    background: transparent;
    color: var(--text-muted);
    padding: 6px;
    border-radius: 5px;
    font-size: 12px;
    font-weight: 600;
    text-transform: capitalize;
  }
  .seg-btn.on {
    background: var(--surface-raised);
    color: var(--primary);
    box-shadow: var(--shadow-1);
  }
  .set-path {
    font-size: 12px;
    color: var(--text-muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 7px 9px;
    word-break: break-all;
    margin-bottom: 4px;
  }
  .set-path.muted {
    font-style: italic;
  }
  .set-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .switch {
    flex: 0 0 auto;
    width: 40px;
    height: 23px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg);
    position: relative;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .switch .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: transform 0.15s ease, background 0.15s ease;
  }
  .switch.on {
    background: color-mix(in srgb, var(--primary) 30%, transparent);
    border-color: var(--primary);
  }
  .switch.on .knob {
    transform: translateX(17px);
    background: var(--primary);
  }
  .set-btnrow {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .set-btn {
    flex: 1 1 auto;
    border: 1px solid var(--border);
    background: var(--surface-raised);
    color: var(--text);
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .set-btn:hover:not(:disabled) {
    border-color: var(--primary);
    color: var(--primary);
  }
  .set-btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .set-danger {
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border));
    background: transparent;
    color: var(--danger);
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    font-weight: 600;
    width: 100%;
    cursor: pointer;
  }
  .set-danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
  }
  .set-danger:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .known {
    display: block;
    text-align: left;
    word-break: break-all;
    margin-top: 2px;
  }
</style>
