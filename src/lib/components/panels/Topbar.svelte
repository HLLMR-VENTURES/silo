<script lang="ts">
  let {
    topbarH = $bindable(),
    view,
    appVer,
    update,
    updating,
    updatePct,
    onApplyUpdate,
    hasSavegames,
    savesOpen,
    loadoutsOpen,
    activeLoadoutName,
    mpOpen,
    collectionsOpen,
    showOrganize,
    unorganizedCount,
    hasMods,
    updateChecking,
    scanning,
    busy,
    launchBlocked,
    organizeBlocked,
    hasGame,
    activeCount,
    settingsOpen,
    onSwitchView,
    onToggleSaves,
    onToggleLoadouts,
    onToggleMp,
    onToggleCollections,
    onOrganize,
    onCheckUpdates,
    onRescan,
    onLaunch,
    onToggleSettings,
  }: {
    topbarH: number;
    view: "library" | "browse";
    appVer: string | null;
    update: { version: string } | null;
    updating: boolean;
    updatePct: number | null;
    onApplyUpdate: () => void;
    hasSavegames: boolean;
    savesOpen: boolean;
    loadoutsOpen: boolean;
    activeLoadoutName: string | null;
    mpOpen: boolean;
    collectionsOpen: boolean;
    showOrganize: boolean;
    unorganizedCount: number;
    hasMods: boolean;
    updateChecking: boolean;
    scanning: boolean;
    busy: string | null;
    /** An archive move is copying right now — launching would race it. */
    launchBlocked: boolean;
    /** An archive move is unfinished (running or paused) — writes are refused. */
    organizeBlocked: boolean;
    hasGame: boolean;
    activeCount: number;
    settingsOpen: boolean;
    onSwitchView: (v: "library" | "browse") => void;
    onToggleSaves: () => void;
    onToggleLoadouts: () => void;
    onToggleMp: () => void;
    onToggleCollections: () => void;
    onOrganize: () => void;
    onCheckUpdates: () => void;
    onRescan: () => void;
    onLaunch: () => void;
    onToggleSettings: () => void;
  } = $props();
</script>

