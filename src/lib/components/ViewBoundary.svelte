<script lang="ts">
  // Keeps one view's crash from blanking the whole window:
  //   <ViewBoundary name="Library"><LibraryView /></ViewBoundary>
  // A render error shows an error state with "Reload this view" (re-renders the children) and
  // "Copy details"; the error is logged quietly.
  import type { Snippet } from "svelte";
  import Button from "../ui/Button.svelte";
  import ErrorState from "../ui/ErrorState.svelte";
  import { describeError, formatDetail } from "../errorCopy";
  import { reportError } from "../state/notify.svelte";

  let { name, children }: { name: string; children: Snippet } = $props();

  function details(error: unknown): string {
    return formatDetail(describeError(error, `showing ${name}`).detail);
  }
</script>

<svelte:boundary onerror={(error) => reportError(error, `showing ${name}`, { quiet: true })}>
  {@render children()}

  {#snippet failed(error, reset)}
    <ErrorState
      title="This part of Silo hit a problem."
      body="Your mods weren't changed."
      details={details(error)}
    >
      {#snippet actions()}
        <Button variant="primary" icon="rescan" onclick={reset}>Reload this view</Button>
      {/snippet}
    </ErrorState>
  {/snippet}
</svelte:boundary>
