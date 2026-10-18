<script lang="ts">
  // Icon-only button: <IconButton icon="close" label="Close" onclick={...} />.
  // `label` is required: it becomes the accessible name and the hover tooltip. The tooltip is the
  // in-app one (use:tooltip), not a native `title`: a native tip can stay on screen after its button
  // is gone (a removed row, a closed drawer), the in-app one is removed with it.
  import type { HTMLButtonAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";
  import { tooltip } from "./tooltip";

  interface Props extends Omit<HTMLButtonAttributes, "class" | "onclick" | "children"> {
    label: string;
    icon: IconName;
    size?: "sm" | "md";
    variant?: "ghost" | "secondary";
    /** Toggle buttons: sets aria-pressed. Leave undefined for plain actions. */
    pressed?: boolean;
    disabled?: boolean;
    onclick?: (e: MouseEvent) => void;
    class?: string;
  }

  let {
    label,
    icon,
    size = "md",
    variant = "ghost",
    pressed,
    disabled = false,
    type = "button",
    onclick,
    class: className = "",
    ...rest
  }: Props = $props();
</script>

<button
  {...rest}
  {type}
  class="icon-btn icon-btn-{variant} icon-btn-{size} {className}"
  class:pressed
  aria-label={label}
  use:tooltip={label}
  aria-pressed={pressed}
  {disabled}
  {onclick}
>
  <Icon name={icon} size={16} filled={pressed === true && icon === "favorite"} />
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    color: var(--muted);
    background: transparent;
    cursor: pointer;
    transition:
      background-color var(--dur-1) var(--ease),
      color var(--dur-1) var(--ease);
  }
  .icon-btn-md {
    min-width: 2rem;
    min-height: 2rem;
  }
  .icon-btn-sm {
    min-width: 1.75rem;
    min-height: 1.75rem;
  }
  .icon-btn-secondary {
    background: var(--surface);
    border-color: var(--border-strong);
    color: var(--text);
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--sunken);
    color: var(--text);
  }
  .icon-btn.pressed {
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .icon-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  @media (prefers-reduced-motion: reduce) {
    .icon-btn {
      transition: none;
    }
  }
</style>
