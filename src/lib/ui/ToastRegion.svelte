<script lang="ts">
  // Bottom-right live region for toast(). Mount once, at the app root. It portals to <body> and is
  // exempt from dialog inertness, so announcements still reach screen readers while a dialog is open.
  import { portal } from "./portal";
  import Toast from "./Toast.svelte";
  import { toasts } from "./toasts.svelte";

  let windowActive = $state(typeof document === "undefined" ? true : document.hasFocus());
</script>

<svelte:window onfocus={() => (windowActive = true)} onblur={() => (windowActive = false)} />

<div class="toast-region" role="status" aria-live="polite" data-overlay-exempt use:portal>
  {#each toasts as item (item.id)}
    <Toast {item} {windowActive} />
  {/each}
</div>

<style>
  .toast-region {
    position: fixed;
    right: var(--sp-4);
    bottom: var(--sp-4);
    z-index: var(--z-toast);
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: var(--sp-2);
    pointer-events: none;
  }
</style>