<header class="topbar" bind:clientHeight={topbarH}>
  <!-- Main row: identity + view tabs on the left, app actions on the right. -->
  <div class="topbar-main">
    <div class="brand">
      <div class="logo">S</div>
      <div>
        <h1>
          Silo{#if appVer}<span class="ver" title="Silo version {appVer}">v{appVer}</span>{/if}
        </h1>
        <p class="tagline">Farming Simulator 25 mod library</p>
      </div>
    </div>

    <nav class="tabs" aria-label="Views">
      <button class="tab" class:on={view === "library"} onclick={() => onSwitchView("library")}>
        Library
      </button>
      <button class="tab" class:on={view === "browse"} onclick={() => onSwitchView("browse")}>
        Browse
      </button>
    </nav>

    <div class="topbar-spacer"></div>

    {#if update}
      <button
        class="btn update-btn"
        onclick={onApplyUpdate}
        disabled={updating}
        title="Download and install Silo {update.version}, then restart"
      >
        {#if updating}
          {updatePct != null ? `Updating… ${updatePct}%` : "Updating…"}
        {:else}
          ⬆ Update to {update.version}
        {/if}
      </button>
    {/if}

    {#if showOrganize}
      <button
        class="btn"
        onclick={onOrganize}
        disabled={!!busy || scanning || organizeBlocked}
        title={organizeBlocked ? "Finish or cancel the archive move first" : undefined}
      >
        Organize {unorganizedCount}
      </button>
    {/if}
    {#if hasMods}
      <button
        class="btn"
        title="Check the SiloAPI catalog (and any linked GitHub repos) for updates"
        onclick={onCheckUpdates}
        disabled={!!busy || updateChecking}
      >
        {updateChecking ? "Checking…" : "⟳ Updates"}
      </button>
    {/if}
    <button class="btn" onclick={onRescan} disabled={scanning || !!busy}>
      {scanning ? "Scanning…" : "Rescan"}
    </button>
    {#if hasGame}
      <button
        class="btn primary launch-btn"
        title={launchBlocked
          ? "An archive move is running — pause it to play"
          : "Launch Farming Simulator 25 with the current active set"}
        onclick={onLaunch}
        disabled={!!busy || launchBlocked}
      >
        ▶ Launch{activeCount ? ` (${activeCount})` : ""}
      </button>
    {/if}
    <button
      class="btn icon-btn"
      class:on={settingsOpen}
      title="Settings"
      aria-label="Settings"
      onclick={onToggleSettings}
      disabled={!!busy}
    >
      ⚙
    </button>
  </div>

  <!-- Second row: the panel toggles, grouped intentionally rather than wrapping ad hoc. -->
  <nav class="topbar-nav" aria-label="Panels">
    {#if hasSavegames}
      <button class="btn" class:on={savesOpen} onclick={onToggleSaves} disabled={!!busy}>
        Savegames
      </button>
    {/if}

    <button
      class="btn loadout-btn"
      class:on={loadoutsOpen}
      onclick={onToggleLoadouts}
      disabled={!!busy}
    >
      {#if activeLoadoutName !== null}
        ● {activeLoadoutName}
      {:else}
        Loadouts
      {/if}
    </button>

    <button
      class="btn"
      class:on={collectionsOpen}
      title="Collections: share a curated set of mods as a link, or open one someone sent you"
      onclick={onToggleCollections}
      disabled={!!busy}
    >
      Collections
    </button>

    <button
      class="btn"
      class:on={mpOpen}
      title="Multiplayer: share or verify your mod set so friends can join"
      onclick={onToggleMp}
      disabled={!!busy}
    >
      Multiplayer
    </button>
  </nav>
</header>

<style>
  .topbar {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px 20px;
    border-bottom: 1px solid var(--border);
    background: var(--surface);
  }
  /* Main row: brand + tabs on the left, app actions pushed right by the spacer. */
  .topbar-main {
    display: flex;
    align-items: center;
    gap: 12px 16px;
    flex-wrap: wrap;
  }
  /* Second row: the panel toggles, on their own line. A hairline separates the two rows so the
     split reads as intentional rather than as accidental wrapping. */
  .topbar-nav {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
    padding-top: 10px;
    border-top: 1px solid var(--border);
  }
  /* The spacer pushes actions right on a wide bar, but must not force a blank row when the main
     row wraps — collapse it once that row is narrow enough to wrap. */
  @media (max-width: 900px) {
    .topbar-spacer {
      display: none;
    }
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .logo {
    width: 40px;
    height: 40px;
    border-radius: var(--radius);
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-weight: 600;
    font-size: var(--fs-xl);
    color: var(--on-primary);
    background: linear-gradient(135deg, var(--primary), var(--primary-hover));
    box-shadow: var(--shadow-1);
  }
  h1 {
    font-size: var(--fs-xl);
    line-height: 1.1;
  }
  .ver {
    margin-left: 6px;
    font-family: var(--font-mono, monospace);
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-muted);
    vertical-align: super;
    font-variant-numeric: tabular-nums;
  }
  .tagline {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .btn {
    border: 1px solid var(--border);
    background: var(--surface-raised);
    color: var(--text);
    padding: 9px 18px;
    border-radius: var(--radius);
    font-weight: 600;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .btn.primary {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--primary-hover);
  }
  .tabs {
    display: flex;
    gap: 4px;
    margin-left: 20px;
    padding: 3px;
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .tab {
    padding: 7px 16px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-weight: 600;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.15s ease, color 0.15s ease;
  }
  .tab:hover {
    color: var(--text);
  }
  .tab.on {
    background: var(--surface-raised);
    color: var(--text);
    box-shadow: var(--shadow-1);
  }
  .topbar-spacer {
    flex: 1 1 auto;
  }
  .icon-btn {
    padding: 9px 12px;
    font-size: 1rem;
    line-height: 1;
  }
  .icon-btn.on {
    color: var(--primary-text);
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
  }
  .launch-btn {
    font-weight: 700;
  }
  .update-btn {
    font-weight: 700;
    color: var(--accent-text);
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
    background: color-mix(in srgb, var(--accent) 14%, var(--surface-raised));
  }
  .update-btn:hover:not(:disabled) {
    color: var(--accent-text);
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 22%, var(--surface-raised));
  }
  .btn:hover:not(:disabled):not(.primary) {
    color: var(--text);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
  }
  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .loadout-btn.on {
    border-color: color-mix(in srgb, var(--primary) 50%, var(--border));
    color: var(--primary-text);
  }
</style>
