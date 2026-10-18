<script lang="ts">
  // Settings → Diagnostics: copy or export what a bug report needs, or open the folder that
  // holds Silo's log (silo.log and its rotated copies).
  //   <DiagnosticsSettings {onCopyDiagnostics} {onExportReport} />
  import Button from "../../ui/Button.svelte";
  import SettingsCard from "./SettingsCard.svelte";
  import { folderOpener } from "./folders.svelte";

  interface Props {
    onCopyDiagnostics: () => void;
    onExportReport: () => void;
  }

  let { onCopyDiagnostics, onExportReport }: Props = $props();

  const folders = folderOpener();
</script>

<SettingsCard
  title="Diagnostics"
  description="A summary of your library, conflicts and health issues, plus Silo's recent log. Your user name is removed."
>
  {#snippet tools()}
    <Button size="sm" variant="ghost" icon="external" onclick={() => folders.open("logs")}>
      Open log folder
    </Button>
  {/snippet}
  {#if folders.error?.which === "logs"}<p class="hint">{folders.error.msg}</p>{/if}
  {#snippet actions()}
    <Button icon="copy" onclick={onCopyDiagnostics}>Copy diagnostics</Button>
    <Button icon="external" onclick={onExportReport}>Export report</Button>
  {/snippet}
</SettingsCard>
