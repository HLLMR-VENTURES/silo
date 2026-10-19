<script lang="ts">
  // Keeps one view's crash from blanking the whole window:
  //   <ViewBoundary name="Library"><LibraryView /></ViewBoundary>
  //   <ViewBoundary name="Updates" overlay onClose={() => (open = false)}>...</ViewBoundary>
  // A render error shows an error state with "Reload this view" (re-renders the children) and
  // "Copy details"; the error is logged quietly. `overlay` (drawers, panels, dialogs) shows that
  // state as a fixed card centred in the window, so it can't end up off-screen, with "Close" when
  // `onClose` is given. `bodyText` is the line under the title: only views that can't change
  // anything should say "Your mods weren't changed."
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import { describeError, formatDetail } from "../errorCopy";
  import { reportError } from "../state/notify.svelte";

  let {
    name,
    overlay = false,
    onClose,
    bodyText = "Something went wrong showing this part of Silo.",
    children,
  }: {
    name: string;
    overlay?: boolean;
    onClose?: () => void;
    bodyText?: string;
    children: Snippet;
  } = $props();

  function details(error: unknown): string {
    return formatDetail(describeError(error, `showing ${name}`).detail);
  }
</script>

<svelte:boundary onerror={(error) => reportError(error, `showing ${name}`, { quiet: true })}>
  {@render children()}

  {#snippet failed(error, reset)}
    {#snippet state()}
      <ErrorState title="This part of Silo hit a problem" body={bodyText} details={details(error)}>
        {#snippet actions()}
          <Button variant="primary" icon="rescan" onclick={reset}>Reload this view</Button>
          {#if onClose}
            <Button variant="secondary" onclick={onClose}>Close</Button>
          {/if}
        {/snippet}
      </ErrorState>
    {/snippet}
    {#if overlay}
      <div class="boundary-overlay" role="alertdialog" aria-label="{name} hit a problem">
        <div class="boundary-card">{@render state()}</div>
      </div>
    {:else}
      {@render state()}
    {/if}
  {/snippet}
</svelte:boundary>

<style>
  .boundary-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-dialog);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--sp-4);
    background: var(--scrim);
  }
  .boundary-card {
    max-width: min(32rem, 100%);
    max-height: 100%;
    overflow: auto;
    padding: var(--sp-4);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
    box-shadow: var(--shadow-2);
  }
</style>
