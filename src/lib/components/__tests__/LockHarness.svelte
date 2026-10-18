<script lang="ts">
  // Test-only: the Updates panel wired the way App wires it, its lock from the ops store
  // (foreground holders only: a background op such as the archive move leaves panels usable).
  import type { UpdateRow } from "../../types";
  import { blockedForeground } from "../../state/ops.svelte";
  import { NO_SECTIONS } from "../../updates/sections";
  import UpdatesPanel from "../panels/UpdatesPanel.svelte";

  let { row, onInstall }: { row: UpdateRow; onInstall: (r: UpdateRow) => void } = $props();
  const writeLock = $derived(blockedForeground("library-write"));
  const noop = () => {};
</script>

<UpdatesPanel
  sections={{ ...NO_SECTIONS, ready: [row] }}
  checked={true}
  checking={false}
  catalogError={null}
  rowStates={{}}
  updatingAll={false}
  lockReason={writeLock.reason}
  checkDisabledReason=""
  foreign={[]}
  onCheck={noop}
  {onInstall}
  onUpdateAll={noop}
  onOpenPage={noop}
  onUseDropped={noop}
  onOpenSource={noop}
  onMarkChecked={noop}
  onRecheck={noop}
  onCopyDetails={noop}
/>
