<script lang="ts">
  // Updates: "Mod updates · Last checked 2 minutes ago · [Check now]", the privacy line, the
  // one-time offer to check at startup, progress while checking, then the sections App renders.
  //   <UpdatesView checking={false} checkedAt={null} checkDisabled={false} oncheck={check}>...</UpdatesView>
  import { onMount, type Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";
  import Button from "../ui/Button.svelte";
  import ProgressBar from "../ui/ProgressBar.svelte";
  import { tooltip } from "../ui/tooltip";
  import { lastCheckedText } from "../updates/sections";
  import {
    answerAutoCheckOffer,
    autoCheckOffer,
    takeAutoCheckOffer,
    UPDATE_PRIVACY_LINE,
  } from "../updates/autoCheck.svelte";

  interface Props {
    checking: boolean;
    /** When the last check finished (epoch ms), or null before any. */
    checkedAt: number | null;
    /** Linked GitHub repos checked so far, while checking; null before that step. */
    progress?: { done: number; total: number } | null;
    /** The check can't start now (a library write is running, or there are no mods). */
    checkDisabled: boolean;
    /** Why it can't, shown as the button's tooltip. */
    checkDisabledReason?: string;
    oncheck: () => void;
    children: Snippet;
  }

  let {
    checking,
    checkedAt,
    progress = null,
    checkDisabled,
    checkDisabledReason = "",
    oncheck,
    children,
  }: Props = $props();

  // "2 minutes ago" keeps itself current while the view is open.
  let now = $state(Date.now());
  onMount(() => {
    if (takeAutoCheckOffer()) autoCheckOffer.open = true;
    const t = setInterval(() => (now = Date.now()), 30_000);
    return () => clearInterval(t);
  });

  const description = $derived(
    checking
      ? "Mod updates · Checking…"
      : checkedAt != null
        ? `Mod updates · Last checked ${lastCheckedText(checkedAt, now)}`
        : "Mod updates · Not checked yet",
  );
  // Before the first check the panel's empty state carries the privacy line and Check now, so the
  // header doesn't repeat them.
  const neverChecked = $derived(!checking && checkedAt == null);
  const progressLabel = $derived(
    progress && progress.total > 0
      ? `Checked ${progress.done} of ${progress.total} ${progress.total === 1 ? "mod" : "mods"} on GitHub`
      : "Asking Silo's catalog…",
  );
</script>

<ViewBoundary name="Updates">
  {#snippet checkNow()}
    <span use:tooltip={checkDisabled && checkDisabledReason ? checkDisabledReason : null}>
      <Button icon="update" loading={checking} disabled={checkDisabled} onclick={oncheck}>
        Check now
      </Button>
    </span>
  {/snippet}
  <ViewHeader title="Updates" {description} actions={neverChecked ? undefined : checkNow} />
  {#if !neverChecked}<p class="privacy">{UPDATE_PRIVACY_LINE}</p>{/if}

  {#if autoCheckOffer.open}
    <section class="offer" aria-labelledby="auto-check-offer">
      <h3 id="auto-check-offer">Check for mod updates when Silo starts?</h3>
      <p>You can change this in Settings.</p>
      <div class="offer-acts">
        <Button variant="primary" onclick={() => answerAutoCheckOffer(true)}>Check when Silo starts</Button>
        <Button onclick={() => answerAutoCheckOffer(false)}>Only when I ask</Button>
      </div>
    </section>
  {/if}

  {#if checking}
    <div class="progress">
      <ProgressBar
        value={progress && progress.total > 0 ? progress.done : undefined}
        max={progress?.total || 1}
        label={progressLabel}
      />
    </div>
  {/if}

  {@render children()}
</ViewBoundary>

<style>
  .privacy {
    margin: 0 0 var(--sp-4);
    max-width: 52rem;
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }
  .offer {
    max-width: 52rem;
    margin: 0 0 var(--sp-4);
    padding: var(--sp-3) var(--sp-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius);
    background: var(--surface-raised);
  }
  .offer h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-md);
    font-weight: 600;
  }
  .offer p {
    margin: var(--sp-1) 0 var(--sp-3);
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }
  .offer-acts {
    display: flex;
    gap: var(--sp-2);
    flex-wrap: wrap;
  }
  .progress {
    max-width: 52rem;
    margin: 0 0 var(--sp-4);
  }
</style>
