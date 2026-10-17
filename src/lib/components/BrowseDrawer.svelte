<script lang="ts">
  import type { CatalogModDetail, ModSourceOption } from "../types";
  import { resizable } from "../resize";
  import GithubCard from "./GithubCard.svelte";
  import ModHubCard from "./ModHubCard.svelte";
  import { label, gatedReason, fmtCount, parseRepo, canExpand, loadCatalogImage } from "../browse";

  let {
    detail,
    detailLoading,
    installingId,
    gh,
    installed,
    onClose,
    onUseSource,
    onOpenDesc,
    onNeedAuth,
    onFindInLibrary,
  }: {
    detail: CatalogModDetail | null;
    detailLoading: boolean;
    installingId: string | null;
    gh: { connected: boolean; canWrite: boolean };
    installed: Set<string>;
    onClose: () => void;
    onUseSource: (d: CatalogModDetail, s: ModSourceOption) => void;
    onOpenDesc: (d: CatalogModDetail) => void;
    onNeedAuth?: () => void;
    onFindInLibrary?: (techName: string) => void;
  } = $props();

  function hasLocally(d: CatalogModDetail): boolean {
    return d.techName != null && installed.has(d.techName);
  }

  // The drawer is non-modal (clicks fall through to the grid), so Escape is the click-away close.
  $effect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  });

  // Cover loads through the Rust proxy (data: URL) — the CDN needs a referer.
  let cover = $state("");
  $effect(() => {
    const url = detail?.imageUrl;
    cover = "";
    if (url) loadCatalogImage(url).then((u) => { if (detail?.imageUrl === url) cover = u; });
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="drawer-backdrop" onclick={onClose}></div>
<aside class="drawer" use:resizable>
  {#if detailLoading}
    <div class="empty">Loading…</div>
  {:else if detail}
    {@const d = detail}
    <div class="drawer-head">
      <div class="dh-info">
        <div class="dh-title">{d.title}</div>
        {#if d.author || d.latestVersion}
          <div class="dh-sub">
            {#if d.author}{d.author}{/if}{#if d.author && d.latestVersion}
              · {/if}{#if d.latestVersion}v{d.latestVersion.replace(/^v/i, "")}{/if}
          </div>
        {/if}
        {#if d.techName}<div class="dh-tech tnum">{d.techName}</div>{/if}
      </div>
      <button class="drawer-x" title="Close" onclick={onClose}>✕</button>
    </div>
    <div class="drawer-body">
      {#if cover}
        <img class="drawer-img" src={cover} alt="" />
      {/if}
      <dl class="facts">
        {#if d.category}<dt>Category</dt><dd>{d.category}</dd>{/if}
        {#if d.rating != null}
          <dt>Rating</dt>
          <dd class="tnum">
            ⭐ {d.rating.toFixed(1)}{#if d.ratingCount}
              &nbsp;({fmtCount(d.ratingCount)}){/if}
          </dd>
        {/if}
        {#if d.downloads != null}
          <dt>Downloads</dt><dd class="tnum">{fmtCount(d.downloads)}</dd>
        {/if}
      </dl>

      {#if d.description}
        <p class="drawer-desc clamped">{d.description}</p>
        {#if canExpand(d)}
          <button class="read-more" onclick={() => onOpenDesc(d)}>Read more →</button>
        {/if}
      {/if}

      {#if d.tags?.length}
        <div class="drawer-tags">
          {#each d.tags as t (t.namespace + t.value)}
            <span
              class="dtag"
              title="{t.namespace}{t.confidence != null ? ` · ${Math.round(t.confidence * 100)}% confidence` : ''}{t.source ? ` · ${t.source}` : ''}"
            >{t.value}</span>
          {/each}
        </div>
      {/if}

      <div class="drawer-sec">Available from</div>
      {#if d.sources.length === 0}
        <p class="drawer-none">No sources recorded.</p>
      {:else}
        <ul class="srcs">
          {#each d.sources as s (s.source + s.sourceUrl)}
            <li>
              <div class="src-head">
                <span class="src-name">{label(s.source)}</span>
                {#if s.version}<span class="src-ver tnum">{s.version}</span>{/if}
                <button
                  class="src-action"
                  class:can-install={s.installable}
                  disabled={hasLocally(d) || installingId === d.id}
                  onclick={() => onUseSource(d, s)}
                >
                  {#if hasLocally(d)}
                    In library
                  {:else if s.installable}
                    {installingId === d.id ? "Installing…" : "Install ⬇"}
                  {:else}
                    Open page ↗
                  {/if}
                </button>
              </div>
              {#if !s.installable}
                <p class="src-why">{gatedReason(s.source)}</p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}

      {#if d.sources.some((s) => (s.source === "github" && parseRepo(s.sourceUrl)) || s.source === "modhub")}
        {@const ghSrcs = d.sources.filter((s) => s.source === "github" && parseRepo(s.sourceUrl))}
        {@const mhSrcs = d.sources.filter((s) => s.source === "modhub")}
        <div class="drawer-sec">Interact</div>
        <div class="src-cards">
          {#each ghSrcs as s (s.sourceUrl)}
            {@const r = parseRepo(s.sourceUrl)}
            {#if r}
              <GithubCard
                owner={r.owner}
                repo={r.repo}
                connected={gh.connected}
                canWrite={gh.canWrite}
                onConnect={() => onNeedAuth?.()}
              />
            {/if}
          {/each}
          {#each mhSrcs as s (s.sourceUrl)}
            <ModHubCard
              rating={d.rating}
              ratingCount={d.ratingCount}
              sourceUrl={s.sourceUrl}
            />
          {/each}
        </div>
      {/if}

      {#if !hasLocally(d) && d.sources.length > 0 && !d.sources.some((s) => s.installable)}
        <!-- Nothing here is directly installable — tell the user exactly what to do,
             and that Silo will still take it from there. -->
        <p class="drawer-hint">
          None of these sources allow apps to download for you. Grab the .zip from a
          source above and drop it in your mods folder — Silo files it automatically
          on the next scan.
        </p>
      {/if}
      {#if hasLocally(d)}
        <div class="drawer-owned">
          <span>Already in your library</span>
          {#if d.techName && onFindInLibrary}
            <button class="find-lib-btn" onclick={() => onFindInLibrary?.(d.techName!)}>
              ⌕ Find in Library ↗
            </button>
          {/if}
        </div>
      {/if}
    </div>
  {/if}
</aside>

<style>
  .empty {
    color: var(--text-muted);
    text-align: center;
    padding: 60px 0;
  }
  /* ── Detail drawer ── */
  .drawer-backdrop {
    position: fixed;
    inset: 0;
    top: var(--topbar-h, 0px);
    background: rgba(0, 0, 0, 0.15);
    /* Non-modal: let clicks fall through to the catalog grid so clicking another card switches
       the drawer to that mod. Close via the ✕ or Escape instead of clicking away. */
    pointer-events: none;
    z-index: 40;
  }
  .drawer {
    position: fixed;
    top: var(--topbar-h, 0px);
    right: 0;
    bottom: 0;
    width: min(420px, 92vw);
    background: var(--surface);
    border-left: 1px solid var(--border);
    box-shadow: var(--shadow-2);
    z-index: 41;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .drawer-head {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 14px 16px;
    border-bottom: 1px solid var(--border);
  }
  .dh-info {
    flex: 1 1 auto;
    min-width: 0;
  }
  .dh-title {
    font-family: var(--font-display);
    font-size: 18px;
    font-weight: 600;
    line-height: 1.2;
    color: var(--text);
  }
  .dh-sub {
    font-size: 12.5px;
    color: var(--text-muted);
    margin-top: 2px;
  }
  .dh-tech {
    font-size: 11.5px;
    color: var(--text-muted);
    opacity: 0.75;
    margin-top: 2px;
  }
  .drawer-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 1rem;
    cursor: pointer;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .drawer-x:hover {
    background: var(--bg);
    color: var(--text);
  }
  .drawer-body {
    padding: 14px 16px 24px;
    overflow-y: auto;
  }
  .drawer-img {
    width: 100%;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
    margin-bottom: 12px;
    display: block;
  }
  /* Match the Library drawer's .d-meta spec sheet: label left, value right. */
  .facts {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 12px;
    margin: 0 0 12px;
    padding: 10px 0;
    border-top: 1px solid var(--border);
    font-size: 12.5px;
  }
  .facts dt {
    color: var(--text-muted);
  }
  .facts dd {
    margin: 0;
    color: var(--text);
    text-align: right;
  }
  .drawer-desc {
    color: var(--text);
    font-size: 0.88rem;
    line-height: 1.5;
    margin: 0 0 4px;
  }
  .drawer-desc.clamped {
    display: -webkit-box;
    -webkit-line-clamp: 4;
    line-clamp: 4;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .read-more {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: 0.8rem;
    font-weight: 600;
    padding: 0;
    margin: 0 0 14px;
    cursor: pointer;
  }
  .read-more:hover {
    text-decoration: underline;
  }
  /* Match the Library drawer's section labels so both read as one system. */
  .drawer-sec {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 700;
    color: var(--text-muted);
    margin-bottom: 8px;
  }
  .drawer-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin: 0 0 14px;
  }
  .dtag {
    font-size: 0.72rem;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-muted);
    text-transform: capitalize;
  }
  .src-cards {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 14px;
  }
  .drawer-none {
    color: var(--text-muted);
    font-size: 0.85rem;
    margin: 0 0 14px;
  }
  .srcs {
    list-style: none;
    padding: 0;
    margin: 0 0 16px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .srcs li {
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
  }
  .src-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .src-name {
    font-weight: 600;
    font-size: 0.85rem;
    color: var(--text);
  }
  .src-ver {
    font-size: 0.75rem;
    color: var(--text-muted);
  }
  .src-action {
    margin-left: auto;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text-muted);
    font: inherit;
    font-size: 0.78rem;
    padding: 4px 9px;
    cursor: pointer;
    white-space: nowrap;
  }
  .src-action.can-install {
    background: var(--primary);
    border-color: transparent;
    color: var(--on-primary);
    font-weight: 600;
  }
  .src-action:hover:not(:disabled) {
    filter: brightness(1.06);
  }
  .src-action:disabled {
    opacity: 0.5;
    cursor: default;
  }
  /* Why this source can't be installed directly — names who's responsible. */
  .src-why {
    margin: 6px 0 0;
    font-size: 0.72rem;
    line-height: 1.4;
    color: var(--text-muted);
  }
  .drawer-hint {
    font-size: 0.78rem;
    line-height: 1.5;
    color: var(--text-muted);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 9px 11px;
    margin: 0 0 12px;
  }
  .drawer-owned {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    text-align: center;
    color: var(--text-muted);
    font-size: 0.85rem;
    padding: 8px;
  }
  .find-lib-btn {
    border: 1px solid color-mix(in srgb, var(--primary) 45%, var(--border));
    background: var(--surface);
    color: var(--primary);
    font: inherit;
    font-weight: 600;
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .find-lib-btn:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: var(--primary);
  }
</style>
