<script lang="ts">
  // Component form of the `tooltip` action, for triggers you can't put `use:` on (components):
  //   <Tooltip text="Rescan library">{#snippet children()}<IconButton icon="rescan" label="Rescan" />{/snippet}</Tooltip>
  // The tip attaches to the snippet's first element. Prefer `use:tooltip={"..."}` on plain elements.
  // A tooltip is never the only place for essential information (see tooltip.ts).
  import type { Snippet } from "svelte";
  import { tooltip } from "./tooltip";

  let { text, children }: { text: string; children: Snippet } = $props();

  let wrap = $state<HTMLElement | null>(null);

  $effect(() => {
    const el = wrap?.firstElementChild;
    if (!(el instanceof HTMLElement)) return;
    const handle = tooltip(el, text);
    return () => handle.destroy();
  });
</script>

<span class="tooltip-wrap" bind:this={wrap}>{@render children()}</span>

<style>
  .tooltip-wrap {
    display: contents;
  }
</style>
