<script lang="ts">
  // The "Browse" tab: discover mods from the canonical SiloAPI catalog, see which
  // you already have, and install new ones straight into the library.
  import { onMount, onDestroy } from "svelte";
  import { inlineError } from "../state/notify.svelte";
  import {
    browseMods,
    siloapiStats,
    siloapiStatus,
    siloapiModDetail,
    siloapiCategories,
    siloapiFacets,
    installRemoteMod,
    onInstallProgress,
    openExternal,
    ghStatus,
    type Facets,
  } from "../api";
  import type {
    BrowseMod,
    SiloStats,
    CatalogModDetail,
    CategoryCount,
    ModSourceOption,
  } from "../types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import ModCard from "./ModCard.svelte";
  import DescModal from "./DescModal.svelte";
  import BrowseDrawer from "./BrowseDrawer.svelte";
  import { browseCacheKey, getBrowseView, setBrowseView } from "../browseCache";
  import FilterBar from "./FilterBar.svelte";
  import { blocked, tryStart } from "../state/ops.svelte";

  interface Props {
    /** Tech names already in the local library, to flag "in library". */
    installed: Set<string>;
    /** Called after a successful install so the parent can rescan. */
    onInstalled: (filename: string) => void;
    /** Route the user to the source-connect UI (Settings) from a source card. */
    onNeedAuth?: () => void;
    /** Jump to an installed mod in the Library (reverse of the library's "Find in Browse"). */
    onFindInLibrary?: (techName: string) => void;
    /** Pre-seed the catalog search (e.g. from a library row's "Find in Browse").
     *  Applied on mount; Browse remounts on every view switch, so this seeds each entry. */
    seed?: string | null;
    /** techName to auto-open once results land, so "Find in Browse" lands on the mod's drawer
     *  rather than just filtering to it. */
    seedTech?: string | null;
  }
  let {
    installed,
    onInstalled,
    onNeedAuth,
    onFindInLibrary,
    seed = null,
    seedTech = null,
  }: Props = $props();

  // Source connection state, for the interactive cards in the drawer.
  let gh = $state<{ connected: boolean; canWrite: boolean }>({
    connected: false,
    canWrite: false,
  });
  async function refreshGh() {
    try {
      const s = await ghStatus();
      gh = { connected: !!s.user, canWrite: s.canWrite };
    } catch {
      gh = { connected: false, canWrite: false };
    }
  }
  refreshGh();

  let query = $state("");
  // `category` is the API filter param, derived from the parent/child dropdowns below.
  let category = $state("");
  let parentCategory = $state("");
  let childCategory = $state("");
  let sort = $state<"popular" | "newest" | "name" | "downloads" | "rating">(
    "newest",
  );
  // Sort direction. The catalog API sorts are fixed-direction today (see silo-api), so on
  // Browse this arrow is disabled until the server honors it; the Library wires it client-side.
  let sortDir = $state<"desc" | "asc">("desc");
  let categories = $state<CategoryCount[]>([]);
  // Split the flat "Parent - Child" catalog categories into a parent → children tree so the
  // bar can show two dropdowns. A single-level category (e.g. "Gameplay") is a childless parent.
  const catTree = $derived.by(() => {
    const map = new Map<
      string,
      { name: string; count: number; children: { name: string; count: number; value: string }[] }
    >();
    for (const c of categories) {
      const i = c.category.indexOf(" - ");
      const parent = i >= 0 ? c.category.slice(0, i) : c.category;
      const child = i >= 0 ? c.category.slice(i + 3) : null;
      let node = map.get(parent);
      if (!node) {
        node = { name: parent, count: 0, children: [] };
        map.set(parent, node);
      }
      node.count += c.count;
      if (child) node.children.push({ name: child, count: c.count, value: c.category });
    }
    return [...map.values()].sort((a, b) => a.name.localeCompare(b.name));
  });
  // Category / sort / facets / year live in the shared <FilterBar>. When any of them change it
  // calls back here to recompute the API `category` param (parent-only, or the child's full
  // "Parent - Child" string) and re-query.
  function onFilterChange() {
    category = childCategory || parentCategory;
    load();
  }
  // Semantic facet filters (silo-api #4). activeTags are "namespace:value" strings, ANDed by
  // the server; availableBy is the period-correct year filter. facets drives a compact
  // dropdown row — one value per facet (picking a new value replaces the prior one).
  let facets = $state<Facets | null>(null);
  let activeTags = $state<string[]>([]);
  let availableBy = $state<number | null>(null);
  // Render facets in a sensible order; anything else the server adds falls in after.
  const FACET_ORDER = ["region", "era", "theme", "brand", "realism"];
  const FACET_LABEL: Record<string, string> = {
    theme: "Theme",
    brand: "Brand",
    region: "Region",
    realism: "Realism",
    era: "Era",
  };
  const filterCount = $derived(
    activeTags.length + (availableBy != null ? 1 : 0) + (parentCategory ? 1 : 0),
  );

  function clearFilters() {
    activeTags = [];
    availableBy = null;
    parentCategory = "";
    childCategory = "";
    category = "";
    load();
  }
  let results = $state<BrowseMod[]>([]);
  let total = $state(0);
  let lastPageFull = $state(false);
  let loadingMore = $state(false);
  let stats = $state<SiloStats | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let installing = $state<string | null>(null);
  let installedNote = $state<string | null>(null);
  let base = $state("");
  // Live download progress per mod id: { done, total } bytes.
  let progress = $state<Record<string, { done: number; total: number | null }>>({});

  // Detail drawer state.
  let detail = $state<CatalogModDetail | null>(null);
  let detailLoading = $state(false);
  // Full-description modal (the drawer shows a clamped snippet). `source` records where
  // the body came from so we can label it honestly.
  let descModal = $state<{
    title: string;
    text: string;
    url: string | null;
    loading: boolean;
    source: "catalog" | "summary";
  } | null>(null);

  /** Open the description modal with the best body the catalog has: the ingested full text
   *  if present, else the short summary. */
  function openDesc(d: CatalogModDetail) {
    const summary = d.description ?? "";
    if (d.descriptionFull) {
      descModal = { title: d.title, text: d.descriptionFull, url: d.pageUrl, loading: false, source: "catalog" };
    } else {
      descModal = { title: d.title, text: summary, url: d.pageUrl, loading: false, source: "summary" };
    }
  }

  let debounce: ReturnType<typeof setTimeout> | null = null;
  let unlisten: UnlistenFn | null = null;

  async function openDetail(m: BrowseMod) {
    detail = null;
    detailLoading = true;
    // Re-read GitHub state so a connection made in Settings reflects in the cards.
    refreshGh();
    try {
      detail = await siloapiModDetail(m.id);
    } catch (e) {
      error = inlineError(e, "Loading the mod's details");
    } finally {
      detailLoading = false;
    }
  }

  /** Click a source button: install it if we can, otherwise open its page. */
  async function useSource(m: { id: string }, s: ModSourceOption) {
    if (s.installable) await install(m, s.source);
    else await openExternal(s.sourceUrl);
  }

  function hasLocally(m: BrowseMod): boolean {
    return m.techName != null && installed.has(m.techName);
  }

  const PAGE = 60;
  // A server predating the `total` field reports 0. Don't render "of 0" or silently
  // hide Load-more — fall back to "the last page came back full, so there's probably
  // more", and just omit the total we don't have.
  const knowsTotal = $derived(total > 0);
  const hasMore = $derived(knowsTotal ? results.length < total : lastPageFull);

  /** Fetch the first page for the current filters, replacing what's shown. Served from the
   *  cross-mount view cache when we've seen this exact filter+sort combo before (no re-poll). */
  async function load() {
    error = null;
    const key = browseCacheKey({ category, tags: activeTags, availableBy, query, sort });
    const cached = getBrowseView(key);
    if (cached) {
      results = cached.results;
      total = cached.total;
      lastPageFull = cached.lastPageFull;
      loading = false;
      return;
    }
    loading = true;
    try {
      const page = await browseMods({
        query: query.trim() || undefined,
        category: category || undefined,
        sort,
        tags: activeTags,
        availableBy,
        limit: PAGE,
        offset: 0,
      });
      results = page.mods;
      total = page.total;
      lastPageFull = page.mods.length === PAGE;
      setBrowseView(key, { results, total, lastPageFull });
    } catch (e) {
      error = inlineError(e, "Loading the catalog");
      results = [];
      total = 0;
      lastPageFull = false;
    } finally {
      loading = false;
    }
  }

  /** Append the next page. Guards against double-firing and against the filters
   *  changing mid-flight, which would splice the wrong results onto the grid. */
  async function loadMore() {
    if (loadingMore || !hasMore) return;
    loadingMore = true;
    const forQuery = query;
    const forCategory = category;
    const forTags = activeTags.join("|");
    const forAvail = availableBy;
    const forSort = sort;
    try {
      const page = await browseMods({
        query: forQuery.trim() || undefined,
        category: forCategory || undefined,
        sort,
        tags: activeTags,
        availableBy,
        limit: PAGE,
        offset: results.length,
      });
      // filters moved on mid-flight → drop this page rather than splice wrong results
      if (
        forQuery !== query ||
        forCategory !== category ||
        forTags !== activeTags.join("|") ||
        forAvail !== availableBy ||
        forSort !== sort
      )
        return;
      results = [...results, ...page.mods];
      total = page.total;
      lastPageFull = page.mods.length === PAGE;
      // Grow the cached view so the next revisit restores everything we've paged in.
      setBrowseView(browseCacheKey({ category, tags: activeTags, availableBy, query, sort }), {
        results,
        total,
        lastPageFull,
      });
    } catch (e) {
      error = inlineError(e, "Loading more mods");
    } finally {
      loadingMore = false;
    }
  }

  function onSearch() {
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(load, 300);
  }

  async function install(m: { id: string }, source?: string) {
    // It adds a mod to the mods folder: a library write, one at a time.
    const op = tryStart("Installing a mod", { exclusive: "library-write" });
    if (!op) {
      error = `${blocked("library-write").reason} Try again when it's done.`;
      return;
    }
    installing = m.id;
    error = null;
    installedNote = null;
    progress = { ...progress, [m.id]: { done: 0, total: null } };
    let filename: string | null = null;
    try {
      filename = await installRemoteMod(m.id, source);
      installedNote = `Installed ${filename}`;
    } catch (e) {
      error = inlineError(e, "Installing the mod");
    } finally {
      installing = null;
      const { [m.id]: _drop, ...rest } = progress;
      progress = rest;
      op.end();
    }
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (filename !== null) onInstalled(filename);
  }

  onMount(async () => {
    unlisten = await onInstallProgress((p) => {
      progress = { ...progress, [p.id]: { done: p.done, total: p.total } };
    });
    try {
      base = await siloapiStatus();
      stats = await siloapiStats();
    } catch {
      /* stats/base are best-effort */
    }
    try {
      // Sort alphabetically (the API returns them by mod count).
      categories = (await siloapiCategories()).sort((a, b) =>
        a.category.localeCompare(b.category, undefined, { sensitivity: "base" }),
      );
    } catch {
      // Older server without /categories — the filter just stays hidden.
    }
    try {
      facets = await siloapiFacets();
    } catch {
      // Older server without /facets — the filter panel just stays empty.
    }
    if (seed && seed.trim()) query = seed.trim();
    await load();
    // "Find in Browse" targets a specific mod — open its drawer once the seeded search lands it.
    if (seedTech) {
      const hit = results.find((m) => m.techName === seedTech);
      if (hit) await openDetail(hit);
    }
  });

  onDestroy(() => unlisten?.());
