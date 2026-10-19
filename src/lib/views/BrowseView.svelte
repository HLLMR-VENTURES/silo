<script lang="ts">
  // The catalog: <BrowseView>{#snippet children()}<ModBrowser ... />{/snippet}</BrowseView>.
  // ModBrowser keeps its own state, search binding and detail drawer.
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";

  let { children }: { children: Snippet } = $props();
</script>

<ViewBoundary name="Browse">
  <ViewHeader title="Browse" description="Find mods in Silo's catalog of ModHub and GitHub listings." />
  <div class="browse">{@render children()}</div>
</ViewBoundary>

<style>
  /* Browse fills the view (AppShell `fill`): ModBrowser's card grid scrolls itself. */
  .browse {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  /* While Browse's own detail drawer is open (it publishes its width as --drawer-w), the centred
     content shifts left of it instead of hiding under it. */
  .browse:has(:global(aside.drawer)) {
    padding-right: var(--drawer-w, 0px);
  }
</style>
