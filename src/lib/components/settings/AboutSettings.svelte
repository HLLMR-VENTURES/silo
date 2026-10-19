<script lang="ts">
  // Settings → About: the version, the last scan, a ready self-update, and Silo's links.
  //   <AboutSettings {appVer} {scanMs} {update} {updating} {updatePct} {onApplyUpdate} />
  import Button from "../../ui/Button.svelte";
  import { openExternal } from "../../api";
  import { CHANGELOG_URL, DISCORD_URL, DISCUSSIONS_URL, SOURCE_URL, WEBSITE_URL } from "../../links";
  import { reportError } from "../../state/notify.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  interface Props {
    appVer: string | null;
    /** How long the last library scan took. */
    scanMs?: number | null;
    /** A Silo update that's ready to install, if any. */
    update?: { version: string } | null;
    updating?: boolean;
    updatePct?: number | null;
    onApplyUpdate?: () => void;
  }

  let {
    appVer,
    scanMs = null,
    update = null,
    updating = false,
    updatePct = null,
    onApplyUpdate,
  }: Props = $props();

  const LINKS = [
    { label: "Website", url: WEBSITE_URL },
    { label: "Source code", url: SOURCE_URL },
    { label: "Changelog", url: CHANGELOG_URL },
    { label: "Discord", url: DISCORD_URL },
    { label: "GitHub Discussions", url: DISCUSSIONS_URL },
  ];

  function open(url: string) {
    openExternal(url).catch((e) => reportError(e, "Opening the link"));
  }
</script>

<SettingsCard title="Silo">
  <p class="version tnum">Silo {appVer ?? "—"}</p>
  {#if scanMs !== null}
    <p class="hint tnum">Last library scan: scanned in {scanMs} ms.</p>
  {/if}
  {#if update}
    <p class="hint">Silo {update.version} is ready to install. Silo restarts to finish.</p>
    <div class="btnrow">
      <Button
        variant="primary"
        icon={updating ? undefined : "update"}
        disabled={updating}
        title="Download and install Silo {update.version}, then restart"
        onclick={() => onApplyUpdate?.()}
      >
        {#if updating}
          {updatePct != null ? `Updating… ${updatePct}%` : "Updating…"}
        {:else}
          Restart to update
        {/if}
      </Button>
    </div>
  {/if}
</SettingsCard>

<SettingsCard title="Links">
  {#snippet actions()}
    {#each LINKS as l (l.url)}
      <Button variant="ghost" size="sm" icon="external" onclick={() => open(l.url)}>{l.label}</Button>
    {/each}
  {/snippet}
</SettingsCard>

<style>
  .version {
    font-weight: 600;
  }
</style>
