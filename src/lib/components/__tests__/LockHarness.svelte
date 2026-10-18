<script lang="ts">
  // Test-only: the Updates panel wired the way App wires it, its lock from the ops store
  // (foreground holders only: a background op such as the archive move leaves panels usable).
  import type { UpdateRow } from "../../types";
  import { blockedForeground } from "../../state/ops.svelte";
  import UpdatesPanel from "../panels/UpdatesPanel.svelte";

  let { row, onInstall }: { row: UpdateRow; onInstall: (r: UpdateRow) => void } = $props();
  const writeLock = $derived(blockedForeground("library-write"));
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
