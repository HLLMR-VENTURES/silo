<script lang="ts">
  // Shows its content inside `target` while that element is on screen, and keeps the content
  // mounted (hidden) while `keep` is true and the target is gone, so leaving a tab or a view
  // doesn't tear down work in progress:
  //   <KeepMounted target={lastRunSlot} keep={bisecting}><LogTriage ... /></KeepMounted>
  // Without `keep` it behaves like an {#if target}: mounted with the target, destroyed without it.
  // The content is one element moved between `target` and a hidden parking spot, never re-created.
  import type { Snippet } from "svelte";

  interface Props {
    /** Where the content shows (a tab panel's slot); null when that place isn't rendered. */
    target: HTMLElement | null;
    /** Keep the content alive while the target is gone. */
    keep?: boolean;
    children: Snippet;
  }

  let { target, keep = false, children }: Props = $props();

  /** Svelte action: move `node` into `to` when it's in the document, else back to where it was made. */
  function dock(node: HTMLElement, to: HTMLElement | null) {
    const home = node.parentElement;
    function place(t: HTMLElement | null) {
      const dest = t && t.isConnected ? t : home;
      if (dest && node.parentElement !== dest) dest.appendChild(node);
    }
    place(to);
    return {
      update: place,
      destroy() {
        node.remove();
      },
    };
  }
</script>

<div class="keep-park" hidden>
  {#if target || keep}
    <div class="keep-content" use:dock={target}>{@render children()}</div>
  {/if}
</div>
