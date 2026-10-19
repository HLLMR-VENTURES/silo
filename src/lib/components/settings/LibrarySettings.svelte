<script lang="ts">
  // Settings → Library: the layout (Organize), auto-filing new mods, and where the library
  // archive lives.
  //   <LibrarySettings {organizedCount} {unorganizedCount} {autoFileNew} {archive} ... />
  import Button from "../../ui/Button.svelte";
  import Switch from "../../ui/Switch.svelte";
  import type { ArchiveStatus } from "../../api";
  import SettingsCard from "./SettingsCard.svelte";
  import { nMods } from "../../firstRun/firstRun";
  import { folderOpener } from "./folders.svelte";
  import { settingsLock } from "./lock";

  interface Props {
    autoFileNew: boolean;
    organizedCount: number;
    unorganizedCount: number;
    /** Why library-changing controls are off (a library write is running), or "". */
    lockReason: string;
    scanning: boolean;
    archive: ArchiveStatus | null;
    /** An archive move is running or paused — library writes are refused until it's done. */
    moveUnfinished: boolean;
    /** The startup status read gave up (App retries when Settings opens). */
    archiveLoadFailed: boolean;
    onSetAutoFile: (v: boolean) => void;
    onOrganize: () => void;
    onChangeArchive: () => void;
    onArchiveDefault: () => void;
  }

  let {
    autoFileNew,
    organizedCount,
    unorganizedCount,
    lockReason,
    scanning,
    archive,
    moveUnfinished,
    archiveLoadFailed,
    onSetAutoFile,
    onOrganize,
    onChangeArchive,
    onArchiveDefault,
  }: Props = $props();

  const lock = $derived(settingsLock({ lockReason, scanning, moveUnfinished }));
  const folders = folderOpener();
</script>

<SettingsCard title="Library layout">
  <p class="hint">
    Organize files your zipped mods into your library archive, sorted by category, where the
    game ignores them. Active mods are linked back into your mods folder.
    <b>Undo organizing</b> (in Maintenance) puts every mod back.
    {#if organizedCount > 0}{nMods(organizedCount)} organized now.{/if}
  </p>
  <div class="btnrow">
    <Button disabled={lock.locked} title={lock.reason} onclick={onOrganize}>
      Organize {unorganizedCount > 0 ? nMods(unorganizedCount) : "library"}
    </Button>
  </div>
  {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
</SettingsCard>

<SettingsCard title="Auto-file new mods">
  <div class="row">
    <p class="hint">When Silo reads your mods folder, it files newly downloaded mods into your library and keeps them active.</p>
    <Switch
      checked={autoFileNew}
      ariaLabel="Auto-file new mods"
      onchange={() => onSetAutoFile(!autoFileNew)}
    />
  </div>
</SettingsCard>

<SettingsCard title="Library archive" description="Where Silo keeps the mods it has filed.">
  {#snippet tools()}
    {#if archive?.path && archive.available}
      <Button size="sm" variant="ghost" icon="external" onclick={() => folders.open("archive")}>
        Open folder
      </Button>
    {/if}
    <Button
      size="sm"
      disabled={lock.locked || !archive?.path}
      title={lock.reason}
      onclick={onChangeArchive}
    >
      Change…
    </Button>
  {/snippet}
  {#if folders.error?.which === "archive"}<p class="hint">{folders.error.msg}</p>{/if}
  {#if archive?.path}
    <p class="path" class:muted={!archive.available}>{archive.path}</p>
    <p class="hint">
      {archive.isDefault ? "Inside your mods folder (default)" : "Custom location"}{archive.available
        ? ""
        : " · not available"}{moveUnfinished ? " · move in progress" : ""}
    </p>
    {#if !archive.available && !archive.isDefault}
      <p class="hint">
        Reconnect its drive, or use Change… / Use default to point Silo elsewhere. Nothing is moved.
      </p>
    {/if}
    {#if !archive.isDefault}
      <div class="btnrow">
        <Button size="sm" disabled={lock.locked} title={lock.reason} onclick={onArchiveDefault}>
          Use default
        </Button>
      </div>
    {/if}
    {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
  {:else}
    <p class="path muted">
      {archive
        ? "No mods folder selected."
        : archiveLoadFailed
          ? "Couldn't read the library archive. Reopen Settings to retry."
          : "Loading…"}
    </p>
  {/if}
</SettingsCard>
