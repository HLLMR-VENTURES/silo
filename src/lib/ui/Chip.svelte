<script lang="ts">
  // Active filter chip: <Chip label="Category: Tractors" onremove={() => clear("category")} />.
  // With `onremove` it gets a ✕ button named "Remove filter: {label}".
  import type { HTMLAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import { tooltip } from "./tooltip";

  interface Props extends Omit<HTMLAttributes<HTMLSpanElement>, "class" | "children"> {
    label: string;
    icon?: IconName;
    onremove?: () => void;
    class?: string;
  }

  let { label, icon, onremove, class: className = "", ...rest }: Props = $props();
</script>

<span {...rest} class="chip {className}" class:removable={!!onremove}>
  {#if icon}<Icon name={icon} size={14} />{/if}
  <span class="label">{label}</span>
  {#if onremove}
    <button
      type="button"
      class="remove"
      aria-label="Remove filter: {label}"
      use:tooltip={"Remove filter"}
      onclick={onremove}
    >
      <Icon name="close" size={12} />
    </button>
  {/if}
</span>

<style>
  .chip {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-1);
    min-height: 1.75rem;
    max-width: 100%;
    padding: 0 var(--sp-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-pill);
    background: var(--surface);
    color: var(--text);
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
  }
  .chip.removable {
    padding-right: var(--sp-1);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .remove {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    /* 24 px target (WCAG 2.5.8). */
    width: 1.5rem;
    height: 1.5rem;
    padding: 0;
    border: 0;
    border-radius: var(--r-pill);
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .remove:hover {
    background: var(--sunken);
    color: var(--text);
  }
</style>
