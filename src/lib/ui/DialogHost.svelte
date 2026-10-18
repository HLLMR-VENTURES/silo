<script lang="ts">
  // Renders the ask()/askText() queue, one dialog at a time. Mount once, at the app root.
  import { onMount } from "svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import { currentDialog, registerDialogHost, settleDialog } from "./dialogs.svelte";

  onMount(() => registerDialogHost());

  const req = $derived(currentDialog());
</script>

{#if req}
  {#key req.id}
    {#if req.kind === "confirm"}
      {@const r = req}
      <ConfirmDialog open {...r.opts} onresult={(ok, checked) => settleDialog(r, ok, checked)} />
    {:else}
      {@const r = req}
      <PromptDialog open {...r.opts} onresult={(v) => settleDialog(r, v)} />
    {/if}
  {/key}
{/if}
