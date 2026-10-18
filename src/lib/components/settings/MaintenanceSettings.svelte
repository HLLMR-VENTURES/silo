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
      <Icon name="cloud" size={14} /> Your mods folder is on a cloud-synced drive (OneDrive / Google Drive /
      Proton Drive), which doesn't support hardlinks — so Silo projects active mods as <b>copies</b>.
      That works, but uses more disk. Excluding this folder from cloud sync lets Silo use hardlinks
      instead (no duplication).
    </p>
  {:else if organizedCount > 0}
    <p class="hint">
      Collapses any active mod that was projected as a full copy (rather than a hardlink) back into
      a hardlink — frees the duplicated space and makes activating/deactivating instant. Safe: only
      touches files byte-identical to the archived copy, never a build you swapped in.
    </p>
    <div class="btnrow">
      <Button icon="link" disabled={lock.locked} title={lock.reason} onclick={onOptimizeLinks}>
        Reclaim disk space
      </Button>
    </div>
    {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
  {:else}
    <p class="hint">Available once your library is organized (Organize, on the Library tab).</p>
  {/if}
</SettingsCard>

<SettingsCard title="Undo organizing">
  {#if organizedCount > 0}
    <p class="hint">
      Puts every mod back in your mods folder and removes Silo's links. Nothing is deleted. To
      organize again, use Organize on the Library tab.
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
