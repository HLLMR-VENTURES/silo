<script lang="ts">
  import Icon from "../../ui/Icon.svelte";
  import type { ModEntry } from "../../types";
  import { fmtSize } from "../../format";
  import { plural } from "../../plural";

  let {
    modCount,
    organizedCount,
    activeCount,
    mapsCount,
    libStats,
  }: {
    modCount: number;
    organizedCount: number;
    activeCount: number;
    mapsCount: number;
    libStats: {
      totalSize: number;
      cats: { name: string; count: number; size: number }[];
      maxCatSize: number;
      largest: ModEntry[];
      rated: number;
      avgRating: number;
      tagged: number;
    };
  } = $props();
</script>

<div class="conflicts-panel">
  <div class="lp-head">
    <h3 class="lp-title">Library statistics</h3>
    <span class="lp-sub tnum">{fmtSize(libStats.totalSize)}</span>
  </div>

  <div class="st-tiles">
    <div class="st-tile"><b class="tnum">{modCount}</b><span>{plural(modCount, { one: "mod", other: "mods" })}</span></div>
    <div class="st-tile"><b class="tnum">{organizedCount}</b><span>organized</span></div>
    <div class="st-tile"><b class="tnum">{activeCount}</b><span>active</span></div>
    <div class="st-tile"><b class="tnum">{mapsCount}</b><span>{plural(mapsCount, { one: "map", other: "maps" })}</span></div>
    <div class="st-tile"><b class="tnum">{libStats.tagged}</b><span>tagged</span></div>
    <div class="st-tile">
      <b class="tnum">{libStats.avgRating ? libStats.avgRating.toFixed(1) : "–"}</b>
      <span>avg <Icon name="favorite" size={11} filled label="stars" /> ({libStats.rated})</span>
    </div>
  </div>

  <div class="hz-group">Disk usage by category</div>
  {#each libStats.cats as c (c.name)}
    <div class="st-cat">
      <div class="st-cat-top">
        <span class="st-cat-name">{c.name}</span>
        <span class="st-cat-size tnum">{fmtSize(c.size)} · {c.count}</span>
      </div>
      <div class="st-bar">
        <div class="st-bar-fill" style="width: {(c.size / libStats.maxCatSize) * 100}%"></div>
      </div>
    </div>
  {/each}

  <div class="hz-group">Largest mods</div>
  {#each libStats.largest as m (m.techName)}
    <div class="st-big">
      <span class="st-big-name">{m.title ?? m.techName}</span>
      <span class="st-big-size tnum">{fmtSize(m.size)}</span>
    </div>
  {/each}
</div>

<style>
  /* A page section in its view (it was a panel floating over the library). */
  .conflicts-panel {
    max-width: 48rem;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 10px;
  }
  .lp-head {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    padding: 6px 8px 10px;
    font-family: var(--font-display);
    font-weight: 600;
  }
  .lp-title {
    margin: 0;
    font: inherit;
  }
  .lp-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    font-family: var(--font-ui);
  }
  .hz-group {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    padding: 12px 8px 6px;
  }
  .st-tiles {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
    padding: 8px 0 4px;
  }
  .st-tile {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 10px;
    text-align: center;
  }
  .st-tile b {
    display: block;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
  }
  .st-tile span {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .st-cat {
    padding: 5px 8px;
  }
  .st-cat-top {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-xs);
    margin-bottom: 3px;
  }
  .st-cat-size {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .st-bar {
    height: 6px;
    background: var(--border);
    border-radius: 999px;
    overflow: hidden;
  }
  .st-bar-fill {
    height: 100%;
    background: var(--primary);
    border-radius: 999px;
  }
  .st-big {
    display: flex;
    justify-content: space-between;
    font-size: var(--fs-xs);
    padding: 4px 8px;
  }
  .st-big-size {
    color: var(--text-muted);
  }
</style>
