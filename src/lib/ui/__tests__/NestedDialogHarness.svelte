<script lang="ts">
  // Test-only: a dialog that opens a second dialog on top of itself.
  import Dialog from "../Dialog.svelte";

  let outer = $state(false);
  let inner = $state(false);
</script>

<button type="button" onclick={() => (outer = true)}>Open outer</button>
<Dialog bind:open={outer} title="Outer">
  <button type="button" onclick={() => (inner = true)}>Open inner</button>
  <Dialog bind:open={inner} title="Inner">
    <button type="button">Inner action</button>
  </Dialog>
</Dialog>
