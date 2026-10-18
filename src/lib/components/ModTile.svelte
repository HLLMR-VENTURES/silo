<script lang="ts" module>
  // Shared icon cache (same session-wide cache the row view uses).
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
    onFindInBrowse,
    onContextMenu,
    tags = [],
    overridden = false,
    organized = false,
    active = false,
    hasSettings = false,
    hasUpdate = false,
  }: {
    mod: ModEntry;
    curation: CurationRow;
    onToggle: (flag: Flag) => void;
    onEditCategory: (ev: MouseEvent) => void;
    onToggleActive: () => void;
    onOpenSettings: () => void;
    onOpenDetail: () => void;
    onFindInBrowse: () => void;
    onContextMenu?: (ev: MouseEvent) => void;
    tags?: string[];
    overridden?: boolean;
    organized?: boolean;
    active?: boolean;
    hasSettings?: boolean;
    hasUpdate?: boolean;
  } = $props();

  const verdict = $derived(getVerdict(mod));

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
    if (bytes >= 1024 * 1024 * 1024) return (bytes / 1024 ** 3).toFixed(1) + " GB";
    if (bytes >= 1024 * 1024) return (bytes / 1024 ** 2).toFixed(0) + " MB";
    if (bytes >= 1024) return (bytes / 1024).toFixed(0) + " KB";
    return bytes + " B";
  }
  const initial = $derived((mod.title ?? mod.techName).trim().charAt(0).toUpperCase() || "?");

  // Keep clicks on the interactive bits from also opening the drawer.
  function stop(fn: () => void) {
    return (e: MouseEvent) => {
      e.stopPropagation();
      fn();
    };
  }
</script>

<!-- svelte-ignore a11y_no_static_element_interactions, a11y_click_events_have_key_events -->
<div
  class="card"
  class:active
  class:broken={curation.broken}
  class:dimmed={curation.hidden || (organized && !active)}
  class:has-error={!!mod.error}
  oncontextmenu={onContextMenu}
