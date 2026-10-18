<script lang="ts">
  import Icon from "../../ui/Icon.svelte";
  import { inlineError } from "../../state/notify.svelte";
  import { onMount, tick } from "svelte";
  import { nav, setTab } from "../../state/nav.svelte";
  import type { GameInfo } from "../../types";
  import { openKnownFolder, secretStorageSecure } from "../../api";
  import type { ModsRootStatus, ArchiveStatus } from "../../api";
  import GitHubAuth from "../GitHubAuth.svelte";
  import { UI_SCALES, parseUiScale, type UiScale } from "../../uiScale";

  // When the OS keychain is unavailable, connecting an account falls back to storing the
  // token in the local DB unencrypted — warn before the user connects rather than silently.
  let keychainOk = $state(true);

  // go("settings", "accounts") (Browse's "connect GitHub" path) lands on the GitHub section:
  // scrolled into view and focused, once. The tab is cleared so the next request does it again.
  /** The section ids go("settings", id) can land on. */
  const SECTION_IDS: Record<string, string> = { accounts: "settings-accounts" };
  $effect(() => {
    const target = nav.tab.settings;
    if (!target || !SECTION_IDS[target]) return;
    void tick().then(() => {
      const el = document.getElementById(SECTION_IDS[target]);
      if (!el) return;
      el.scrollIntoView?.({ block: "start" });
      el.focus({ preventScroll: true });
      setTab("settings", undefined);
    });
  });
  onMount(async () => {
    try {
      keychainOk = await secretStorageSecure();
    } catch {
      keychainOk = true; // don't cry wolf if the probe itself errors
    }
  });

  let {
    theme,
    uiScale,
    roots,
    gameInfo,
    userDir,
    appVer,
    autoFileNew,
    organizedCount,
    unorganizedCount,
    cloudSynced,
    lockReason,
    scanning,
    onSetTheme,
    onSetUiScale,
    onOpenGameGraphics,
    onExportReport,
    onCopyDiagnostics,
    onSetAutoFile,
    onOrganize,
    onRebuild,
    onRestoreVanilla,
    onOptimizeLinks,
    modsStatus,
    onChangeModsFolder,
    onUseGameFolder,
    onSwitchModsFolder,
    archive,
    moveUnfinished,
    archiveLoadFailed,
    onChangeArchive,
    onArchiveDefault,
    scanMs = null,
    update = null,
    updating = false,
    updatePct = null,
    onApplyUpdate,
  }: {
    theme: "system" | "light" | "dark";
    uiScale: UiScale;
    roots: string[];
    gameInfo: GameInfo | null;
    userDir: string | null;
    appVer: string | null;
    autoFileNew: boolean;
    organizedCount: number;
    unorganizedCount: number;
    cloudSynced: boolean;
    /** Why library-changing controls are off (a library write is running), or "". */
    lockReason: string;
    scanning: boolean;
    onSetTheme: (t: "system" | "light" | "dark") => void;
    onSetUiScale: (s: UiScale) => void;
    /** Opens Tools → Graphics settings (the game.xml editor lives there now). */
    onOpenGameGraphics: () => void;
    onExportReport: () => void;
    onCopyDiagnostics: () => void;
    onSetAutoFile: (v: boolean) => void;
    onOrganize: () => void;
    onRebuild: () => void;
    onRestoreVanilla: () => void;
    onOptimizeLinks: () => void;
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
    /** How long the last library scan took, for About. */
    scanMs?: number | null;
    /** A Silo update that's ready to install, if any. */
    update?: { version: string } | null;
    updating?: boolean;
    updatePct?: number | null;
    onApplyUpdate?: () => void;
  } = $props();

  // Folder- and library-changing controls are off during a library write, a scan, or mid-move.
  const locked = $derived(!!lockReason || scanning || moveUnfinished);
  /** Why a `locked` control is off. */
  const lockTitle = $derived(
    moveUnfinished
      ? "Finish or cancel the archive move first"
      : lockReason || (scanning ? "Wait for the scan to finish." : undefined),
  );

  const others = $derived((modsStatus?.known ?? []).filter((k) => !k.current));

  type OpenTarget = "modsFolder" | "archive" | "gameInstall";
  /** Why an Open ↗ link couldn't open its folder, shown under that link's row. */
  let openErr = $state<{ which: OpenTarget; msg: string } | null>(null);
  function openFolder(which: OpenTarget) {
    openErr = null;
    openKnownFolder(which).catch((e) => (openErr = { which, msg: inlineError(e, "Opening the folder") }));
  }
  function sourceLabel(s: ModsRootStatus["source"]): string {
    return s === "user"
      ? "Chosen in Silo"
      : s === "gameOverride"
        ? "Following FS25's setting"
        : "FS25 default";
  }
</script>

