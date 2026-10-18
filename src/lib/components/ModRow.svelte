<script lang="ts" module>
  // Shared across all rows so scrolling back doesn't re-request an icon we've
  // already loaded this session. (The Rust side also disk-caches decoded PNGs.)
  const iconCache = new Map<string, string>();
</script>

<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import type { ModEntry, CurationRow } from "../types";
  import { getModIcon } from "../api";
  import { getVerdict } from "../provenanceCache.svelte";

  type Flag = "favorite" | "hidden" | "broken";
  let {
    mod,
    curation,
    onToggle,
    onEditCategory,
    onToggleActive,
    onOpenSettings,
    onOpenDetail,
    onContextMenu,
    tags = [],
    overridden = false,
    organized = false,
    active = false,
    hasSettings = false,
  }: {
    mod: ModEntry;
    curation: CurationRow;
    onToggle: (flag: Flag) => void;
    onEditCategory: (ev: MouseEvent) => void;
    onToggleActive: () => void;
    onOpenSettings: () => void;
    onOpenDetail: () => void;
    onContextMenu?: (ev: MouseEvent) => void;
    tags?: string[];
    overridden?: boolean;
    organized?: boolean;
    active?: boolean;
    hasSettings?: boolean;
  } = $props();

  // At-a-glance integrity badge, populated once this mod is verified (drawer or sweep).
  const verdict = $derived(getVerdict(mod));

  // Lazy icon: rows only mount when scrolled into view (virtualized), so this
  // fetches on-screen icons only. Guarded so a slow response can't land on a row
  // that's since been reused for a different mod.
  let iconSrc = $state<string | null>(null);
  $effect(() => {
    const m = mod;
    if (!m.iconFilename) {
      iconSrc = null;
      return;
    }
    const hit = iconCache.get(m.path);
    if (hit) {
      iconSrc = hit;
      return;
    }
    iconSrc = null;
    getModIcon(m.path, m.kind, m.iconFilename).then((url) => {
      if (url && mod.path === m.path) {
        iconCache.set(m.path, url);
        iconSrc = url;
      }
    });
  });

  const sizeLabel = $derived(formatSize(mod.size));
  function formatSize(bytes: number): string {
    if (bytes >= 1024 * 1024 * 1024)
      return (bytes / 1024 ** 3).toFixed(1) + " GB";
    if (bytes >= 1024 * 1024) return (bytes / 1024 ** 2).toFixed(0) + " MB";
    if (bytes >= 1024) return (bytes / 1024).toFixed(0) + " KB";
    return bytes + " B";
  }

  // Initial for the placeholder tile (icon decode comes in a later slice).
  const initial = $derived(
    (mod.title ?? mod.techName).trim().charAt(0).toUpperCase() || "?",
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="row"
  class:has-error={!!mod.error}
  class:broken={curation.broken}
  class:dimmed={curation.hidden || (organized && !active)}
  oncontextmenu={onContextMenu}
>
  <button
    class="active-dot"
    class:on={active}
    title={active ? "Active — click to park" : "Parked — click to activate"}
    onclick={onToggleActive}
    aria-label="Toggle active"
  ></button>

  {#if iconSrc}
    <img class="tile img" src={iconSrc} alt="" loading="lazy" />
  {:else}
    <div class="tile" class:map={mod.isMap}>{initial}</div>
  {/if}

  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="main" onclick={onOpenDetail}>
    <div class="titleline">
      <span class="title">{mod.title ?? mod.techName}</span>
      {#if mod.version}<span class="ver tnum">v{mod.version.replace(/^v/i, "")}</span>{/if}
      {#if curation.rating > 0}<span class="rating" role="img" aria-label={`${curation.rating} of 5 stars`}>{#each Array.from({ length: curation.rating }) as _, i (i)}<Icon name="favorite" size={11} filled />{/each}</span>{/if}
    </div>
    <div class="sub">
      <span class="author">{mod.author ?? "Unknown author"}</span>
      <span class="dot">·</span>
      <span class="tech">{mod.techName}</span>
      {#each tags.slice(0, 3) as t (t)}<span class="rowtag">#{t}</span>{/each}
    </div>
  </div>

  <div class="badges">
    <button
      class="badge cat"
      class:overridden
      title={overridden ? "Custom category — click to change" : "Click to reassign category"}
      onclick={(e) => onEditCategory(e)}
    >
      {mod.category}{mod.subcategory ? " · " + mod.subcategory : ""}
    </button>
    {#if mod.kind === "dir"}
      <span
        class="badge dev"
        title="Unpacked folder mod (dev build) — Silo shows it but leaves it unmanaged"
      >Dev</span>
    {/if}
    {#if organized && !active}<span class="badge parked">parked</span>{/if}
    {#if verdict?.status === "verified"}
      <span class="badge verified" title="Verified against the published build"><Icon name="verifiedBuild" size={12} /> Verified</span>
    {:else if verdict?.status === "modified"}
      <span class="badge modded" title="Modified from the published build"><Icon name="modified" size={12} /> Modified</span>
    {/if}
    {#if mod.isMap}<span class="badge map">Map</span>{/if}
    {#if mod.mpSupported}<span class="badge mp">MP</span>{/if}
    {#if mod.scriptCount > 0}
      <span class="badge script" title="Injects {mod.scriptCount} Lua script(s)">
        {mod.scriptCount} script{mod.scriptCount > 1 ? "s" : ""}
      </span>
    {/if}
    {#if mod.uniqueType}
      <span class="badge unique" title="uniqueType: {mod.uniqueType}">unique</span>
    {/if}
    {#if mod.dependencies.length > 0}
      <span class="badge dep" title={mod.dependencies.join(", ")}>
        {mod.dependencies.length} dep{mod.dependencies.length > 1 ? "s" : ""}
      </span>
    {/if}
    {#if mod.ignoredDigitPrefix}
      <span class="badge warn" title="Name starts with a digit — the game ignores this mod">
        ignored
      </span>
    {/if}
    {#if mod.error}
      <span class="badge err" title={mod.error}>error</span>
    {/if}
  </div>

  <div class="actions">
    {#if hasSettings}
      <button
        class="act on"
        title="Edit this mod's settings"
        aria-label="Edit this mod's settings"
        onclick={onOpenSettings}
      >
        <Icon name="modSettings" />
      </button>
    {/if}
    <button
      class="act star"
      class:on={curation.favorite}
      title={curation.favorite ? "Remove favorite" : "Favorite"}
      aria-label={curation.favorite ? "Remove favorite" : "Favorite"}
      onclick={() => onToggle("favorite")}
    >
      <Icon name="favorite" filled={curation.favorite} />
    </button>
    <button
      class="act"
      class:on={curation.hidden}
      title={curation.hidden ? "Unhide" : "Hide"}
      aria-label={curation.hidden ? "Unhide" : "Hide"}
      onclick={() => onToggle("hidden")}
    >
      <Icon name="hidden" />
    </button>
    <button
      class="act warn"
      class:on={curation.broken}
      title={curation.broken ? "Clear broken flag" : "Mark broken/unfinished"}
      aria-label={curation.broken ? "Clear broken flag" : "Mark broken/unfinished"}
      onclick={() => onToggle("broken")}
    >
      <Icon name="problem" />
    </button>
  </div>

  <div class="size tnum">{sizeLabel}</div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 14px;
    height: 100%;
    padding: 0 16px;
    border-bottom: 1px solid var(--border);
  }
  .row:hover {
    background: color-mix(in srgb, var(--primary) 6%, transparent);
  }
  .active-dot {
    flex: 0 0 auto;
    width: 12px;
    height: 12px;
    padding: 0;
    border-radius: 50%;
    border: 2px solid var(--border);
    background: transparent;
    transition: background 0.12s ease, border-color 0.12s ease;
  }
  .active-dot:hover {
    border-color: var(--primary);
  }
  .active-dot.on {
    background: var(--primary);
    border-color: var(--primary);
  }
  .badge.parked {
    color: var(--text-muted);
    background: color-mix(in srgb, var(--text-muted) 12%, transparent);
  }
  .tile {
    flex: 0 0 auto;
    width: 46px;
    height: 46px;
    border-radius: var(--radius-sm);
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    font-weight: 600;
    color: var(--on-primary);
    background: linear-gradient(135deg, var(--primary), var(--primary-hover));
    box-shadow: var(--shadow-1);
  }
  .tile.map {
    background: linear-gradient(135deg, var(--earth), var(--earth-strong));
    color: var(--on-earth);
  }
  .tile.img {
    object-fit: cover;
    background: var(--surface-raised);
    color: transparent;
  }
  .main {
    flex: 1 1 auto;
    min-width: 0;
    cursor: pointer;
  }
  .rating {
    color: var(--accent-text);
    font-size: var(--fs-xs);
    letter-spacing: -1px;
  }
  .rowtag {
    color: var(--info);
    font-size: var(--fs-xs);
    opacity: 0.85;
  }
  .titleline {
    display: flex;
    align-items: baseline;
    gap: 8px;
  }
  .title {
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ver {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .sub {
    color: var(--text-muted);
    font-size: var(--fs-xs);
    display: flex;
    gap: 6px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tech {
    opacity: 0.7;
  }
  .badges {
    flex: 0 0 auto;
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .badge {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--text-muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .badge.cat {
    color: var(--primary-text);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 30%, var(--border));
    cursor: pointer;
    font-family: inherit;
  }
  .badge.cat:hover {
    background: color-mix(in srgb, var(--primary) 22%, transparent);
  }
  .badge.cat.overridden {
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .badge.map {
    color: var(--earth-text);
    border-color: color-mix(in srgb, var(--earth) 40%, var(--border));
  }
  .badge.verified {
    color: var(--primary-text);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
  }
  .badge.modded {
    color: var(--warn-text);
    background: color-mix(in srgb, var(--warn-text) 14%, transparent);
    border-color: color-mix(in srgb, var(--warn-text) 40%, var(--border));
  }
  .badge.mp {
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 40%, var(--border));
  }
  .badge.unique,
  .badge.warn {
    color: var(--warn-text);
    border-color: color-mix(in srgb, var(--warn-text) 45%, var(--border));
  }
  .badge.err {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }
  .badge.dep {
    color: var(--accent-text);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .badge.dev {
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .size {
    flex: 0 0 auto;
    width: 64px;
    text-align: right;
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .has-error {
    background: color-mix(in srgb, var(--danger) 5%, transparent);
  }
  .actions {
    flex: 0 0 auto;
    display: flex;
    gap: 2px;
    align-items: center;
  }
  .act {
    border: none;
    background: transparent;
    color: var(--text-muted);
    display: inline-grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    opacity: 0;
    transition: opacity 0.12s ease, background 0.12s ease, color 0.12s ease;
  }
  .row:hover .act {
    opacity: 0.75;
  }
  .act:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    opacity: 1;
  }
  /* Star + any active flag stay visible even when not hovering. */
  .act.star,
  .act.on {
    opacity: 1;
  }
  .act.star.on {
    color: var(--accent-text);
  }
  .act.warn.on {
    color: var(--danger);
  }
  .act.on:not(.star):not(.warn) {
    color: var(--info);
  }
  .row.broken {
    box-shadow: inset 3px 0 0 var(--danger);
  }
  .row.dimmed {
    opacity: 0.5;
  }
</style>
