<script lang="ts">
  import type { ModEntry, CurationRow, Conflict, CatalogModDetail, SourceRow } from "../types";
  import type { ForeignFile } from "../api";
  import { getModIcon, revealInFolder, catalogDetailByTech, openExternal } from "../api";
  import { label as sourceLabel, loadCatalogImage } from "../browse";
  import { resizable } from "../resize";
  import ModStatus from "./ModStatus.svelte";
  import ModCuration from "./ModCuration.svelte";
  import ModRepoLink from "./ModRepoLink.svelte";
  import ModSourceLink from "./ModSourceLink.svelte";
  import ProvenanceCheck from "./ProvenanceCheck.svelte";

  type Flag = "favorite" | "hidden" | "broken";
  interface Props {
    mod: ModEntry;
    curation: CurationRow;
    tags: string[];
    active: boolean;
    organized: boolean;
    hasSettings: boolean;
    libraryTechNames: Set<string>;
    conflicts: Conflict[];
    categories: string[];
    isOverridden: boolean;
    onSetCategory: (category: string, subcategory: string | null) => void;
    onResetCategory: () => void;
    onClose: () => void;
    onToggle: (flag: Flag) => void;
    onToggleActive: () => void;
    onOpenSettings: () => void;
    onFindInBrowse: () => void;
    onCurationChange: (row: CurationRow) => void;
    onTagsChange: (tags: string[]) => void;
    onFilterTag: (tag: string) => void;
    repo: { owner: string; repo: string } | null;
    onRepoChange: (r: { owner: string; repo: string } | null) => void;
    /** The page this mod came from, if the user saved one. */
    source: SourceRow | null;
    onSourceChange: (row: SourceRow | null) => void;
    onInstalled: () => void;
    /** A build sitting in the mods folder that isn't Silo's managed copy (usually a manual update
     *  the user dropped in). Null when the projected file matches. */
    foreign: ForeignFile | null;
    onAdoptForeign: () => void;
    onRestoreForeign: () => void;
  }
  let {
    mod,
    curation,
    tags,
    active,
    organized,
    hasSettings,
    libraryTechNames,
    conflicts,
    categories,
    isOverridden,
    onSetCategory,
    onResetCategory,
    onClose,
    onToggle,
    onToggleActive,
    onOpenSettings,
    onFindInBrowse,
    onCurationChange,
    onTagsChange,
    onFilterTag,
    repo,
    onRepoChange,
    source,
    onSourceChange,
    onInstalled,
    foreign,
    onAdoptForeign,
    onRestoreForeign,
  }: Props = $props();

  // Is the dropped-in build newer than Silo's managed copy? (segment-wise numeric compare)
  const foreignIsNewer = $derived(
    !!(foreign?.flatVersion && foreign?.managedVersion
      ? versionNewer(foreign.flatVersion, foreign.managedVersion)
      : foreign?.flatVersion && !foreign?.managedVersion),
  );

  // Category override editor — the discoverable way to fix a miscategorized mod.
  // Drafts are synced from the prop by the effect below (which also re-syncs when the
  // drawer switches mods), so they start empty rather than capturing the initial prop.
  let catDraft = $state("");
  let subDraft = $state("");
  $effect(() => {
    catDraft = mod.category;
    subDraft = mod.subcategory ?? "";
  });
  const catDirty = $derived(
    catDraft !== mod.category || (subDraft.trim() || null) !== (mod.subcategory ?? null),
  );

  let icon = $state<string | null>(null);
  $effect(() => {
    const m = mod;
    icon = null;
    if (m.iconFilename) {
      getModIcon(m.path, m.kind, m.iconFilename).then((u) => {
        if (u && mod.path === m.path) icon = u;
      });
    }
  });

  const label = $derived(mod.title ?? mod.techName);
  const myConflicts = $derived(conflicts.filter((c) => c.mods.includes(label)));
  const sizeLabel = $derived(formatSize(mod.size));
  function formatSize(b: number): string {
    if (b >= 1024 ** 3) return (b / 1024 ** 3).toFixed(1) + " GB";
    if (b >= 1024 ** 2) return (b / 1024 ** 2).toFixed(0) + " MB";
    if (b >= 1024) return (b / 1024).toFixed(0) + " KB";
    return b + " B";
  }

  // Catalog awareness: resolve this library mod to its catalog record, so the library drawer
  // shows the same summary / sources / latest version Browse does — the "is this outdated?"
  // loop, without leaving the library. Null when the mod isn't catalogued.
  let catalog = $state<CatalogModDetail | null>(null);
  let showFullDesc = $state(false);
  $effect(() => {
    const tn = mod.techName;
    catalog = null;
    showFullDesc = false;
    catalogDetailByTech(tn)
      .then((c) => {
        if (mod.techName === tn) catalog = c;
      })
      .catch(() => {});
  });
  // Catalog cover image, loaded the same way Browse does — so the two drawers share a header.
  let cover = $state("");
  $effect(() => {
    const url = catalog?.imageUrl;
    cover = "";
    if (url) loadCatalogImage(url).then((u) => { if (catalog?.imageUrl === url) cover = u; });
  });
  // The library list stays clickable behind this drawer (clicking another tile switches it), so
  // Escape is the quick close alongside the ✕.
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      if (showFullDesc) showFullDesc = false; // close the full-description modal first
      else onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });
  // True only when `latest` is *strictly newer* than `current` (segment-wise numeric compare,
  // same rule as the Rust `github::is_newer`). A merely-different — or older — catalog version
  // is NOT an update, so a locally-newer mod no longer reads as "update available".
  function versionNewer(latest: string, current: string): boolean {
    const seg = (s: string) =>
      s
        .replace(/^v/i, "")
        .split(/[.\-+_]/)
        .map((p) => parseInt(p, 10))
        .filter((n) => Number.isFinite(n));
    const a = seg(latest);
    const b = seg(current);
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      const x = a[i] ?? 0;
      const y = b[i] ?? 0;
      if (x !== y) return x > y;
    }
    return false;
  }
  const catalogNewer = $derived(
    !!(catalog?.latestVersion && mod.version && versionNewer(catalog.latestVersion, mod.version)),
  );
  const summaryText = $derived((catalog?.descriptionFull || catalog?.description || "").trim());
  const summaryHasMore = $derived(summaryText.length > 420);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" style="top: var(--topbar-h, 0px)" onclick={onClose}></div>
