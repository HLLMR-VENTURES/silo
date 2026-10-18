<script lang="ts">
  // Test-only: the Updates panel wired the way App wires it, its lock from the ops store.
  import type { UpdateRow } from "../../types";
  import { blocked } from "../../state/ops.svelte";
  import UpdatesPanel from "../panels/UpdatesPanel.svelte";

  let { row, onInstall }: { row: UpdateRow; onInstall: (r: UpdateRow) => void } = $props();
  const writeLock = $derived(blocked("library-write"));
</script>

<UpdatesPanel
  updateResults={[row]}
  availableUpdates={[row]}
  updateChecking={false}
  lockReason={writeLock.reason}
  checkYourself={[]}
  {onInstall}
  onOpenSource={() => {}}
  onMarkChecked={() => {}}
  onClose={() => {}}
/>
