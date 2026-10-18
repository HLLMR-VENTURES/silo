<script lang="ts" module>
  import type { IconName } from "../ui/icons";

  export type ContextMenuItem =
    | {
        label: string;
        onClick: () => void;
        icon?: IconName;
        /** Draw the icon filled (e.g. a set favorite star). */
        iconFilled?: boolean;
        danger?: boolean;
        disabled?: boolean;
      }
    | { separator: true };
</script>

<script lang="ts">
  import Icon from "../ui/Icon.svelte";

  // A generic right-click menu, positioned at the cursor. Mirrors CategoryMenu's
  // backdrop + fixed-position pattern; closes on pick, outside click, or Escape.
  let {
    x,
    y,
    items,
    onClose,
  }: {
    x: number;
    y: number;
    items: ContextMenuItem[];
    onClose: () => void;
  } = $props();

  // Clamp so the menu never spills off-screen (rough height estimate per item).
  const width = 220;
  const estHeight = $derived(
    items.reduce((h, it) => h + ("separator" in it ? 9 : 34), 12),
  );
  const left = $derived(Math.min(x, window.innerWidth - width - 8));
  const top = $derived(Math.min(y, Math.max(8, window.innerHeight - estHeight - 8)));

  function pick(fn: () => void) {
    fn();
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

<svelte:window on:keydown={onKey} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose} oncontextmenu={(e) => { e.preventDefault(); onClose(); }}></div>
<div class="menu" style="left: {left}px; top: {top}px; width: {width}px" role="menu">
  {#each items as item, i (i)}
    {#if "separator" in item}
      <div class="sep"></div>
    {:else}
      <button
        class="menu-item"
        class:danger={item.danger}
        disabled={item.disabled}
        onclick={() => pick(item.onClick)}
        role="menuitem"
      >
        {#if item.icon}<span class="ico"><Icon name={item.icon} size={14} filled={item.iconFilled} /></span>{/if}
        <span class="lbl">{item.label}</span>
      </button>
    {/if}
  {/each}
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
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    padding: 6px;
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text);
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-sm);
    cursor: pointer;
  }
  .menu-item:hover:not(:disabled) {
    background: color-mix(in srgb, var(--primary) 14%, transparent);
    color: var(--primary-text);
  }
  .menu-item:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .menu-item.danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
    color: var(--danger);
  }
  .ico {
    flex: 0 0 auto;
    width: 1rem;
    display: inline-flex;
    justify-content: center;
    opacity: 0.9;
  }
  .lbl {
    flex: 1 1 auto;
  }
  .sep {
    height: 1px;
    background: var(--border);
    margin: 4px 6px;
  }
</style>
