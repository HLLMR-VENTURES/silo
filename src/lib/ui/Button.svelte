<script lang="ts">
  // <Button variant="primary" icon="play" onclick={...}>Play</Button>. Renders <a> when `href` is set.
  // `loading` shows a spinner over the (hidden) label so the width holds, sets aria-busy and
  // swallows clicks without dropping focus.
  import type { Snippet } from "svelte";
  import type { HTMLAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  interface Props extends Omit<HTMLAttributes<HTMLElement>, "class" | "onclick"> {
    variant?: "primary" | "secondary" | "ghost" | "danger" | "link";
    size?: "sm" | "md" | "lg";
    /** Leading icon. */
    icon?: IconName;
    loading?: boolean;
    disabled?: boolean;
    type?: "button" | "submit" | "reset";
    /** Render a link instead of a button. */
    href?: string;
    target?: string;
    rel?: string;
    onclick?: (e: MouseEvent) => void;
    class?: string;
    children?: Snippet;
  }

  let {
    variant = "secondary",
    size = "md",
    icon,
    loading = false,
    disabled = false,
    type = "button",
    href,
    onclick,
    class: className = "",
    children,
    ...rest
  }: Props = $props();

  const iconSize = $derived(size === "sm" ? 14 : size === "lg" ? 18 : 16);
  const inert = $derived(loading || disabled);
  const cls = $derived(`btn btn-${variant} btn-${size} ${className}`.trim());

  function handleClick(e: MouseEvent) {
    if (inert) {
      e.preventDefault();
      e.stopImmediatePropagation();
      return;
    }
    onclick?.(e);
  }
</script>

{#snippet inner()}
  <span class="content" class:hidden={loading}>
    {#if icon}<Icon name={icon} size={iconSize} />{/if}
    {@render children?.()}
  </span>
  {#if loading}<span class="spinner" aria-hidden="true"></span>{/if}
{/snippet}

{#if href !== undefined}
  <a
    {...rest}
    class={cls}
    href={disabled ? undefined : href}
    aria-disabled={inert ? "true" : undefined}
    aria-busy={loading ? "true" : undefined}
    onclick={handleClick}
  >
    {@render inner()}
  </a>
{:else}
  <button
    {...rest}
    class={cls}
    {type}
    {disabled}
    aria-disabled={loading ? "true" : undefined}
    aria-busy={loading ? "true" : undefined}
    onclick={handleClick}
  >
    {@render inner()}
  </button>
{/if}

<style>
  .btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--sp-2);
    border: 1px solid transparent;
    border-radius: var(--r-sm);
    font: inherit;
    font-weight: 600;
    line-height: 1;
    white-space: nowrap;
    text-decoration: none;
    cursor: pointer;
    user-select: none;
    transition:
      background-color var(--dur-1) var(--ease),
      border-color var(--dur-1) var(--ease),
      color var(--dur-1) var(--ease);
  }

  .btn-sm {
    min-height: 1.75rem;
    padding: 0 var(--sp-3);
    font-size: var(--fs-sm);
  }
  .btn-md {
    min-height: 2.25rem;
    padding: 0 var(--sp-4);
    font-size: var(--fs-md);
  }
  .btn-lg {
    min-height: 2.75rem;
    padding: 0 var(--sp-5);
    font-size: var(--fs-lg);
  }

  .btn-primary {
    background: var(--primary);
    color: var(--on-primary);
  }
  .btn-primary:hover:not(:disabled, [aria-disabled="true"]) {
    background: var(--primary-hover);
  }

  .btn-danger {
    background: var(--danger);
    color: var(--on-danger);
  }
  .btn-danger:hover:not(:disabled, [aria-disabled="true"]) {
    background: color-mix(in srgb, var(--danger) 88%, var(--text));
  }

  /* Colored fills: a --bg ring inside the global outline keeps the ring's contrast. */
  .btn-primary:focus-visible,
  .btn-danger:focus-visible {
    box-shadow: 0 0 0 2px var(--bg);
  }

  .btn-secondary {
    background: var(--surface);
    border-color: var(--border-strong);
    color: var(--text);
  }
  .btn-secondary:hover:not(:disabled, [aria-disabled="true"]) {
    background: var(--sunken);
  }

  .btn-ghost {
    background: transparent;
    color: var(--text);
  }
  .btn-ghost:hover:not(:disabled, [aria-disabled="true"]) {
    background: var(--sunken);
  }

  .btn-link {
    min-height: 0;
    padding: 0;
    border: 0;
    background: none;
    color: var(--primary-text);
    font-weight: 500;
    text-decoration: underline;
    text-underline-offset: 0.15em;
  }
  .btn-link:hover:not(:disabled, [aria-disabled="true"]) {
    text-decoration-thickness: 2px;
  }

  .btn:disabled,
  .btn[aria-disabled="true"] {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .btn[aria-busy="true"] {
    opacity: 1;
    cursor: progress;
  }

  .content {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
  }
  .content.hidden {
    visibility: hidden;
  }

  .spinner {
    position: absolute;
    inset: 0;
    margin: auto;
    width: 1em;
    height: 1em;
    border: 2px solid currentColor;
    border-right-color: transparent;
    border-radius: 50%;
    animation: spin 0.7s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(1turn);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .btn {
      transition: none;
    }
    .spinner {
      animation-duration: 1.6s;
    }
  }
</style>
