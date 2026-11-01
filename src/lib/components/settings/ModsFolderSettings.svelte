<script lang="ts">
  // Settings → Mods folder: the folder Silo manages (and FS25 loads), other folders Silo knows,
  // and where the game is installed.
  //   <ModsFolderSettings {roots} {modsStatus} {gameInfo} {lockReason} {scanning} {moveUnfinished}
  //     {onChangeModsFolder} {onUseGameFolder} {onSwitchModsFolder} />
  import Button from "../../ui/Button.svelte";
  import type { ModsRootStatus } from "../../api";
  import type { GameInfo } from "../../types";
  import SettingsCard from "./SettingsCard.svelte";
  import { nMods } from "../../firstRun/firstRun";
  import { folderOpener } from "./folders.svelte";
  import { settingsLock } from "./lock";
  import { displayPath } from "../../format";

  interface Props {
    roots: string[];
    modsStatus: ModsRootStatus | null;
    gameInfo: GameInfo | null;
    /** Why library-changing controls are off (a library write is running), or "". */
    lockReason: string;
    scanning: boolean;
    /** An archive move is running or paused. */
    moveUnfinished: boolean;
    onChangeModsFolder: () => void;
    onUseGameFolder: () => void;
    onSwitchModsFolder: (path: string) => void;
  }

  let {
    roots,
    modsStatus,
    gameInfo,
    lockReason,
    scanning,
    moveUnfinished,
    onChangeModsFolder,
    onUseGameFolder,
    onSwitchModsFolder,
  }: Props = $props();

  const lock = $derived(settingsLock({ lockReason, scanning, moveUnfinished }));
  const others = $derived((modsStatus?.known ?? []).filter((k) => !k.current));
  const folders = folderOpener();

  function sourceLabel(s: ModsRootStatus["source"]): string {
    return s === "user"
      ? "Chosen in Silo"
      : s === "gameOverride"
        ? "Following the game's setting"
        : "The game's default";
  }
</script>

<SettingsCard title="Mods folder" description="The folder Silo manages and Farming Simulator 25 loads mods from.">
  {#snippet tools()}
    {#if roots.length}
      <Button size="sm" variant="ghost" icon="external" onclick={() => folders.open("modsFolder")}>
        Open folder
      </Button>
    {/if}
    <Button size="sm" disabled={lock.locked} title={lock.reason} onclick={onChangeModsFolder}>
      Change…
    </Button>
  {/snippet}
  {#if folders.error?.which === "modsFolder"}<p class="hint">{folders.error.msg}</p>{/if}
  {#if modsStatus?.path}
    <p class="path" class:muted={!modsStatus.available}>{modsStatus.path}</p>
    <p class="hint">
      {sourceLabel(modsStatus.source)}{modsStatus.available ? "" : " · not available"}
    </p>
  {:else}
    <p class="path muted">No mods folder detected.</p>
  {/if}
  {#if modsStatus?.unresolvedOverride}
    <p class="hint">
      Farming Simulator 25 is set to load mods from <span class="tnum">{modsStatus.unresolvedOverride}</span>,
      which Silo can't find on this computer. Choose the folder with Change….
    </p>
  {/if}
  {#if modsStatus?.gameSettingProblem}
    <p class="hint">Silo can't change the game's mods folder setting. {modsStatus.gameSettingProblem}</p>
  {/if}
  {#if modsStatus?.path && modsStatus.source === "user"}
    <div class="btnrow">
      <Button size="sm" disabled={lock.locked} title={lock.reason} onclick={onUseGameFolder}>
        Use the game's folder
      </Button>
    </div>
  {/if}
  {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
</SettingsCard>

{#if others.length}
  <SettingsCard
    title="Other folders Silo knows"
    description="Switch to a mods folder you used before. Each keeps its own library."
  >
    <ul class="known">
      {#each others as k (k.path)}
        <li>
          <span class="known-path">{k.path}</span>
          <span class="hint tnum">{nMods(k.modCount)}</span>
          <Button
            size="sm"
            disabled={lock.locked}
            title={lock.reason}
            aria-label="Switch to {k.path}"
            onclick={() => onSwitchModsFolder(k.path)}
          >
            Switch
          </Button>
        </li>
      {/each}
    </ul>
    {#if lock.locked}<p class="reason">{lock.reason}</p>{/if}
  </SettingsCard>
{/if}

<SettingsCard title="Game">
  {#snippet tools()}
    {#if gameInfo}
      <Button size="sm" variant="ghost" icon="external" onclick={() => folders.open("gameInstall")}>
        Open folder
      </Button>
    {/if}
  {/snippet}
  {#if folders.error?.which === "gameInstall"}<p class="hint">{folders.error.msg}</p>{/if}
  {#if gameInfo}
    <p class="path">{displayPath(gameInfo.installDir)}</p>
  {:else}
    <p class="path muted">Farming Simulator 25 install not found. The Play button is hidden.</p>
  {/if}
</SettingsCard>

<style>
  .known {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .known li {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
  }
  .known-path {
    flex: 1;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    word-break: break-all;
  }
</style>
