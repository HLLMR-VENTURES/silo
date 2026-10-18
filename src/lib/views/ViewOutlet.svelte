<script lang="ts">
  // Renders the snippet for the current view, nothing for a view without one:
  //   <ViewOutlet view={nav.view}>
  //     {#snippet library()}<LibraryView ... />{/snippet}
  //     {#snippet browse()}<BrowseView ... />{/snippet}
  //   </ViewOutlet>
  // App keeps owning the state; each snippet passes it down to its view.
  import type { Snippet } from "svelte";
  import type { ViewId } from "../state/nav.svelte";

  type Props = { view: ViewId } & Partial<Record<ViewId, Snippet>>;

  let { view, ...views }: Props = $props();

  const current = $derived(views[view]);
</script>

{@render current?.()}
