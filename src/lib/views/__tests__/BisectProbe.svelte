<script lang="ts">
  // Test-only stand-in for BisectFlow: reports onBisecting(false) when it's torn down, as the
  // real flow does when it unmounts mid-run.
  import { onDestroy } from "svelte";

  let { onBisecting }: { onBisecting: (active: boolean) => void } = $props();
  let rounds = $state(0);

  onDestroy(() => onBisecting(false));
</script>

<button type="button" onclick={() => rounds++}>Next round ({rounds})</button>