<!-- A page (SettingsView titles it): every section stacked at content width. -->
<div class="loadouts-panel settings">

  <div class="set-section">
    <div class="set-label">Appearance</div>
    <div class="seg">
      {#each ["system", "light", "dark"] as t (t)}
        <button class="seg-btn" class:on={theme === t} onclick={() => onSetTheme(t as any)}>
          {t}
        </button>
      {/each}
    </div>
    <div class="set-row ui-size">
      <label class="set-label" for="set-ui-size">UI size</label>
      <select
        id="set-ui-size"
        class="set-select"
        value={String(uiScale)}
        onchange={(e) => onSetUiScale(parseUiScale(e.currentTarget.value))}
      >
        {#each UI_SCALES as s (s)}
          <option value={String(s)}>{Math.round(s * 100)}%</option>
        {/each}
      </select>
    </div>
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Mods folder</div>
      {#if roots.length}
        <button class="set-link" onclick={() => openFolder("modsFolder")}>
          Open <Icon name="external" size={12} />
        </button>
      {/if}
      <button class="set-link" disabled={locked} title={lockTitle} onclick={onChangeModsFolder}>
        Change…
      </button>
    </div>
    {#if openErr?.which === "modsFolder"}<div class="set-hint">{openErr.msg}</div>{/if}
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
        <button class="set-link" onclick={() => openFolder("archive")}>
          Open <Icon name="external" size={12} />
        </button>
      {/if}
      <button class="set-link" disabled={locked || !archive?.path} title={lockTitle} onclick={onChangeArchive}>
        Change…
      </button>
    </div>
    {#if openErr?.which === "archive"}<div class="set-hint">{openErr.msg}</div>{/if}
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
        <button class="set-link" onclick={() => openFolder("gameInstall")}>
          Open <Icon name="external" size={12} />
        </button>
      {/if}
    </div>
    {#if openErr?.which === "gameInstall"}<div class="set-hint">{openErr.msg}</div>{/if}
    {#if gameInfo}
      <div class="set-path">{gameInfo.installDir}</div>
    {:else}
      <div class="set-path muted">
        Farming Simulator 25 install not found. The Play button is hidden.
      </div>
    {/if}
  </div>

  {#if userDir}
    <div class="set-section">
      <div class="set-row">
        <div class="set-label">Graphics &amp; performance</div>
        <button class="set-link" onclick={onOpenGameGraphics}>Edit game.xml</button>
      </div>
      <div class="set-hint">
        Tune graphics settings (presets: Performance / Balanced / Quality) without launching the game.
        Also in Tools → Graphics settings.
      </div>
    </div>
  {/if}

  {#if !keychainOk}
    <div class="set-section">
      <div class="keychain-warn">
        <Icon name="problem" size={13} /> No OS keychain is available on this machine, so a connected GitHub token
        would be stored <strong>unencrypted</strong> in Silo's local database. Connect an account
        only if you're comfortable with that; disconnecting removes the stored token.
      </div>
    </div>
  {/if}

  <div class="set-section" id="settings-accounts" tabindex="-1" aria-labelledby="settings-accounts-label">
    <div class="set-label" id="settings-accounts-label">GitHub account</div>
    <GitHubAuth />
  </div>

  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Diagnostics</div>
      <button class="set-link" onclick={onExportReport}>Export report <Icon name="external" size={12} /></button>
      <button class="set-link" onclick={onCopyDiagnostics}>Copy diagnostics</button>
    </div>
    <div class="set-hint">
      A summary of your library, conflicts and health issues, plus Silo's recent log. Your user name
      is removed.
    </div>
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
      <button class="set-btn" onclick={onRebuild} disabled={!!lockReason || scanning} title={lockReason || undefined}>
        <Icon name="rescan" size={13} /> Rebuild categories
      </button>
    </div>
    <div class="set-hint" style="margin-top:8px">
      Rebuild re-scans every mod from scratch — use it after a Silo update improves
      categorization, so existing mods pick up the better categories.
    </div>
    {#if organizedCount > 0 && cloudSynced}
      <div class="set-hint" style="margin-top:10px">
        <Icon name="cloud" size={13} /> Your mods folder is on a cloud-synced drive (OneDrive / Google Drive / Proton Drive),
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
        <Icon name="link" size={13} /> Optimize links (reclaim space)
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

  <div class="set-section">
    <div class="set-label">About</div>
    <div class="set-path tnum">Silo {appVer ? `v${appVer}` : "—"}</div>
    {#if scanMs !== null}
      <div class="set-hint tnum">Last library scan took {scanMs} ms.</div>
    {/if}
    {#if update}
      <div class="set-hint">Silo {update.version} is ready to install. Silo restarts to finish.</div>
      <button
        class="set-btn"
        onclick={() => onApplyUpdate?.()}
        disabled={updating}
        title="Download and install Silo {update.version}, then restart"
      >
        {#if updating}
          {updatePct != null ? `Updating… ${updatePct}%` : "Updating…"}
        {:else}
          <Icon name="update" size={13} /> Restart to update
        {/if}
      </button>
    {/if}
  </div>
</div>

<style>
  .loadouts-panel {
    max-width: 48rem;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px;
  }
  .set-section {
    padding: 10px 8px;
    border-top: 1px solid var(--border);
  }
  .set-section:first-of-type {
    border-top: none;
  }
  .set-section:focus {
    outline: none;
  }
  .set-section:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .set-label {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
    margin-bottom: 6px;
  }
  .set-hint {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
    margin-bottom: 8px;
  }
  .keychain-warn {
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--text);
    background: color-mix(in srgb, var(--danger) 12%, var(--surface-raised));
    border: 1px solid color-mix(in srgb, var(--danger) 45%, var(--border));
    border-radius: var(--radius);
    padding: 10px 12px;
  }
  .set-link {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
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
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: capitalize;
  }
  .ui-size {
    margin-top: 10px;
  }
  .ui-size .set-label {
    margin-bottom: 0;
  }
  .set-select {
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 5px 8px;
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .seg-btn.on {
    background: var(--surface-raised);
    color: var(--primary-text);
    box-shadow: var(--shadow-1);
  }
  .set-path {
    font-size: var(--fs-xs);
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
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .set-btn:hover:not(:disabled) {
    border-color: var(--primary);
    color: var(--primary-text);
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
    font-size: var(--fs-xs);
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
