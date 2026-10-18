<script lang="ts" module>
  export interface StatusStripItem {
    id: string;
    text: string;
    /** Link-button text, e.g. "Review". */
    action: string;
    run: () => void;
  }

  /** Items beyond this many are not shown. */
  export const MAX_STRIP_ITEMS = 2;
</script>

<script lang="ts">
  // One-row Library summary under the view header:
  //   <StatusStrip left="Ready to play · 142 of 737 active · Harvest 2026"
  //     items={[{ id: "upd", text: "4 updates", action: "Review", run: () => go("updates") }]} />
  // At most two items show (the first two: order them by importance).
  import Button from "../../ui/Button.svelte";

  interface Props {
    left: string;
    items?: StatusStripItem[];
  }

  let { left, items = [] }: Props = $props();

  const shown = $derived(items.slice(0, MAX_STRIP_ITEMS));
</script>

<div class="strip tnum">
  <span class="left">{left}</span>
  {#if shown.length}
    <ul class="items">
      {#each shown as item (item.id)}
        <li class="item">
          <span>{item.text}</span>
          <Button variant="link" size="sm" onclick={() => item.run()}>{item.action}</Button>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .strip {
    display: flex;
    align-items: center;
    gap: var(--sp-4);
    min-height: 2rem;
    color: var(--muted);
    font-size: var(--fs-sm);
    white-space: nowrap;
    overflow: hidden;
  }
  .left {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .items {
    display: flex;
    gap: var(--sp-4);
    margin: 0 0 0 auto;
    padding: 0;
    list-style: none;
  }
  .item {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    color: var(--text);
  }
</style>
