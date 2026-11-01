<script lang="ts">
  // The "Browse" tab: discover mods from the canonical SiloAPI catalog, see which
  // you already have, and install new ones straight into the library.
  // The cards sit in a VirtualGrid (keyed by catalog id, one tab stop, columns from width); Enter
  // on a card opens its drawer. Loading shows skeleton cards, an unreachable catalog an error state
  // with [Try again] (the library is unaffected), and an empty search [Clear search].
  import { onMount, untrack } from "svelte";
  import { reportError, success } from "../state/notify.svelte";
  import {
    browseMods,
    siloapiStats,
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
  import VirtualGrid from "./library/VirtualGrid.svelte";
  import { browseCacheKey, getBrowseView, setBrowseView } from "../browseCache";
  import {
    BROWSE_GAP,
    browseCardHeight,
    browseBadges,
    browseColumns,
    catalogErrorCopy,
    emptyResultsTitle,
    installState,
    withInstalledAs,
    zipStem,
    type CatalogErrorCopy,
    type InstallProgressEntry,
  } from "../browse";
  import FilterBar from "./FilterBar.svelte";
  import Button from "../ui/Button.svelte";
  import EmptyState from "../ui/EmptyState.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import Skeleton from "../ui/Skeleton.svelte";
  import { toast } from "../ui/toasts.svelte";
  import { blocked, tryStart } from "../state/ops.svelte";

  interface Props {
    /** Tech names already in the local library, to flag "in library". */
    installed: Set<string>;
    /** Tech names with a known newer version (the last update check): "Update" on their cards. */
    updates?: ReadonlySet<string>;
    /** Called after a successful install so the parent can rescan. */
    onInstalled: (filename: string) => void;
    /** Route the user to the source-connect UI (Settings) from a source card. */
    onNeedAuth?: () => void;
    /** Jump to an installed mod in the Library (reverse of the library's "Find in Browse"). */
    onFindInLibrary?: (techName: string) => void;
    /** Show the Updates view (a card's "See update"). */
    onShowUpdates?: () => void;
    /** Pre-seed the catalog search (e.g. from a library row's "Find in Browse").
     *  Read once on mount, then `onSeedConsumed` tells the owner to forget it, so a later visit
     *  to Browse doesn't repeat the old search. */
    seed?: string | null;
    /** techName to auto-open once results land, so "Find in Browse" lands on the mod's drawer
     *  rather than just filtering to it. */
    seedTech?: string | null;
    /** Called once on mount, after `seed` / `seedTech` were read. */
    onSeedConsumed?: () => void;
    /** Show the "Browse mods" heading (off inside BrowseView, whose header already says Browse). */
    showTitle?: boolean;
    /** The catalog search text. The app header's search binds it; a change re-queries after the
     *  same 300 ms pause typing in the old search box had. */
    query?: string;
  }
  let {
    installed,
    updates,
    onInstalled,
    onNeedAuth,
    onFindInLibrary,
    onShowUpdates,
    seed = null,
    seedTech = null,
    onSeedConsumed,
    showTitle = true,
    query = $bindable(""),
  }: Props = $props();

  // Mods installed this session whose zip isn't named after the catalog techName: techName →
  // the library name they went in as (the zip's own name, never renamed). Lets the card and
  // drawer show "in your library" and find the right library row.
  let installedAs = $state<Record<string, string>>({});
  const shownInstalled = $derived(withInstalledAs(installed, installedAs));
  /** The library name of a catalog techName: what it was installed as, or itself. */
  const libraryName = (tech: string) => installedAs[tech] ?? tech;
  const findInLibrary = (tech: string) => onFindInLibrary?.(libraryName(tech));

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

  // The query the last search ran for (or the mount loads), so only a real change re-queries.
  let searchedQuery = untrack(() => query);
  $effect(() => {
    const q = query;
    if (q === searchedQuery) return;
    searchedQuery = q;
    onSearch();
  });
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

  /** Empty the search (the header's box too) and re-query at once. */
  function clearSearch() {
    if (debounce) clearTimeout(debounce);
    debounce = null;
    query = searchedQuery = "";
    load();
  }

  let results = $state<BrowseMod[]>([]);
  let total = $state(0);
  let lastPageFull = $state(false);
  let loadingMore = $state(false);
  let stats = $state<SiloStats | null>(null);
  let loading = $state(true);
  /** The first page failed: the whole view shows the error state. */
  let loadError = $state<CatalogErrorCopy | null>(null);
  /** "Load more" failed: said beside the button, the cards already shown stay. */
  let moreError = $state(false);
  /** The cache key of what `results` holds, so "Load more" never appends to another search. */
  let shownKey: string | null = null;
  /** The search the shown results (or the empty state) answer. */
  let shownQuery = $state("");
  let installing = $state<string | null>(null);
  // Live download progress per mod id: { done, total } bytes.
  let progress = $state<Record<string, InstallProgressEntry>>({});
  let grid = $state<{ focusKey(key: string): boolean }>();

  // Detail drawer state.
  let detail = $state<CatalogModDetail | null>(null);
  let detailLoading = $state(false);
  /** The card the drawer opened from, so closing it hands focus back there. */
  let detailFrom: string | null = null;
  /** Bumped by every open: only the latest request may fill the drawer. */
  let detailSeq = 0;
  /** The drawer was opened from the keyboard: it takes focus once loaded. */
  let detailByKey = $state(false);
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
  /** Set on unmount: late answers (the progress listener, the first load) must not act after it. */
  let destroyed = false;

  async function openDetail(m: BrowseMod, byKey = false) {
    const seq = ++detailSeq;
    detail = null;
    detailLoading = true;
    detailFrom = m.id;
    detailByKey = byKey;
    // Re-read GitHub state so a connection made in Settings reflects in the cards.
    refreshGh();
    try {
      const d = await siloapiModDetail(m.id);
      if (seq === detailSeq) detail = d;
    } catch (e) {
      if (seq === detailSeq) reportError(e, `Loading ${m.title}`, { retry: () => openDetail(m, byKey) });
    } finally {
      if (seq === detailSeq) detailLoading = false;
    }
  }

  function closeDetail() {
    detailSeq++;
    // Hand focus back to the card only when it was in the drawer (or fell to the page), so a
    // grid the user scrolled meanwhile isn't yanked back.
    const active = document.activeElement;
    const restore = !active || active === document.body || !!active.closest("aside.drawer");
    detail = null;
    detailLoading = false;
    const from = detailFrom;
    detailFrom = null;
    if (from && restore) grid?.focusKey(from);
  }

  /** Open a source's page in the browser. */
  async function openSource(s: ModSourceOption) {
    try {
      await openExternal(s.sourceUrl);
    } catch (e) {
      reportError(e, "Opening the mod's page");
    }
  }

  /** Click a source button: install it if we can, otherwise open its page. */
  async function useSource(m: BrowseMod, s: ModSourceOption) {
    if (s.installable) await install(m, s.source);
    else await openSource(s);
  }

  const ctx = $derived({ installed: shownInstalled, updates, installingId: installing });
  // Only the installing card reads `progress`, so a progress event re-renders that card alone.
  const stateOf = (m: BrowseMod) =>
    installState(m, { ...ctx, progress: m.id === installing ? progress[m.id] : undefined });

  /** A card's primary button: Install, Open on ModHub, Show in library or See update. */
  function primary(m: BrowseMod) {
    const st = stateOf(m);
    switch (st.kind) {
      case "install":
        // The button becomes a progress bar: keep focus on the card rather than lose it.
        grid?.focusKey(m.id);
        return void install(m, st.source.source);
      case "open":
        return void openSource(st.source);
      case "installed":
        if (m.techName) findInLibrary(m.techName);
        return;
      case "update":
        if (onShowUpdates) onShowUpdates();
        else if (m.techName) findInLibrary(m.techName);
        return;
      case "none":
        return void openDetail(m);
    }
  }

  /** The card's accessible name: "Lizard R-Series by Lizard Modding, in your library". */
  function cardLabel(m: BrowseMod): string {
    const by = m.author ? ` by ${m.author}` : "";
    const here = m.techName != null && shownInstalled.has(m.techName);
    return `${m.title}${by}${here ? ", in your library" : ""}`;
  }

  const PAGE = 60;
  // A server predating the `total` field reports 0. Don't render "of 0" or silently
  // hide Load-more — fall back to "the last page came back full, so there's probably
  // more", and just omit the total we don't have.
  const knowsTotal = $derived(total > 0);
  const hasMore = $derived(knowsTotal ? results.length < total : lastPageFull);
  const filtered = $derived(filterCount > 0);

  /** The root font size (Settings → UI size), so card heights follow it. */
  let remPx = $state(16);
  function readRem() {
    const px = parseFloat(getComputedStyle(document.documentElement).fontSize);
    if (Number.isFinite(px) && px > 0) remPx = px;
  }
  const cardHeight = $derived(browseCardHeight(remPx));

  /** Fetch the first page for the current filters, replacing what's shown. Served from the
   *  cross-mount view cache when we've seen this exact filter+sort combo before (no re-poll). */
  async function load() {
    loadError = null;
    moreError = false;
    const key = browseCacheKey({ category, tags: activeTags, availableBy, query, sort });
    const forQuery = query;
    const cached = getBrowseView(key);
    if (cached) {
      results = cached.results;
      total = cached.total;
      lastPageFull = cached.lastPageFull;
      shownKey = key;
      shownQuery = forQuery;
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
      const view = { results: page.mods, total: page.total, lastPageFull: page.mods.length === PAGE };
      // The filters moved on while this was out: keep the page for later, a newer load owns the grid.
      if (key !== browseCacheKey({ category, tags: activeTags, availableBy, query, sort })) {
        if (!getBrowseView(key)) setBrowseView(key, view);
        return;
      }
      results = view.results;
      total = view.total;
      lastPageFull = view.lastPageFull;
      shownKey = key;
      shownQuery = forQuery;
      setBrowseView(key, view);
    } catch (e) {
      if (key !== browseCacheKey({ category, tags: activeTags, availableBy, query, sort })) return;
      reportError(e, "Loading the catalog", { quiet: true });
      loadError = catalogErrorCopy(e);
      shownKey = null;
      shownQuery = forQuery;
      results = [];
      total = 0;
      lastPageFull = false;
    } finally {
      if (key === browseCacheKey({ category, tags: activeTags, availableBy, query, sort })) loading = false;
    }
  }

  /** Append the next page. Guards against double-firing and against the filters
   *  changing mid-flight, which would splice the wrong results onto the grid. */
  async function loadMore() {
    if (loadingMore || !hasMore || loading) return;
    const key = browseCacheKey({ category, tags: activeTags, availableBy, query, sort });
    // The shown cards answer another search (typing, its load still pending): appending this
    // search's next page to them would mix two searches, and cache the mix.
    if (key !== shownKey) return;
    loadingMore = true;
    moreError = false;
    try {
      const page = await browseMods({
        query: query.trim() || undefined,
        category: category || undefined,
        sort,
        tags: activeTags,
        availableBy,
        limit: PAGE,
        offset: results.length,
      });
      // filters moved on mid-flight → drop this page rather than splice wrong results
      if (key !== browseCacheKey({ category, tags: activeTags, availableBy, query, sort }) || key !== shownKey)
        return;
      results = [...results, ...page.mods];
      total = page.total;
      lastPageFull = page.mods.length === PAGE;
      // Grow the cached view so the next revisit restores everything we've paged in.
      setBrowseView(key, { results, total, lastPageFull });
    } catch (e) {
      reportError(e, "Loading more mods", { quiet: true });
      if (key === shownKey) moreError = true;
    } finally {
      loadingMore = false;
    }
  }

  function onSearch() {
    if (debounce) clearTimeout(debounce);
    debounce = setTimeout(load, 300);
  }

  async function install(m: BrowseMod, source?: string) {
    // It adds a mod to the mods folder: a library write, one at a time.
    const op = tryStart(`Installing ${m.title}`, { exclusive: "library-write" });
    if (!op) {
      toast({ message: `${blocked("library-write").reason} Try again when it's done.`, tone: "warn" });
      return;
    }
    installing = m.id;
    progress = { ...progress, [m.id]: { done: 0, total: null } };
    let filename: string | null = null;
    try {
      filename = await installRemoteMod(m.id, source);
    } catch (e) {
      reportError(e, `Installing ${m.title}`, { retry: () => install(m, source) });
    } finally {
      installing = null;
      const { [m.id]: _drop, ...rest } = progress;
      progress = rest;
      op.end();
    }
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (filename === null) return;
    // The library knows the mod by the file it went in as, which can differ from the
    // catalog's techName (the download is never renamed).
    const name = zipStem(filename);
    if (m.techName && m.techName.toLowerCase() !== name.toLowerCase()) {
      installedAs = { ...installedAs, [m.techName]: name };
    }
    onInstalled(filename);
    success(
      `Installed ${m.title}${name.toLowerCase() === m.techName?.toLowerCase() ? "" : ` as ${name}`}.`,
      onFindInLibrary ? { action: { label: "Show in library", run: () => onFindInLibrary(name) } } : {},
    );
  }

  /** Stats, categories and facets: best-effort extras, fetched again by [Try again] if missing. */
  async function loadExtras() {
    if (!stats) {
      try {
        stats = await siloapiStats();
      } catch {
        /* the count line just stays hidden */
      }
    }
    if (categories.length === 0) {
      try {
        // Sort alphabetically (the API returns them by mod count).
        categories = (await siloapiCategories()).sort((a, b) =>
          a.category.localeCompare(b.category, undefined, { sensitivity: "base" }),
        );
      } catch {
        // Older server without /categories — the filter just stays hidden.
      }
    }
    if (!facets) {
      try {
        facets = await siloapiFacets();
      } catch {
        // Older server without /facets — the filter panel just stays empty.
      }
    }
  }

  /** The error state's [Try again]. */
  function retry() {
    void loadExtras();
    void load();
  }

  async function init(seedText: string | null, seedTarget: string | null) {
    if (seedText && seedText.trim()) query = searchedQuery = seedText.trim();
    void loadExtras();
    await load();
    if (destroyed) return;
    // "Find in Browse" targets a specific mod — open its drawer once the seeded search lands it.
    if (seedTarget) {
      const hit = results.find((m) => m.techName === seedTarget);
      if (hit) await openDetail(hit);
    }
  }

  onMount(() => {
    // Read the seed before anything awaits, then let the owner forget it.
    const seedText = seed;
    const seedTarget = seedTech;
    if (seedText || seedTarget) onSeedConsumed?.();
    // The listener resolves after an await: if Browse unmounted meanwhile, drop it at once,
    // otherwise it would outlive the view (one more leaked listener per visit).
    onInstallProgress((p) => {
      progress = { ...progress, [p.id]: { done: p.done, total: p.total } };
    }).then(
      (fn) => {
        if (destroyed) fn();
        else unlisten = fn;
      },
      (e) => reportError(e, "Listening for install progress", { quiet: true }),
    );
    void init(seedText, seedTarget);
    // Settings → UI size changes the root font size (App sets --ui-scale on <html>).
    readRem();
    const rem = new MutationObserver(readRem);
    rem.observe(document.documentElement, { attributes: true, attributeFilter: ["style"] });
    return () => {
      rem.disconnect();
      destroyed = true;
      unlisten?.();
      unlisten = null;
      if (debounce) clearTimeout(debounce);
      debounce = null;
    };
  });
</script>

<div class="browse">
  <div class="browse-head">
    <div class="bh-title">
      {#if showTitle}<h3 class="bh-heading">Browse mods</h3>{/if}
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
    showSearch={false}
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

  <div class="bh-status" aria-live="polite">
    {#if results.length > 0}
      <span class="showing tnum">
        Showing {results.length.toLocaleString()}{knowsTotal
          ? ` of ${total.toLocaleString()}`
          : ""}
      </span>
    {/if}
  </div>

  {#if loadError}
    <ErrorState title={loadError.title} body={loadError.body} details={loadError.details} level={3}>
      {#snippet actions()}
        <Button variant="primary" icon="rescan" onclick={retry}>Try again</Button>
      {/snippet}
    </ErrorState>
  {:else if loading && results.length === 0}
    <div class="skeletons" aria-busy="true" aria-label="Loading the catalog">
      {#each { length: 8 } as _, i (i)}
        <div class="sk-card" style:height="{cardHeight}px">
          <Skeleton variant="block" height="124px" class="sk-thumb" />
          <div class="sk-body">
            <Skeleton width="85%" />
            <Skeleton width="55%" />
            <Skeleton width="40%" />
            <Skeleton variant="block" height="28px" class="sk-btn" />
          </div>
        </div>
      {/each}
    </div>
  {:else if results.length === 0}
    <EmptyState icon="search" title={emptyResultsTitle(shownQuery, filtered)} level={3}>
      {#snippet actions()}
        {#if shownQuery.trim()}<Button variant="primary" onclick={clearSearch}>Clear search</Button>{/if}
        {#if filtered}<Button variant="secondary" onclick={clearFilters}>Clear filters</Button>{/if}
      {/snippet}
    </EmptyState>
  {:else}
    <div class="grid-wrap" aria-busy={loading}>
      <VirtualGrid
        bind:this={grid}
        items={results}
        getKey={(m) => m.id}
        itemHeight={cardHeight}
        gap={BROWSE_GAP}
        columns={browseColumns}
        label="Catalog mods"
        itemLabel={cardLabel}
        onopen={(m) => openDetail(m, true)}
      >
        {#snippet item(m)}
          <ModCard
            {m}
            state={stateOf(m)}
            badges={browseBadges(m, ctx)}
            busy={installing !== null}
            onPrimary={() => primary(m)}
            onOpenDetail={() => openDetail(m)}
          />
        {/snippet}
      </VirtualGrid>
    </div>

    {#if hasMore || moreError}
      <div class="more">
        {#if moreError}<span class="more-error">Silo couldn't load more mods.</span>{/if}
        <Button variant="secondary" loading={loadingMore} disabled={loading} onclick={loadMore}>
          {#if moreError}
            Try again
          {:else if knowsTotal}
            Load {Math.min(PAGE, total - results.length)} more
          {:else}
            Load more
          {/if}
        </Button>
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
      progress={detail ? progress[detail.id] : undefined}
      busy={installing !== null}
      takeFocus={detailByKey}
      {gh}
      installed={shownInstalled}
      {updates}
      onClose={closeDetail}
      onUseSource={(d, s) => useSource(d, s)}
      onOpenDesc={openDesc}
      {onNeedAuth}
      onFindInLibrary={onFindInLibrary ? findInLibrary : undefined}
      {onShowUpdates}
    />
  {/if}

  {#if descModal}
    <DescModal modal={descModal} onClose={() => (descModal = null)} />
  {/if}
</div>

<style>
  /* Fills the view (AppShell `fill`): the card grid scrolls itself. No width cap of its own:
     AppShell's wide content cap bounds Browse, and the grid adds columns as the window grows. */
  .browse {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
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
  .bh-heading {
    font-family: var(--font-display);
    margin: 0;
    font-size: 1.5rem;
    color: var(--text);
  }
  .catalog-count {
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .bh-status {
    display: flex;
    align-items: baseline;
    gap: 12px;
    min-height: 1.25rem;
    margin: 4px 0 8px;
  }
  .showing {
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  .grid-wrap {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
  }
  .skeletons {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(232px, 1fr));
    gap: 16px;
    overflow: hidden;
    flex: 1 1 auto;
    min-height: 0;
    align-content: start;
  }
  .sk-card {
    display: flex;
    flex-direction: column;
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    overflow: hidden;
  }
  .sk-card :global(.sk-thumb) {
    border-radius: 0;
  }
  .sk-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    padding: var(--sp-3);
  }
  .sk-body :global(.sk-btn) {
    margin-top: auto;
  }
  .more {
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: var(--sp-3);
    padding: var(--sp-3) 0 0;
  }
  .more.end {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .more-error {
    color: var(--danger);
    font-size: var(--fs-sm);
  }
</style>
