<script lang="ts">
  // Test-only: Field wired to a TextField the way views use it.
  import Field from "../Field.svelte";
  import TextField from "../TextField.svelte";

  let { hint, error }: { hint?: string; error?: string } = $props();
  let value = $state("");
</script>

<Field label="Mods folder" {hint} {error}>
  {#snippet control({ id, describedBy, invalid })}
    <TextField {id} aria-describedby={describedBy} {invalid} bind:value />
  {/snippet}
</Field>
