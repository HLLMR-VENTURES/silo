<script lang="ts">
  // ModHub (Giants) source card. Giants has no API and no OAuth, and reviews require a
  // logged-in farming-simulator.com session — so this card is READ + DEEP-LINK only. It
  // never handles Giants credentials or automates a review; the rating shown comes from
  // the catalog aggregate (SiloAPI ingests it politely, server-side), and the action
  // sends the user to the real page to log in and rate. This is the ethical line.
  import Icon from "../ui/Icon.svelte";
  import { openExternal } from "../api";

  let {
    rating = null,
    ratingCount = null,
    sourceUrl,
  }: {
    rating?: number | null;
    ratingCount?: number | null;
    sourceUrl: string;
  } = $props();

  function fmt(n: number): string {
    if (n < 1000) return `${n}`;
    return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  }
</script>

<div class="src-card modhub">
  <div class="src-head">
    <span class="src-name">ModHub</span>
    <button class="src-link" onclick={() => openExternal(sourceUrl)} title="Open on ModHub">
      farming-simulator.com <Icon name="external" size={12} />
    </button>
  </div>

  <div class="src-nums tnum">
    {#if rating != null}
      <span title="Community rating on ModHub">
        <Icon name="favorite" size={13} filled /> {rating.toFixed(1)}{#if ratingCount}<span class="sub">&nbsp;({fmt(ratingCount)})</span>{/if}
      </span>
    {:else}
      <span class="muted">Rating not synced yet</span>
    {/if}
  </div>

  <div class="src-actions">
    <button class="act" onclick={() => openExternal(sourceUrl)}>Rate on ModHub <Icon name="external" size={12} /></button>
  </div>
  <p class="why">Reviews are left on ModHub — Silo links you there to sign in and rate.</p>
</div>

<style>
  .src-card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface, var(--bg));
    padding: 10px 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .src-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 10px;
  }
  .src-name {
    font-weight: 700;
    font-size: var(--fs-sm);
  }
  .src-link {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 0;
    cursor: pointer;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .src-link:hover {
    text-decoration: underline;
  }
  .src-nums {
    display: flex;
    gap: 14px;
    font-size: var(--fs-sm);
    color: var(--text);
    flex-wrap: wrap;
  }
  .sub {
    color: var(--text-muted);
  }
  .muted {
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .src-actions {
    display: flex;
    gap: 8px;
  }
  .act {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .act:hover {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .why {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.45;
  }
</style>
