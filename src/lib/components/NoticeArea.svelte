<script lang="ts">
  // Standing conditions (setNotice in src/lib/state/notify.svelte.ts), full width under the top
  // bar. Always shows the most important notice, every blocking notice, every notice with
  // progress and every `pinned` one; "+N more" expands the rest in place. Only blocking notices are alerts; the others
  // sit in a polite status region.
  import Button from "../ui/Button.svelte";
  import Icon from "../ui/Icon.svelte";
  import IconButton from "../ui/IconButton.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import type { IconName } from "../ui/icons";
  import { dismissNotice, sortedNotices, type NoticeAction, type NoticeEntry } from "../state/notify.svelte";

  const ICON: Record<NoticeEntry["level"], IconName> = {
    blocking: "problem",
    warning: "problem",
    info: "info",
  };

  let expanded = $state(false);
  const all = $derived(sortedNotices());
  /** Shown at all times: the top one, blocking ones, ones with progress, pinned ones. */
  const pinned = $derived(
    all.filter((n, i) => i === 0 || n.level === "blocking" || !!n.progress || !!n.pinned),
  );
  const rest = $derived(all.filter((n) => !pinned.includes(n)));
  const showRest = $derived(expanded && rest.length > 0);
  // Nothing left behind "+N more": the next notice that lands there starts collapsed.
  $effect(() => {
    if (rest.length === 0) expanded = false;
  });
  const uid = $props.id();
  const restId = `notice-more-${uid}`;

  function ordered(actions: NoticeAction[] | undefined): NoticeAction[] {
    if (!actions) return [];
    return [...actions.filter((a) => a.primary), ...actions.filter((a) => !a.primary)];
  }

  function run(a: NoticeAction) {
    Promise.resolve()
      .then(() => a.run())
      .catch((e) => console.error("[Silo] notice action failed", e));
  }
</script>

{#snippet strip(n: NoticeEntry)}
  <div
    class="notice level-{n.level}"
    role={n.level === "blocking" ? "alert" : "status"}
    aria-live={n.level === "blocking" ? undefined : "polite"}
  >
    <span class="icon"><Icon name={n.icon ?? ICON[n.level]} size={16} /></span>
    <div class="text">
      <p class="title">{n.title}</p>
      {#if n.body}<p class="body">{n.body}</p>{/if}
      {#if n.progress}
        <ProgressBar
          class="bar"
          value={n.progress.done}
          max={n.progress.total}
          label={n.progress.label}
        />
      {/if}
    </div>
    {#if n.actions?.length}
      <div class="actions">
        {#each ordered(n.actions) as a (a.label)}
          <Button
            size="sm"
            variant={a.primary ? "primary" : "secondary"}
            disabled={a.disabled}
            title={a.title}
            onclick={() => run(a)}
          >
            {a.label}
          </Button>
        {/each}
      </div>
    {/if}
    {#if n.dismissible}
      <IconButton icon="close" label="Dismiss: {n.title}" size="sm" onclick={() => dismissNotice(n.key)} />
    {/if}
  </div>
{/snippet}

{#if pinned.length > 0}
  <section class="notice-area" aria-label="Notices">
    {#each pinned as n (n.key)}{@render strip(n)}{/each}
    {#if rest.length > 0}
      <button
        type="button"
        class="more"
        aria-expanded={showRest}
        aria-controls={restId}
        onclick={() => (expanded = !showRest)}
      >
        <Icon name={showRest ? "chevronUp" : "chevronDown"} size={14} />
        {showRest ? "Show less" : `+${rest.length} more`}
      </button>
      <div id={restId} class="rest" hidden={!showRest}>
        {#each rest as n (n.key)}{@render strip(n)}{/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .notice-area {
    display: flex;
    flex-direction: column;
    width: 100%;
    border-bottom: 1px solid var(--border);
  }
  .notice {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
    padding: var(--sp-2) var(--sp-4);
    background: var(--info-soft);
    color: var(--text);
  }
  .notice + .notice,
  .rest .notice {
    border-top: 1px solid var(--border);
  }
  .level-warning {
    background: var(--warn-soft);
  }
  .level-blocking {
    background: var(--danger-soft);
  }
  .icon {
    display: inline-flex;
    flex: none;
    padding-top: 0.125rem;
    color: var(--info);
  }
  .level-warning .icon {
    color: var(--warn-text);
  }
  .level-blocking .icon {
    color: var(--danger);
  }
  .text {
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: var(--sp-1);
    min-width: 0;
  }
  .title {
    margin: 0;
    font-weight: 600;
    font-size: var(--fs-md);
    overflow-wrap: anywhere;
  }
  .body {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
  }
  .text :global(.bar) {
    max-width: 24rem;
  }
  .actions {
    display: flex;
    flex: none;
    flex-wrap: wrap;
    gap: var(--sp-2);
  }
  .more {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    margin: 0;
    padding: var(--sp-1) var(--sp-4);
    border: 0;
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .more:hover {
    color: var(--text);
  }
  .rest[hidden] {
    display: none;
  }
</style>
