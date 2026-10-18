<script lang="ts">
  // A Browse mod's full description, in a modal Dialog (Esc or Close; focus goes back to the
  // button that opened it).
  import Button from "../ui/Button.svelte";
  import Dialog from "../ui/Dialog.svelte";
  import { openExternal } from "../api";

  let {
    modal,
    onClose,
  }: {
    modal: {
      title: string;
      text: string;
      url: string | null;
      loading: boolean;
      source: "catalog" | "summary";
    };
    onClose: () => void;
  } = $props();

  let open = $state(true);
</script>

<Dialog bind:open title={modal.title} size="lg" onclose={() => onClose()}>
  {#if modal.text}
    <p class="modal-text">{modal.text}</p>
  {:else}
    <p class="modal-empty">No description text available.</p>
  {/if}
  {#if modal.loading}
    <p class="modal-empty" role="status">Loading the full description…</p>
  {/if}
  <p class="modal-note">
    {#if modal.source === "catalog"}
      Full body from the catalog. The source page has the original formatting.
    {:else}
      Catalog summary. The full write-up lives on the mod's page.
    {/if}
  </p>
  {#snippet footer()}
    {#if modal.url}
      <Button icon="external" onclick={() => openExternal(modal.url!)}>Open full mod page</Button>
    {/if}
    <Button variant="primary" data-autofocus onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Dialog>

<style>
  .modal-text {
    margin: 0;
    white-space: pre-wrap;
    line-height: 1.55;
    font-size: var(--fs-sm);
  }
  .modal-empty {
    margin: var(--sp-2) 0 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
  .modal-note {
    margin: var(--sp-4) 0 0;
    color: var(--muted);
    font-size: var(--fs-xs);
  }
</style>