</script>

<div class="browse">
  <div class="browse-head">
    <div class="bh-title">
      <h2>Browse mods</h2>
      {#if stats}
        <span class="catalog-count">
          {stats.mods.toLocaleString()} mods from {stats.sources.toLocaleString()} ModHub and GitHub listings
        </span>
      {/if}
    </div>
  </div>

  <FilterBar
    {catTree}
    bind:parentCategory
    bind:childCategory
    bind:query
    searchPlaceholder="Search the catalog…"
    bind:sort
    sortOptions={[
      { value: "newest", label: "Recently added / updated" },
      { value: "popular", label: "Popular" },
      { value: "downloads", label: "Most downloaded" },
      { value: "rating", label: "Top rated" },
      { value: "name", label: "Name (A–Z)" },
    ]}
    bind:sortDir
    dirEnabled={false}
    facetOrder={FACET_ORDER}
    facetData={facets?.facets ?? {}}
    facetLabels={FACET_LABEL}
    bind:activeTags
    bind:availableBy
    {filterCount}
    onChange={onFilterChange}
    onSearchInput={onSearch}
    onClear={clearFilters}
  />

  <div class="bh-status">
    {#if results.length > 0}
      <span class="showing tnum">
        Showing {results.length.toLocaleString()}{knowsTotal
          ? ` of ${total.toLocaleString()}`
          : ""}
      </span>
    {/if}
    {#if base}<span class="source-note">Catalog: {base}</span>{/if}
  </div>

  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if installedNote}
    <div class="ok-note">{installedNote} — rescanning library…</div>
  {/if}

  {#if loading && results.length === 0}
    <div class="empty">Loading catalog…</div>
  {:else if results.length === 0}
    <div class="empty">
      No mods found{query ? ` for “${query}”` : ""}{category ? ` in ${category}` : ""}.
    </div>
  {:else}
    <div class="grid">
      {#each results as m (m.id)}
        <ModCard
          {m}
          here={hasLocally(m)}
          installing={installing === m.id}
          progressEntry={progress[m.id]}
          onUseSource={(s) => useSource(m, s)}
          onOpenDetail={() => openDetail(m)}
          onFindInLibrary={m.techName ? () => onFindInLibrary?.(m.techName!) : undefined}
        />
      {/each}
    </div>

    {#if hasMore}
      <div class="more">
        <button class="btn more-btn" disabled={loadingMore} onclick={loadMore}>
          {#if loadingMore}
            Loading…
          {:else if knowsTotal}
            Load {Math.min(PAGE, total - results.length)} more
          {:else}
            Load more
          {/if}
        </button>
      </div>
    {:else if knowsTotal && total > PAGE}
      <div class="more end tnum">That's all {total.toLocaleString()}.</div>
    {/if}
  {/if}

  {#if detailLoading || detail}
    <BrowseDrawer
      {detail}
      {detailLoading}
      installingId={installing}
      {gh}
      {installed}
      onClose={() => (detail = null)}
      onUseSource={(d, s) => useSource(d, s)}
      onOpenDesc={openDesc}
      {onNeedAuth}
      {onFindInLibrary}
    />
  {/if}

  {#if descModal}
    <DescModal modal={descModal} onClose={() => (descModal = null)} />
  {/if}
</div>

<style>
  .browse {
    padding: 16px 20px 40px;
    max-width: 1200px;
    margin: 0 auto;
  }
  .browse-head {
    display: flex;
    align-items: flex-end;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
    margin-bottom: 4px;
  }
  .bh-title {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }
  .bh-title h2 {
    font-family: var(--font-display);
    margin: 0;
    font-size: 1.5rem;
    color: var(--text);
  }
  .catalog-count {
    color: var(--text-muted);
    font-size: 0.85rem;
  }
  .bh-status {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin: 4px 0 12px;
    flex-wrap: wrap;
  }
  .showing {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .source-note {
    color: var(--text-muted);
    font-size: var(--fs-xs);
    margin-left: auto;
    opacity: 0.8;
  }
  .more {
    display: flex;
    justify-content: center;
    padding: 24px 0 8px;
  }
  .more.end {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .more-btn {
    padding: 9px 22px;
    cursor: pointer;
  }
  .more-btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .error {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
    padding: 8px 12px;
    border-radius: var(--radius-sm);
    margin-bottom: 12px;
  }
  .ok-note {
    color: var(--primary-text);
    padding: 8px 12px;
    margin-bottom: 12px;
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    padding: 60px 0;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 16px;
    margin-top: 12px;
  }
  .btn {
    flex: 1;
    padding: 7px 10px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
