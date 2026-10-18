<script lang="ts">
  // The shared, two-line search/filter bar used by both Browse and the Library. It's
  // presentation + binding only: it renders the controls bound to state the parent owns and
  // calls `onChange` after a discrete change (so Browse can re-query; the Library's derived
  // filter recomputes on its own). Search typing is separate (`onSearchInput`) so Browse can
  // debounce it while the Library filters live.
  //
  // Top line:    category (parent · child) · search · sort · direction
  // Bottom line: facets (in `facetOrder`) · year · Clear all
  import Icon from "../ui/Icon.svelte";
  import { tooltip } from "../ui/tooltip";

  export interface CatChild {
    name: string;
    count: number;
    value: string;
  }
  export interface CatNode {
    name: string;
    count: number;
    children: CatChild[];
  }
  export interface FacetValue {
    value: string;
    count: number;
  }

  interface Props {
    // Category tree (parent → children). A single-level category is a childless parent.
    catTree?: CatNode[];
    parentCategory?: string;
    childCategory?: string;
    // Search
    query?: string;
    searchPlaceholder?: string;
    /** False when the app header's search drives `query` instead (the box isn't shown). */
    showSearch?: boolean;
    // Sort
    sort?: string;
    sortOptions: { value: string; label: string }[];
    sortDir?: "asc" | "desc";
    /** Browse's catalog API sorts are fixed-direction, so it passes false to disable the arrow. */
    dirEnabled?: boolean;
    // Facets (bottom line)
    facetOrder?: string[];
    facetData?: Record<string, FacetValue[]>;
    facetLabels?: Record<string, string>;
    activeTags?: string[];
    availableBy?: number | null;
    /** The "available by year" input needs catalog year data; the Library hides it. */
    showYear?: boolean;
    /** The Library keeps its own Clear-all in its toggle row, so it hides this one. */
    showClear?: boolean;
    // Meta
    filterCount?: number;
    /** Called after a discrete change (category / sort / facet / year / direction). */
    onChange?: () => void;
    /** Called on each search keystroke (parent decides whether to debounce). */
    onSearchInput?: () => void;
    onClear?: () => void;
  }

  let {
    catTree = [],
    parentCategory = $bindable(""),
    childCategory = $bindable(""),
    query = $bindable(""),
    searchPlaceholder = "Search…",
    showSearch = true,
    sort = $bindable(""),
    sortOptions,
    sortDir = $bindable("desc"),
    dirEnabled = false,
    facetOrder = [],
    facetData = {},
    facetLabels = {},
    activeTags = $bindable([]),
    availableBy = $bindable(null),
    showYear = true,
    showClear = true,
    filterCount = 0,
    onChange,
    onSearchInput,
    onClear,
  }: Props = $props();

  const childList = $derived(catTree.find((n) => n.name === parentCategory)?.children ?? []);
  const facetGroups = $derived(facetOrder.filter((ns) => (facetData[ns] ?? []).length > 0));

  function prettify(v: string): string {
    return v.replace(/-/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
  }
  function selectedFor(ns: string): string {
    return activeTags.find((t) => t.split(":")[0] === ns) ?? "";
  }
  function setParent(p: string) {
    parentCategory = p;
    childCategory = "";
    onChange?.();
  }
  function setChild(v: string) {
    childCategory = v;
    onChange?.();
  }
  function setFacet(ns: string, tag: string) {
    activeTags = [...activeTags.filter((t) => t.split(":")[0] !== ns), ...(tag ? [tag] : [])];
    onChange?.();
  }
  function setAvailableBy(v: string) {
    const n = parseInt(v, 10);
    availableBy = Number.isFinite(n) && n >= 1900 && n <= 2100 ? n : null;
    onChange?.();
  }
  function toggleDir() {
    if (!dirEnabled) return;
    sortDir = sortDir === "desc" ? "asc" : "desc";
    onChange?.();
  }
</script>

<div class="filterbar">
  <div class="fb-row fb-top">
    {#if catTree.length > 0}
      <select
        class="fb-sel"
        class:on={parentCategory !== ""}
        value={parentCategory}
        onchange={(e) => setParent(e.currentTarget.value)}
        aria-label="Category"
      >
        <option value="">All categories</option>
        {#each catTree as n (n.name)}
          <option value={n.name}>{n.name} ({n.count.toLocaleString()})</option>
        {/each}
      </select>
      {#if childList.length > 0}
        <select
          class="fb-sel"
          class:on={childCategory !== ""}
          value={childCategory}
          onchange={(e) => setChild(e.currentTarget.value)}
          aria-label="Subcategory"
        >
          <option value="">All {parentCategory}</option>
          {#each childList as c (c.value)}
            <option value={c.value}>{c.name} ({c.count.toLocaleString()})</option>
          {/each}
        </select>
      {/if}
    {/if}
    {#if showSearch}
      <input
        class="fb-search"
        type="search"
        placeholder={searchPlaceholder}
        aria-label={searchPlaceholder || "Search"}
        bind:value={query}
        oninput={() => onSearchInput?.()}
      />
    {:else}
      <span class="fb-spacer"></span>
    {/if}
    <select class="fb-sel fb-sort" bind:value={sort} onchange={() => onChange?.()} aria-label="Sort by">
      {#each sortOptions as o (o.value)}
        <option value={o.value}>{o.label}</option>
      {/each}
    </select>
    <button
      type="button"
      class="fb-dir"
      disabled={!dirEnabled}
      onclick={toggleDir}
      use:tooltip={dirEnabled
        ? `Sort ${sortDir === "desc" ? "descending" : "ascending"}`
        : "Sort direction is coming with a catalog update (silo-api)."}
      aria-label="Sort direction: {sortDir === 'desc' ? 'descending' : 'ascending'}"
    >
      <Icon name={sortDir === "desc" ? "arrowDown" : "arrowUp"} size={14} />
    </button>
  </div>

  {#if facetGroups.length > 0}
    <div class="fb-row fb-bottom">
      {#each facetGroups as ns (ns)}
        <select
          class="fb-sel"
          class:on={selectedFor(ns) !== ""}
          value={selectedFor(ns)}
          onchange={(e) => setFacet(ns, e.currentTarget.value)}
          aria-label="Filter by {facetLabels[ns] ?? ns}"
        >
          <option value="">{facetLabels[ns] ?? ns}: all</option>
          {#each facetData[ns] ?? [] as fv (fv.value)}
            <option value="{ns}:{fv.value}">{prettify(fv.value)} ({fv.count.toLocaleString()})</option>
          {/each}
        </select>
      {/each}
      {#if showYear}
        <input
          class="fb-sel year-in"
          class:on={availableBy != null}
          type="number"
          min="1900"
          max="2100"
          placeholder="Year"
          aria-label="Available by year"
          title="Period-correct: only machines that existed by this year (dated mods only)."
          value={availableBy ?? ""}
          onchange={(e) => setAvailableBy(e.currentTarget.value)}
        />
      {/if}
      {#if showClear && filterCount > 0}
        <button type="button" class="clear-filters" onclick={() => onClear?.()}>Clear all <Icon name="close" size={13} /></button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .filterbar {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin: 6px 0 10px;
  }
  .fb-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .fb-sel {
    padding: 9px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    max-width: 220px;
    cursor: pointer;
  }
  .fb-sel.on {
    color: var(--primary-text);
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    background: color-mix(in srgb, var(--primary) 10%, transparent);
    font-weight: 600;
  }
  .fb-spacer {
    flex: 1;
  }
  .fb-search {
    flex: 1;
    min-width: 200px;
    padding: 9px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }
  /* Sort is ordering, not filtering — contrasting tint (matches the app's primary accent). */
  .fb-sort {
    border-color: color-mix(in srgb, var(--primary) 55%, var(--border));
    background: color-mix(in srgb, var(--primary) 14%, var(--surface-raised));
    color: var(--primary-text);
    font-weight: 700;
  }
  .fb-sort:hover {
    border-color: var(--primary);
  }
  .fb-dir {
    width: 34px;
    padding: 9px 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.95rem;
    line-height: 1;
    cursor: pointer;
  }
  .fb-dir:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .year-in {
    width: 110px;
    max-width: 110px;
    cursor: text;
  }
  .year-in::placeholder {
    color: var(--text-muted);
  }
  .clear-filters {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--fs-sm);
    font-weight: 600;
    cursor: pointer;
    padding: 6px;
  }
  .clear-filters:hover {
    color: var(--primary-text);
    text-decoration: underline;
  }
</style>
