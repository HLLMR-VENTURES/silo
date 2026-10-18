<script lang="ts">
  // The mods folder is blocked (`layoutProblem`, #220): it shares files with another layout,
  // so Silo neither scans nor writes it. Says why, and offers the ways out.
  import Icon from "../ui/Icon.svelte";
  import type { ModsRootStatus } from "../api";

  let {
    status,
    onRetry,
    onPointGameHere,
    onChooseAnother,
  }: {
    /** A status whose `layoutProblem` is set. */
    status: ModsRootStatus;
    /** Read the folder again (offered when the check itself failed to read it). */
    onRetry: () => void;
    /** Write FS25's mods-folder setting to this folder. */
    onPointGameHere: () => void;
    onChooseAnother: () => void;
  } = $props();
</script>

<div class="cloud-notice">
  <span class="cn-ico"><Icon name="problem" /></span>
  <div class="cn-text">
    <b>Silo won't change files in this mods folder:</b>
    <span class="tnum">{status.path}</span>. {status.layoutProblem}
    {#if status.layoutConflict}
      (<span class="tnum">{status.layoutConflict}</span>)
    {/if}
  </div>
  {#if status.layoutProblemCause === "readError"}
    <button class="cn-act" onclick={onRetry}>Retry</button>
  {/if}
  {#if status.mismatch && status.canWriteGameSetting}
    <button class="cn-act" onclick={onPointGameHere}>Point FS25 here</button>
  {/if}
  <button class="cn-act" onclick={onChooseAnother}>Choose another</button>
</div>
