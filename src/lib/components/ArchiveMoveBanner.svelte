<script lang="ts">
  // The library archive banners (#166): an unfinished move (running or paused, with
  // Pause / Resume / Retry / Cancel), or else a custom archive whose folder isn't available.
  import Icon from "../ui/Icon.svelte";
  import type { ArchiveStatus, MoveProgress } from "../api";
  import { fmtSize } from "../format";
  import { capList, modsCount, sentence, unDot } from "../archiveMoveLogic";

  let {
    archive,
    moveProgress,
    moveRunning,
    moveUnfinished,
    moveCancelling,
    pauseRequested,
    onPause,
    onResume,
    onCancel,
    onRetry,
    onChooseAnother,
  }: {
    archive: ArchiveStatus | null;
    moveProgress: MoveProgress | null;
    /** A move is copying right now. */
    moveRunning: boolean;
    /** A move is running or paused. */
    moveUnfinished: boolean;
    /** The move is heading back home. */
    moveCancelling: boolean;
    /** Pause was clicked and hasn't taken effect yet. */
    pauseRequested: boolean;
    onPause: () => void;
    onResume: () => void;
    onCancel: () => void;
    /** The archive is unavailable: look again (and rescan). */
    onRetry: () => void;
    /** The archive is unavailable: pick another folder. */
    onChooseAnother: () => void;
  } = $props();
</script>

{#if moveUnfinished}
  <div class="cloud-notice">
    <span class="cn-ico"><Icon name="archive" /></span>
    <div class="cn-text">
      {#if moveRunning}
        <b>{moveCancelling ? "Moving your library back" : "Moving your library"}</b>
        {#if archive?.moving}
          to <span class="tnum">{moveCancelling ? archive.moving.from : archive.moving.to}</span>
        {/if}
        —
        {#if moveProgress && moveProgress.total > 0}
          <span class="tnum">
            {moveProgress.done}/{moveProgress.total} mods · {fmtSize(moveProgress.bytesDone)} of
            {fmtSize(moveProgress.bytesTotal)}</span
          >.
        {:else}
          starting.
        {/if}
        Launch is blocked until it pauses or finishes.
      {:else if archive?.moving?.errors.length}
        <b>{modsCount(archive.moving.errors.length)} couldn't be moved:</b>
        {sentence(capList(archive.moving.errors.map(unDot), 3, "; "))}
        {archive.moving.pauseReason ? `${sentence(archive.moving.pauseReason)} ` : ""}The move is
        paused — Retry tries them again{moveCancelling ? "." : ", or Cancel moves everything back."}
      {:else if archive?.moving}
        <b>{moveCancelling ? "Moving your library back is paused." : "Archive move paused."}</b>
        {archive.moving.pauseReason ? `${sentence(archive.moving.pauseReason)} ` : ""}You can play —
        resume when you're done.
      {/if}
    </div>
    {#if moveRunning}
      <button class="cn-act" onclick={onPause} disabled={pauseRequested}>
        {pauseRequested ? "Pausing…" : "Pause"}
      </button>
    {:else if archive?.moving}
      <button class="cn-act" onclick={onResume}>
        {archive.moving.errors.length ? "Retry" : "Resume"}
      </button>
    {/if}
    {#if !moveCancelling}
      <button class="cn-act" onclick={onCancel}>Cancel move</button>
    {/if}
  </div>
{:else if archive?.path && !archive.available}
  <div class="cloud-notice">
    <span class="cn-ico"><Icon name="problem" /></span>
    <div class="cn-text">
      <b>Your library archive isn't available:</b> <span class="tnum">{archive.path}</span>. Parked
      mods are hidden until it's back; active mods linked from it won't load in-game.
    </div>
    <button class="cn-act" onclick={onRetry}>Retry</button>
    <button class="cn-act" onclick={onChooseAnother}>Choose another</button>
  </div>
{/if}
