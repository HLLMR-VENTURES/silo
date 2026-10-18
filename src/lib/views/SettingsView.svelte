<script lang="ts">
  // Settings as a page: <SettingsView>{#snippet children()}<SettingsPanel ... />{/snippet}</SettingsView>.
  import type { Snippet } from "svelte";
  import ViewBoundary from "../components/ViewBoundary.svelte";
  import ViewHeader from "../components/shell/ViewHeader.svelte";

  let { children }: { children: Snippet } = $props();
</script>

<ViewBoundary name="Settings">
  <ViewHeader title="Settings" />
  {@render children()}
</ViewBoundary>