<aside class="drawer" use:resizable>
  <div class="d-top">
    <div class="d-head">
      <div class="d-title">{label}</div>
      <div class="d-sub">
        {mod.author ?? "Unknown author"}{mod.version ? ` · v${mod.version}` : ""}
      </div>
      <div class="d-tech tnum">{mod.techName}</div>
    </div>
    <button class="d-x" onclick={onClose} aria-label="Close">✕</button>
  </div>

  {#if foreign}
    <div class="d-foreign">
      <div class="d-foreign-msg">
        <b
          >{foreignIsNewer
            ? `A newer build is in your mods folder — v${foreign.flatVersion ?? "?"}.`
            : `A different build is in your mods folder — v${foreign.flatVersion ?? "?"}.`}</b
        >
        Silo manages <b>v{foreign.managedVersion ?? "?"}</b>. This is how ModHub updates arrive —
        you download the zip into the folder. <b>Adopt</b> it to make it Silo's managed version.
      </div>
      <div class="d-foreign-actions">
        <button class="d-adopt" onclick={onAdoptForeign}>
          ⬆ Adopt v{foreign.flatVersion ?? ""}
        </button>
        <button class="d-restore" onclick={onRestoreForeign}>
          Keep Silo's v{foreign.managedVersion ?? ""}
        </button>
      </div>
    </div>
  {/if}

  {#if cover}
    <img class="d-cover" src={cover} alt="" />
  {:else if icon}
    <img class="d-cover d-cover-sq" src={icon} alt="" />
  {/if}

  <div class="d-actions">
    <button class="d-act" class:on={active} onclick={onToggleActive}>
      {active ? "● Active" : "○ Parked"}
    </button>
    <button class="d-act" class:on={curation.favorite} onclick={() => onToggle("favorite")}>
      {curation.favorite ? "★" : "☆"} Favorite
    </button>
    <button class="d-act" class:on={curation.broken} onclick={() => onToggle("broken")}>
      ⚠ Broken
    </button>
    <button class="d-act" class:on={curation.hidden} onclick={() => onToggle("hidden")}>
      ⊘ Hidden
    </button>
    {#if hasSettings}
      <button class="d-act" onclick={onOpenSettings}>⚙ Settings</button>
    {:else if mod.hasSettings}
      <button
        class="d-act"
        disabled
        title="Silo edits these once they're written to modSettings/ — usually after you launch the mod once."
      >
        ⚙ Has settings
      </button>
    {/if}
    <button class="d-act" onclick={onFindInBrowse}>⌕ Find in Browse</button>
    <button class="d-act" onclick={() => revealInFolder(mod.path).catch(() => {})}>
      📂 Reveal
    </button>
  </div>

  {#if catalog}
    <div class="d-cat-info">
      {#if catalogNewer && !foreign}
        <div class="d-upd">
          <span>⬆ Update available — catalog has <b>v{catalog.latestVersion}</b>{mod.version ? ` (you have v${mod.version})` : ""}</span>
          {#if catalog.pageUrl}
            <button class="d-upd-link" onclick={() => openExternal(catalog!.pageUrl!)}>View source ↗</button>
          {/if}
        </div>
      {:else if catalog.latestVersion}
        <div class="d-uptodate">✓ Up to date <span class="d-mut">· catalog latest v{catalog.latestVersion}</span></div>
      {/if}

      {#if summaryText}
        <div class="d-summary">{summaryText}</div>
        {#if summaryHasMore}
          <button class="d-readmore" onclick={() => (showFullDesc = true)}>Read more…</button>
        {/if}
      {/if}

      {#if catalog.sources?.length}
        <div class="d-sources">
          <span class="d-src-label">Available on</span>
          {#each catalog.sources as s (s.source + s.sourceUrl)}
            <button class="d-src" onclick={() => openExternal(s.sourceUrl)}>
              {sourceLabel(s.source)}{s.version ? ` · ${s.version}` : ""} ↗
            </button>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  <div class="d-meta">
    <div><span>Category</span>{mod.category}{mod.subcategory ? ` · ${mod.subcategory}` : ""}</div>
    <div><span>Size</span><span class="tnum">{sizeLabel}</span></div>
    <div><span>Type</span>{mod.isMap ? "Map" : mod.kind === "dir" ? "Folder" : "Zip"}</div>
    {#if mod.storeItemCount > 0}<div><span>Shop items</span><span class="tnum">{mod.storeItemCount}</span></div>{/if}
    {#if mod.scriptCount > 0}<div><span>Scripts</span><span class="tnum">{mod.scriptCount}</span></div>{/if}
    <div><span>Multiplayer</span>{mod.mpSupported ? "Supported" : "Not marked"}</div>
    {#if organized}<div><span>Library</span>Organized</div>{/if}
  </div>

  <div class="d-section">
    <div class="d-label">
      Category {#if isOverridden}<span class="d-ovr">manual</span>{/if}
    </div>
    <div class="d-cat">
      <select class="d-cat-sel" bind:value={catDraft}>
        {#each categories as c (c)}<option value={c}>{c}</option>{/each}
      </select>
      <input class="d-cat-sub" placeholder="Subcategory (optional)" bind:value={subDraft} />
    </div>
    <div class="d-cat-actions">
      <button class="d-cat-save" disabled={!catDirty} onclick={() => onSetCategory(catDraft, subDraft.trim() || null)}>
        Save
      </button>
      {#if isOverridden}
        <button class="d-cat-reset" onclick={onResetCategory}>↺ Reset to auto</button>
      {/if}
    </div>
  </div>

  <ModStatus {mod} />

  <ModCuration
    techName={mod.techName}
    {curation}
    {tags}
    {onCurationChange}
    {onTagsChange}
    {onFilterTag}
  />

  <ModSourceLink {mod} {source} {onSourceChange} {onRepoChange} />

  <ModRepoLink {mod} {repo} {onRepoChange} {onInstalled} />

  <ProvenanceCheck {mod} />

  {#if mod.dependencies.length > 0}
    <div class="d-section">
      <div class="d-label">Dependencies ({mod.dependencies.length})</div>
      {#each mod.dependencies as dep (dep)}
        <div class="d-dep">
          <span class="d-dep-name tnum">{dep}</span>
          {#if libraryTechNames.has(dep)}
            <span class="d-ok">in library</span>
          {:else}
            <span class="d-missing">missing</span>
          {/if}
        </div>
      {/each}
    </div>
  {/if}

  {#if mod.uniqueType}
    <div class="d-section">
      <div class="d-label">uniqueType</div>
      <div class="d-uniq tnum">{mod.uniqueType}</div>
    </div>
  {/if}

  {#if myConflicts.length > 0}
    <div class="d-section">
      <div class="d-label">Conflicts with the active set ({myConflicts.length})</div>
      {#each myConflicts as c (c.kind + c.name)}
        <div class="d-conf" class:crit={c.severity === "critical"}>
          <span class="d-conf-sev">{c.severity}</span>
          {c.kind} “{c.name}” — with {c.mods.filter((m) => m !== label).join(", ")}
        </div>
      {/each}
    </div>
  {/if}
</aside>

{#if showFullDesc && catalog}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="fd-back" onclick={() => (showFullDesc = false)}></div>
  <div class="fd-modal">
    <div class="fd-head">
      <span class="fd-title">{label}</span>
      <button class="d-x" onclick={() => (showFullDesc = false)} aria-label="Close">✕</button>
    </div>
    <div class="fd-body">{catalog.descriptionFull || catalog.description}</div>
  </div>
{/if}

<style>
  .drawer {
    position: fixed;
    z-index: 50;
    top: var(--topbar-h, 0px);
    right: 0;
    height: calc(100vh - var(--topbar-h, 0px));
    width: 420px;
    max-width: calc(100vw - 40px);
    overflow-y: auto;
    background: var(--surface-raised);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    padding: 18px;
    scrollbar-width: thin;
  }
  .d-top {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  /* Cover image — matches the Browse drawer's .drawer-img so both drawers share a header. */
  .d-cover {
    width: 100%;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    margin-top: 12px;
    display: block;
  }
  .d-cover-sq {
    height: 120px;
    object-fit: contain;
    background: var(--surface-raised);
  }
  .d-head {
    flex: 1 1 auto;
    min-width: 0;
  }
  .d-title {
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 600;
    line-height: 1.2;
  }
  .d-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-top: 2px;
  }
  .d-tech {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    opacity: 0.7;
    margin-top: 2px;
  }
  .d-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    font-size: 0.875rem;
  }
  .d-x:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
  }
  .d-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 14px 0;
  }
  .d-act {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-muted);
    padding: 6px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .d-act:hover {
    color: var(--text);
  }
  .d-act.on {
    color: var(--primary-text);
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    background: color-mix(in srgb, var(--primary) 10%, transparent);
  }
  .d-act:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .d-act:disabled:hover {
    color: var(--text-muted);
  }
  .d-cat-info {
    padding: 12px 0;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .d-upd {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    font-size: var(--fs-xs);
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 40%, var(--border));
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    font-variant-numeric: tabular-nums;
  }
  .d-upd-link {
    border: none;
    background: transparent;
    color: var(--accent-text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
  }
  .d-foreign {
    margin: 12px 0 0;
    padding: 11px 12px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--primary) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--primary) 40%, var(--border));
  }
  .d-foreign-msg {
    font-size: var(--fs-xs);
    line-height: 1.5;
    color: var(--text);
  }
  .d-foreign-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 10px;
  }
  .d-adopt {
    border: 1px solid var(--primary);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 700;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .d-adopt:hover {
    background: var(--primary-hover);
  }
  .d-restore {
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .d-restore:hover {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
  }
  .d-uptodate {
    font-size: var(--fs-xs);
    color: var(--primary-text);
  }
  .d-mut {
    color: var(--text-muted);
  }
  .d-summary {
    font-size: var(--fs-xs);
    line-height: 1.55;
    color: var(--text);
    white-space: pre-wrap;
    display: -webkit-box;
    -webkit-line-clamp: 20;
    line-clamp: 20;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .d-readmore {
    align-self: flex-start;
    border: none;
    background: transparent;
    color: var(--primary-text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .d-sources {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }
  .d-src-label {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
  }
  .d-src {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 4px 9px;
    border-radius: var(--radius-sm);
    font: inherit;
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .d-src:hover {
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
  }
  .fd-back {
    position: fixed;
    inset: 0;
    z-index: 60;
    background: color-mix(in srgb, var(--bg) 55%, transparent);
  }
  .fd-modal {
    position: fixed;
    z-index: 61;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(680px, calc(100vw - 40px));
    max-height: min(80vh, 720px);
    display: flex;
    flex-direction: column;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
  }
  .fd-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border);
  }
  .fd-title {
    font-family: var(--font-display);
    font-weight: 600;
  }
  .fd-body {
    padding: 14px;
    overflow-y: auto;
    font-size: var(--fs-sm);
    line-height: 1.6;
    white-space: pre-wrap;
    color: var(--text);
    scrollbar-width: thin;
  }
  .d-meta {
    display: grid;
    gap: 6px;
    font-size: var(--fs-xs);
    padding: 10px 0;
    border-top: 1px solid var(--border);
  }
  .d-meta > div {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .d-meta span:first-child {
    color: var(--text-muted);
  }
  .d-meta > div > span:first-child {
    color: var(--text-muted);
  }
  .d-section {
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .d-label {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
    margin-bottom: 8px;
  }
  .d-ovr {
    font-size: var(--fs-xs);
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    padding: 1px 6px;
    border-radius: 999px;
    margin-left: 6px;
    vertical-align: middle;
  }
  .d-cat {
    display: flex;
    gap: 8px;
  }
  .d-cat-sel,
  .d-cat-sub {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }
  .d-cat-sel {
    flex: 0 0 auto;
  }
  .d-cat-sub {
    flex: 1 1 auto;
    min-width: 0;
  }
  .d-cat-actions {
    display: flex;
    gap: 8px;
    margin-top: 8px;
  }
  .d-cat-save {
    padding: 6px 14px;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-size: 0.82rem;
    font-weight: 600;
    cursor: pointer;
  }
  .d-cat-save:disabled {
    opacity: 0.5;
    cursor: default;
    background: var(--border);
    color: var(--text-muted);
  }
  .d-cat-reset {
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.82rem;
    cursor: pointer;
  }
  .d-dep,
  .d-conf {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: var(--fs-xs);
    padding: 4px 0;
  }
  .d-dep-name {
    color: var(--text);
  }
  .d-ok {
    color: var(--primary-text);
    font-size: var(--fs-xs);
  }
  .d-missing {
    color: var(--warn-text);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .d-uniq {
    font-size: var(--fs-xs);
    color: var(--warn-text);
  }
  .d-conf {
    display: block;
    border-left: 3px solid var(--warn-text);
    padding: 6px 10px;
    margin-bottom: 6px;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--warn-text) 6%, transparent);
    font-size: var(--fs-xs);
  }
  .d-conf.crit {
    border-left-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 6%, transparent);
  }
  .d-conf-sev {
    font-weight: 700;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    color: var(--warn-text);
    margin-right: 6px;
  }
  .d-conf.crit .d-conf-sev {
    color: var(--danger);
  }
</style>
