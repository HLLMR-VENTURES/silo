<script lang="ts">
  // On/off switch: <Switch bind:checked label="Show hidden mods" />.
  // A native <button role="switch">, so Space and Enter both toggle. Give it a visible `label`,
  // or `ariaLabel` when the text naming it lives elsewhere.
  // `controlled`: a click only asks (onchange gets the wanted value) and `checked` follows the
  // parent, for a switch whose change can be refused or lands later (a mod's Active switch).
  import type { HTMLButtonAttributes } from "svelte/elements";

  interface Props
    extends Omit<HTMLButtonAttributes, "class" | "onclick" | "onchange" | "children" | "role"> {
    checked?: boolean;
    /** Visible text beside the switch (also its accessible name). */
    label?: string;
    /** Accessible name when there is no visible label. */
    ariaLabel?: string;
    disabled?: boolean;
    onchange?: (checked: boolean) => void;
    /** Don't flip `checked` on click; the parent sets it (after onchange). */
    controlled?: boolean;
    class?: string;
  }

  let {
    checked = $bindable(false),
    label,
    ariaLabel,
    disabled = false,
    onchange,
    controlled = false,
    class: className = "",
    ...rest
  }: Props = $props();

  function toggle() {
    if (controlled) {
      onchange?.(!checked);
      return;
    }
    checked = !checked;
    onchange?.(checked);
  }
</script>

<button
  {...rest}
  type="button"
  role="switch"
  aria-checked={checked}
  aria-label={label ? undefined : ariaLabel}
  class="switch {className}"
  {disabled}
  onclick={toggle}
>
  <span class="track" class:on={checked} aria-hidden="true"><span class="thumb"></span></span>
  {#if label}<span class="label">{label}</span>{/if}
</button>

<style>
  .switch {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 1.75rem;
    min-width: 2.75rem;
    padding: var(--sp-1);
    margin: calc(-1 * var(--sp-1));
    border: 0;
    border-radius: var(--r-sm);
    background: none;
    color: var(--text);
    font: inherit;
    text-align: start;
    cursor: pointer;
  }
  .track {
    position: relative;
    flex: 0 0 auto;
    width: 2.25rem;
    height: 1.25rem;
    border-radius: var(--r-pill);
    background: var(--border-strong);
    transition: background-color var(--dur-2) var(--ease);
  }
  .track.on {
    background: var(--primary);
  }
  .thumb {
    position: absolute;
    top: 0.125rem;
    left: 0.125rem;
    width: 1rem;
    height: 1rem;
    border-radius: 50%;
    background: var(--raised);
    box-shadow: 0 1px 2px color-mix(in srgb, var(--text) 30%, transparent);
    transition: transform var(--dur-2) var(--ease);
  }
  .track.on .thumb {
    transform: translateX(1rem);
  }
  .switch:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  @media (prefers-reduced-motion: reduce) {
    .track,
    .thumb {
      transition: none;
    }
  }
</style>
