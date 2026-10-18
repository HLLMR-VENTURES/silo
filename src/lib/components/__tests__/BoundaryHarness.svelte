<script lang="ts">
  // Test-only: a ViewBoundary around a child that can throw while rendering.
  import ViewBoundary from "../ViewBoundary.svelte";
  import Thrower from "./Thrower.svelte";

  let {
    overlay = false,
    onClose,
    bodyText,
  }: { overlay?: boolean; onClose?: () => void; bodyText?: string } = $props();
</script>

{#if bodyText === undefined}
  <ViewBoundary name="Library" {overlay} {onClose}><Thrower /></ViewBoundary>
{:else}
  <ViewBoundary name="Library" {overlay} {onClose} {bodyText}><Thrower /></ViewBoundary>
{/if}
