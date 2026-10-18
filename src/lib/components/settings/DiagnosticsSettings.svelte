<script lang="ts">
  // Settings → Diagnostics: copy or export what a bug report needs.
  //   <DiagnosticsSettings {onCopyDiagnostics} {onExportReport} />
  import Button from "../../ui/Button.svelte";
  import SettingsCard from "./SettingsCard.svelte";

  interface Props {
    onCopyDiagnostics: () => void;
    onExportReport: () => void;
  }

  let { onCopyDiagnostics, onExportReport }: Props = $props();
</script>

<SettingsCard
  title="Diagnostics"
  description="A summary of your library, conflicts and health issues, plus Silo's recent log. Your user name is removed."
>
  {#snippet actions()}
    <Button icon="copy" onclick={onCopyDiagnostics}>Copy diagnostics</Button>
    <Button icon="external" onclick={onExportReport}>Export report</Button>
  {/snippet}
</SettingsCard>