>
  <!-- Header: the active on/off switch (green when on). -->
  <div class="tc-head">
    <button
      class="tc-switch"
      class:on={active}
      onclick={stop(onToggleActive)}
      title={active ? "Active — click to park" : "Parked — click to activate"}
      aria-label="Toggle active"
    >
      <span class="knob"></span>
    </button>
    <span class="tc-state" class:on={active}>{active ? "Active" : "Parked"}</span>
    {#if organized && !active}<span class="tc-parked">parked</span>{/if}
    <span class="tc-sp"></span>
    {#if hasSettings}
      <button class="tc-gear" onclick={stop(onOpenSettings)} title="Edit this mod's settings" aria-label="Edit this mod's settings"><Icon name="modSettings" /></button>
    {/if}
  </div>

  <!-- Main region: square icon (left) + info + a far-right vertical action stack. Click opens the drawer. -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="tc-main" onclick={onOpenDetail}>
    {#if iconSrc}
      <img class="tc-icon img" src={iconSrc} alt="" loading="lazy" />
    {:else}
      <div class="tc-icon" class:map={mod.isMap}>{initial}</div>
    {/if}

    <div class="tc-info">
      <div class="tc-title" title={mod.title ?? mod.techName}>{mod.title ?? mod.techName}</div>
      <div class="tc-author">{mod.author ?? "Unknown author"}</div>
      <div class="tc-meta tnum">
        {#if mod.version}<span>v{mod.version.replace(/^v/i, "")}</span><span class="dot">·</span>{/if}
        <span>{sizeLabel}</span>
        {#if curation.rating > 0}<span class="rating" role="img" aria-label={`${curation.rating} of 5 stars`}>{#each Array.from({ length: curation.rating }) as _, i (i)}<Icon name="favorite" size={12} filled />{/each}</span>{/if}
      </div>
      <div class="tc-flags">
        {#if verdict?.status === "verified"}
          <span class="tc-flag ok" title="Verified against the published build"><Icon name="verifiedBuild" size={12} /> Verified</span>
        {:else if verdict?.status === "modified"}
          <span class="tc-flag mod" title="Modified from the published build"><Icon name="modified" size={12} /> Modified</span>
        {/if}
        {#if hasUpdate}<span class="tc-flag upd" title="An update is available"><Icon name="update" size={12} /> Update</span>{/if}
      </div>
      <div class="tc-links">
        <button class="tc-link" onclick={stop(onFindInBrowse)}>Find in Browse <Icon name="external" size={12} /></button>
      </div>
    </div>

    <div class="tc-actions">
      <button
        class="tc-act star"
        class:on={curation.favorite}
        title={curation.favorite ? "Remove favorite" : "Favorite"}
        aria-label={curation.favorite ? "Remove favorite" : "Favorite"}
        onclick={stop(() => onToggle("favorite"))}
      >
        <Icon name="favorite" filled={curation.favorite} />
      </button>
      <button
        class="tc-act"
        class:on={curation.hidden}
        title={curation.hidden ? "Unhide" : "Hide"}
        aria-label={curation.hidden ? "Unhide" : "Hide"}
        onclick={stop(() => onToggle("hidden"))}
      >
        <Icon name="hidden" />
      </button>
      <button
        class="tc-act warn"
        class:on={curation.broken}
        title={curation.broken ? "Clear broken flag" : "Mark broken/unfinished"}
        aria-label={curation.broken ? "Clear broken flag" : "Mark broken/unfinished"}
        onclick={stop(() => onToggle("broken"))}
      >
        <Icon name="problem" />
      </button>
    </div>
  </div>

  <!-- Footer: pills (fills the bottom band, under a full-width divider). -->
  <div class="tc-pills">
    <button
      class="pill cat"
      class:overridden
      title={overridden ? "Custom category — click to change" : "Click to reassign category"}
      onclick={(e) => {
        e.stopPropagation();
        onEditCategory(e);
      }}
    >
      {mod.category}{mod.subcategory ? " · " + mod.subcategory : ""}
    </button>
    {#if mod.isMap}<span class="pill map">Map</span>{/if}
    {#if mod.mpSupported}<span class="pill mp">MP</span>{/if}
    {#if mod.scriptCount > 0}
      <span class="pill" title="Injects {mod.scriptCount} Lua script(s)">
        {mod.scriptCount} script{mod.scriptCount > 1 ? "s" : ""}
      </span>
    {/if}
    {#if mod.uniqueType}<span class="pill warn" title="uniqueType: {mod.uniqueType}">unique</span>{/if}
    {#if mod.dependencies.length > 0}
      <span class="pill dep" title={mod.dependencies.join(", ")}>
        {mod.dependencies.length} dep{mod.dependencies.length > 1 ? "s" : ""}
      </span>
    {/if}
    {#if mod.kind === "dir"}<span class="pill dev" title="Unpacked folder mod (dev build)">Dev</span>{/if}
    {#if mod.ignoredDigitPrefix}
      <span class="pill warn" title="Name starts with a digit — the game ignores this mod">ignored</span>
    {/if}
    {#if mod.error}<span class="pill err" title={mod.error}>error</span>{/if}
    {#each tags.slice(0, 2) as t (t)}<span class="pill tag">#{t}</span>{/each}
  </div>
</div>

<style>
  .card {
    display: flex;
    flex-direction: column;
    height: 100%;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    overflow: hidden;
    transition: transform 0.13s ease, box-shadow 0.13s ease, border-color 0.13s ease;
  }
  .card:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow-2, 0 12px 28px rgba(0, 0, 0, 0.18));
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
  }
  /* Active = bright-green border + ring. */
  .card.active {
    border-color: var(--primary);
    box-shadow: inset 0 0 0 1px var(--primary);
  }
  .card.broken {
    box-shadow: inset 3px 0 0 var(--danger);
  }
  .card.dimmed {
    opacity: 0.55;
  }
  .card.has-error {
    background: color-mix(in srgb, var(--danger) 5%, transparent);
  }

  .tc-head {
    display: flex;
    align-items: center;
    gap: 9px;
    padding: 7px 12px;
    border-bottom: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-raised) 60%, transparent);
  }
  .tc-switch {
    flex: 0 0 auto;
    position: relative;
    width: 38px;
    height: 21px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--surface-raised);
    cursor: pointer;
    padding: 0;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .tc-switch .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 15px;
    height: 15px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: transform 0.15s ease, background 0.15s ease;
  }
  .tc-switch.on {
    background: color-mix(in srgb, var(--primary) 30%, transparent);
    border-color: var(--primary);
  }
  .tc-switch.on .knob {
    transform: translateX(17px);
    background: var(--primary);
  }
  .tc-state {
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  .tc-state.on {
    color: var(--primary-text);
  }
  .tc-parked {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    background: color-mix(in srgb, var(--text-muted) 12%, transparent);
    border-radius: 999px;
    padding: 1px 8px;
  }
  .tc-sp {
    flex: 1 1 auto;
  }
  .tc-gear {
    border: none;
    background: transparent;
    color: var(--primary-text);
    cursor: pointer;
    display: inline-grid;
    place-items: center;
  }

  .tc-main {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    gap: 12px;
    padding: 12px;
    cursor: pointer;
  }
  .tc-icon {
    flex: 0 0 auto;
    align-self: stretch;
    aspect-ratio: 1 / 1;
    height: 100%;
    max-height: 108px;
    border-radius: var(--radius-sm);
    display: grid;
    place-items: center;
    font-family: var(--font-display);
    font-size: var(--fs-2xl);
    font-weight: 600;
    color: var(--on-primary);
    background: linear-gradient(135deg, var(--primary), var(--primary-hover));
    box-shadow: var(--shadow-1);
  }
  .tc-icon.map {
    background: linear-gradient(135deg, var(--earth), var(--earth-strong));
    color: var(--on-earth);
  }
  .tc-icon.img {
    object-fit: cover;
    background: var(--surface-raised);
    color: transparent;
  }
  .tc-info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .tc-title {
    font-weight: 600;
    font-size: 0.95rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tc-author {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tc-meta {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .tc-meta .dot {
    opacity: 0.6;
  }
  .rating {
    color: var(--accent-text);
    letter-spacing: -1px;
  }
  .tc-flags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }
  .tc-flag {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
    border: 1px solid var(--border);
  }
  .tc-flag.ok {
    color: var(--primary-text);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
  }
  .tc-flag.mod {
    color: var(--warn-text);
    border-color: color-mix(in srgb, var(--warn-text) 40%, var(--border));
  }
  .tc-flag.upd {
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .tc-links {
    margin-top: auto;
    padding-top: 4px;
  }
  .tc-link {
    border: none;
    background: transparent;
    color: var(--primary-text);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .tc-link:hover {
    text-decoration: underline;
  }
  .tc-actions {
    flex: 0 0 auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    align-items: center;
  }
  .tc-act {
    border: none;
    background: transparent;
    color: var(--text-muted);
    display: inline-grid;
    place-items: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .tc-act:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
  }
  .tc-act.star.on {
    color: var(--accent-text);
  }
  .tc-act.warn.on {
    color: var(--danger);
  }
  .tc-act.on:not(.star):not(.warn) {
    color: var(--info);
  }

  .tc-pills {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 12px;
    border-top: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-raised) 45%, transparent);
  }
  .pill {
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    color: var(--text-muted);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .pill.cat {
    color: var(--primary-text);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 30%, var(--border));
    cursor: pointer;
    font-family: inherit;
  }
  .pill.cat:hover {
    background: color-mix(in srgb, var(--primary) 22%, transparent);
  }
  .pill.cat.overridden {
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .pill.map {
    color: var(--earth-text);
    border-color: color-mix(in srgb, var(--earth) 40%, var(--border));
  }
  .pill.mp {
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 40%, var(--border));
  }
  .pill.warn {
    color: var(--warn-text);
    border-color: color-mix(in srgb, var(--warn-text) 45%, var(--border));
  }
  .pill.dep {
    color: var(--accent-text);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
  }
  .pill.dev {
    color: var(--accent-text);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
  }
  .pill.err {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }
  .pill.tag {
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 35%, var(--border));
  }
</style>
