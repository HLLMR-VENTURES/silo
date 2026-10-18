<script lang="ts">
  let {
    categories,
    x,
    y,
    onSelect,
    onReset,
    onClose,
  }: {
    categories: string[];
    x: number;
    y: number;
    onSelect: (category: string) => void;
    onReset: () => void;
    onClose: () => void;
  } = $props();
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div
  class="menu"
  style="left: {Math.min(x, window.innerWidth - 220)}px; top: {Math.min(
    y,
    window.innerHeight - 420,
  )}px"
>
  <div class="menu-head">Set category</div>
  {#each categories as c (c)}
    <button class="menu-item" onclick={() => onSelect(c)}>
      {c}
    </button>
  {/each}
  <button class="menu-item reset" onclick={onReset}>
    ↺ Reset to auto
  </button>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .menu {
    position: fixed;
    z-index: 50;
    width: 208px;
    max-height: 400px;
    overflow-y: auto;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    padding: 6px;
    scrollbar-width: thin;
  }
  .menu-head {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    padding: 6px 10px;
  }
  .menu-item {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text);
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
  }
  .menu-item:hover {
    background: color-mix(in srgb, var(--primary) 14%, transparent);
    color: var(--primary-text);
  }
  .menu-item.reset {
    margin-top: 4px;
    border-top: 1px solid var(--border);
    border-radius: 0;
    color: var(--text-muted);
  }
</style>
