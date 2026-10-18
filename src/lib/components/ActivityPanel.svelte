<script lang="ts">
  // What Silo is doing and what happened this session: running ops (with progress) on top, then
  // the activity log, newest first. Lives in ActivityButton's popover.
  import { copyText, saveTextFile } from "../api";
  import { copyOrSave } from "../clipboard";
  import Button from "../ui/Button.svelte";
  import Icon from "../ui/Icon.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import type { IconName } from "../ui/icons";
  import { activity, clearActivity, type ActivityEntry } from "../state/notify.svelte";
  import { running } from "../state/ops.svelte";

  const ICON: Record<ActivityEntry["level"], IconName> = {
    error: "problem",
    warning: "problem",
    info: "info",
    success: "ok",
  };

  let copiedId = $state<number | null>(null);
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => () => clearTimeout(timer));

  function clock(at: number): string {
    const d = new Date(at);
    return `${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
  }

  function duration(ms: number): string {
    if (ms < 1000) return "under a second";
    const s = Math.round(ms / 1000);
    if (s < 60) return `${s} s`;
    const m = Math.floor(s / 60);
    return `${m} min ${s % 60} s`;
  }

  async function copy(entry: ActivityEntry) {
    if (!entry.detail) return;
    try {
      const out = await copyOrSave(entry.detail, "silo-error-details.txt", copyText, saveTextFile);
      if (out === "cancelled") return;
      copiedId = entry.id;
      clearTimeout(timer);
      timer = setTimeout(() => (copiedId = null), 2000);
    } catch {
      // Saving failed too; nothing more to offer here.
    }
  }
</script>

<div class="panel">
  {#if running.length > 0}
    <section aria-label="Running now">
      <ul class="list">
        {#each running as op (op.id)}
          <li class="op">
            {#if op.hasProgress}
              <ProgressBar
                value={op.progress?.done}
                max={op.progress?.total ?? 100}
                label={op.progress?.label ? `${op.label}: ${op.progress.label}` : op.label}
              />
            {:else}
              <span class="op-label">{op.label}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if activity.length === 0}
    {#if running.length === 0}<p class="empty">Nothing has happened yet this session.</p>{/if}
  {:else}
    <section aria-label="Activity log">
      <ul class="list">
        {#each activity as entry (entry.id)}
          <li class="entry level-{entry.level}">
            <time class="time" datetime={new Date(entry.at).toISOString()}>{clock(entry.at)}</time>
            <span class="icon"><Icon name={ICON[entry.level]} size={14} /></span>
            <span class="message">
              {entry.message}
              {#if entry.count > 1}<span class="count">×{entry.count}</span>{/if}
              {#if entry.durationMs !== undefined}
                <span class="took">{duration(entry.durationMs)}</span>
              {/if}
            </span>
            {#if entry.detail}
              <Button size="sm" variant="ghost" onclick={() => copy(entry)}>
                {copiedId === entry.id ? "Copied" : "Copy details"}
              </Button>
              <span class="sr-only" role="status">{copiedId === entry.id ? "Details copied" : ""}</span>
            {/if}
          </li>
        {/each}
      </ul>
    </section>
    <div class="footer">
      <Button size="sm" variant="ghost" onclick={clearActivity}>Clear</Button>
    </div>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    width: min(22rem, calc(100vw - 32px));
    padding: var(--sp-2);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: var(--sp-1);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .op {
    padding: var(--sp-1) 0;
  }
  .op-label {
    font-size: var(--fs-sm);
  }
  section + section {
    padding-top: var(--sp-2);
    border-top: 1px solid var(--border);
  }
  .entry {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
  }
  .time {
    flex: none;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  .icon {
    display: inline-flex;
    flex: none;
    padding-top: 0.125rem;
    color: var(--muted);
  }
  .level-error .icon {
    color: var(--danger);
  }
  .level-warning .icon {
    color: var(--warn-text);
  }
  .level-success .icon {
    color: var(--success);
  }
  .message {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .count,
  .took {
    margin-left: var(--sp-1);
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .empty {
    margin: 0;
    padding: var(--sp-3) var(--sp-2);
    color: var(--muted);
    font-size: var(--fs-sm);
    text-align: center;
  }
  .footer {
    display: flex;
    justify-content: flex-end;
    border-top: 1px solid var(--border);
    padding-top: var(--sp-1);
  }
</style>
