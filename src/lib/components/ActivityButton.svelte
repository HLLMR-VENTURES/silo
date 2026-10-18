<script lang="ts">
  // Top-bar Activity button: a bell with a dot while something runs or an error came in since
  // the panel was last opened. Opens ActivityPanel in a popover; opening marks errors seen.
  import { plural } from "../plural";
  import IconButton from "../ui/IconButton.svelte";
  import Popover from "../ui/Popover.svelte";
  import { markActivitySeen, unseenErrorCount } from "../state/notify.svelte";
  import { running } from "../state/ops.svelte";
  import ActivityPanel from "./ActivityPanel.svelte";

  let open = $state(false);
  let anchor = $state<HTMLElement | null>(null);
  const uid = $props.id();
  const panelId = `activity-${uid}`;

  const busy = $derived(running.length);
  const unseen = $derived(unseenErrorCount());
  const label = $derived.by(() => {
    const parts: string[] = [];
    if (busy > 0) parts.push(`${busy} running`);
    if (unseen > 0) parts.push(`${unseen} new ${plural(unseen, { one: "error", other: "errors" })}`);
    return parts.length ? `Activity, ${parts.join(", ")}` : "Activity";
  });

  function toggle() {
    open = !open;
    if (open) markActivitySeen();
  }

  // Errors that arrive while the panel is open are seen too.
  $effect(() => {
    if (open && unseen > 0) markActivitySeen();
  });
</script>

<span class="activity" bind:this={anchor}>
  <IconButton
    icon="activity"
    {label}
    aria-haspopup="dialog"
    aria-expanded={open}
    aria-controls={open ? panelId : undefined}
    onclick={toggle}
  />
  {#if unseen > 0}
    <span class="dot dot-error" aria-hidden="true"></span>
  {:else if busy > 0}
    <span class="dot dot-busy" aria-hidden="true"></span>
  {/if}
</span>

<Popover bind:open {anchor} placement="bottom-end" id={panelId} role="dialog" aria-label="Activity">
  <ActivityPanel />
</Popover>

<style>
  .activity {
    position: relative;
    display: inline-flex;
  }
  .dot {
    position: absolute;
    top: 0.25rem;
    right: 0.25rem;
    width: 0.5rem;
    height: 0.5rem;
    border-radius: var(--r-pill);
    box-shadow: 0 0 0 2px var(--surface);
    pointer-events: none;
  }
  .dot-busy {
    background: var(--info);
  }
  .dot-error {
    background: var(--danger);
  }
</style>
