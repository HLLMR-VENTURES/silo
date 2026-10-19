<script lang="ts">
  // Settings → Maintenance: re-read every mod, reclaim disk space (copies → links) and undo
  // organizing. App owns the confirms and the work; these only call back.
  //   <MaintenanceSettings {organizedCount} {cloudSynced} {lockReason} {scanning} {moveUnfinished}
  //     {onRebuild} {onOptimizeLinks} {onRestoreVanilla} />
  import Button from "../../ui/Button.svelte";
  import Icon from "../../ui/Icon.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import { rereadLock, settingsLock } from "./lock";

  interface Props {
    organizedCount: number;
    /** The mods folder is on a cloud-synced drive (no hardlinks: active mods are copies). */
    cloudSynced: boolean;
    /** Why library-changing controls are off (a library write is running), or "". */
    lockReason: string;
    scanning: boolean;
    /** An archive move is running or paused. */
    moveUnfinished: boolean;
    /** Re-read every mod (clears the scan cache and rescans). */
    onRebuild: () => void;
    /** Reclaim disk space (collapse copies into hardlinks). */
    onOptimizeLinks: () => void;
    /** Undo organizing (put every mod back in the mods folder; App confirms first). */
    onRestoreVanilla: () => void;
  }

  let {
    organizedCount,
    cloudSynced,
    lockReason,
    scanning,
    moveUnfinished,
    onRebuild,
    onOptimizeLinks,
    onRestoreVanilla,
  }: Props = $props();

  const lock = $derived(settingsLock({ lockReason, scanning, moveUnfinished }));
  const reread = $derived(rereadLock({ lockReason, scanning }));
</script>

<SettingsCard
  title="Re-read every mod"
  description="Scans every mod again from scratch. Use it after a Silo update improves categorization, so existing mods pick up the better categories."
>
  {#snippet actions()}
    <Button icon="rescan" disabled={reread.locked} title={reread.title} onclick={onRebuild}>
      Re-read every mod
    </Button>
    {#if reread.locked}<p class="reason">{reread.reason}</p>{/if}
  {/snippet}
</SettingsCard>

<SettingsCard title="Reclaim disk space">
  {#if organizedCount > 0 && cloudSynced}
    <p class="hint">
      <Icon name="cloud" size={14} /> Your mods folder can't use links, so Silo keeps your active mods
      there as <b>copies</b>. That works, but uses more disk space. This usually means the folder is
      synced by OneDrive, Google Drive or Proton Drive. Leave it out of the sync and Silo can link
      them instead, with no duplicates.
    </p>
  {:else if organizedCount > 0}
    <p class="hint">
      Turns active mods that were copied into your mods folder back into links. This frees the
      duplicate space and makes turning mods on and off instant. It only touches files identical to
      the copy in your library archive, never a build you swapped in.
    </p>
    <div class="btnrow">
      <Button icon="link" disabled={lock.locked} title={lock.reason} onclick={onOptimizeLinks}>
        Reclaim disk space
      </Button>
    </div>
    {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
  {:else}
    <p class="hint">Available once your library is organized. Organize is in Settings → Library.</p>
  {/if}
</SettingsCard>

<SettingsCard title="Undo organizing">
  {#if organizedCount > 0}
    <p class="hint">
      Puts every mod back in your mods folder and removes Silo's links. Nothing is deleted. To
      organize again, use Organize in Settings → Library.
    </p>
    <div class="btnrow">
      <Button variant="danger" disabled={lock.locked} title={lock.reason} onclick={onRestoreVanilla}>
        Undo organizing
      </Button>
    </div>
    {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
  {:else}
    <p class="hint">Nothing is organized, so there's nothing to undo.</p>
  {/if}
</SettingsCard>
