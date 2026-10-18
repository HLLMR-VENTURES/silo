<script lang="ts">
  // One source card = one interactive widget between us and the source. Reads are live
  // and public; actions (Star / Watch) run through the user's OWN GitHub token and
  // land on GitHub's servers — Silo only reflects the state back. Nothing is stored on
  // our side. See SiloAPI docs/ENRICHMENT.md "scope boundary".
  import Icon from "../ui/Icon.svelte";
  import { ghRepoStats, ghStar, ghWatch, openExternal } from "../api";
  import type { RepoStats } from "../types";

  let {
    owner,
    repo,
    connected = false,
    canWrite = false,
    onConnect,
  }: {
    owner: string;
    repo: string;
    connected?: boolean;
    canWrite?: boolean;
    onConnect?: () => void;
  } = $props();

  let stats = $state<RepoStats | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let busy = $state<"" | "star" | "watch">("");

  async function load() {
    loading = true;
    error = null;
    try {
      stats = await ghRepoStats(owner, repo);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  // Re-fetch if the drawer is reused for a different repo.
  $effect(() => {
    owner;
    repo;
    load();
  });

  function fmt(n: number): string {
    if (n < 1000) return `${n}`;
    return `${(n / 1000).toFixed(n < 10_000 ? 1 : 0)}k`;
  }

  async function toggleStar() {
    if (!stats) return;
    if (!connected || !canWrite) return onConnect?.();
    busy = "star";
    error = null;
    const next = !stats.youStarred;
    try {
      await ghStar(owner, repo, next);
      // Reflect the source's new state locally (authoritative copy lives on GitHub).
      stats.youStarred = next;
      stats.stars += next ? 1 : -1;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }

  async function toggleWatch() {
    if (!stats) return;
    if (!connected || !canWrite) return onConnect?.();
    busy = "watch";
    error = null;
    const next = !stats.youWatching;
    try {
      await ghWatch(owner, repo, next);
      stats.youWatching = next;
      stats.watchers += next ? 1 : -1;
    } catch (e) {
      error = String(e);
    } finally {
      busy = "";
    }
  }
</script>

<div class="src-card gh">
  <div class="src-head">
    <span class="src-name">
      <span class="src-glyph" aria-hidden="true"></span> GitHub
    </span>
    {#if stats}
      <button class="src-link" onclick={() => openExternal(stats!.htmlUrl)} title="Open on GitHub">
        {stats.fullName} <Icon name="external" size={12} />
      </button>
    {:else}
      <span class="src-sub mono">{owner}/{repo}</span>
    {/if}
  </div>

  {#if loading}
    <div class="src-body muted">Loading…</div>
  {:else if error && !stats}
    <div class="src-body">
      <span class="err">{error}</span>
      <button class="mini" onclick={load}>Retry</button>
    </div>
  {:else if stats}
    <div class="src-nums tnum">
      <span title="Stars"><Icon name="favorite" size={13} filled label="stars" /> {fmt(stats.stars)}</span>
      <span title="Forks"><Icon name="fork" size={13} label="forks" /> {fmt(stats.forks)}</span>
      <span title="Watchers"><Icon name="visible" size={13} label="watchers" /> {fmt(stats.watchers)}</span>
      <span title="Open issues"><Icon name="dot" size={13} label="open issues" /> {fmt(stats.openIssues)}</span>
      {#if stats.archived}<span class="archived" title="Archived repo">archived</span>{/if}
    </div>

    <div class="src-actions">
      {#if connected && canWrite}
        <button
          class="act"
          class:on={stats.youStarred}
          disabled={busy === "star"}
          onclick={toggleStar}
        >
          <Icon name="favorite" size={13} filled={!!stats.youStarred} />
          {stats.youStarred ? "Starred" : "Star"}
        </button>
        <button
          class="act"
          class:on={stats.youWatching}
          disabled={busy === "watch"}
          onclick={toggleWatch}
        >
          <Icon name="visible" size={13} />
          {stats.youWatching ? "Watching" : "Watch"}
        </button>
      {:else}
        <button class="act connect" onclick={() => onConnect?.()}>
          {connected ? "Enable Star & Watch" : "Connect GitHub to Star"}
        </button>
      {/if}
    </div>
    {#if error}<div class="src-err">{error}</div>{/if}
  {/if}
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
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .src-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
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
  .src-body {
    font-size: var(--fs-xs);
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .muted {
    color: var(--text-muted);
  }
  .src-nums {
    display: flex;
    gap: 14px;
    font-size: var(--fs-sm);
    color: var(--text);
    flex-wrap: wrap;
  }
  .archived {
    color: var(--text-muted);
    font-style: italic;
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
  .act:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .act.on {
    background: color-mix(in srgb, var(--accent, var(--primary)) 16%, transparent);
    border-color: var(--accent, var(--primary));
    color: var(--accent-text);
  }
  .act.connect {
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 40%, var(--border));
  }
  .act:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .mini {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    border-radius: var(--radius-sm);
    padding: 2px 8px;
    font-size: var(--fs-xs);
  }
  .err,
  .src-err {
    color: var(--danger);
    font-size: var(--fs-xs);
  }
</style>
