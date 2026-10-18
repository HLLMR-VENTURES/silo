<script lang="ts" generics="T extends string">
  // Segmented control (one of a few): <Segmented ariaLabel="View" {options} bind:value />.
  // A radiogroup with a roving tabindex: Tab enters on the selected segment; Arrow keys,
  // Home and End move and select (selection follows focus).
  import Icon from "./Icon.svelte";
  import type { ChoiceOption } from "./types";

  interface Props {
    options: ChoiceOption<T>[];
    value?: T;
    /** Accessible name for the group. */
    ariaLabel: string;
    size?: "sm" | "md";
    disabled?: boolean;
    onchange?: (value: T) => void;
    class?: string;
  }

  let {
    options,
    value = $bindable(),
    ariaLabel,
    size = "md",
    disabled = false,
    onchange,
    class: className = "",
  }: Props = $props();

  let buttons: HTMLButtonElement[] = $state([]);

  const selectedIndex = $derived(options.findIndex((o) => o.value === value));
  /** The one segment in the tab order: the selected one, else the first enabled one. */
  const tabStop = $derived(
    selectedIndex >= 0 && !options[selectedIndex].disabled
      ? selectedIndex
      : options.findIndex((o) => !o.disabled),
  );

  function select(i: number) {
    const opt = options[i];
    if (!opt || opt.disabled || disabled) return;
    if (opt.value !== value) {
      value = opt.value;
      onchange?.(opt.value);
    }
  }

  /** Next enabled index from `from` stepping by `step`, wrapping. */
  function step(from: number, dir: 1 | -1): number {
    const n = options.length;
    for (let k = 1; k <= n; k++) {
      const i = (((from + dir * k) % n) + n) % n;
      if (!options[i].disabled) return i;
    }
    return from;
  }

  function onkeydown(e: KeyboardEvent, i: number) {
    let next: number;
    switch (e.key) {
      case "ArrowRight":
      case "ArrowDown":
        next = step(i, 1);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        next = step(i, -1);
        break;
      case "Home":
        next = step(-1, 1);
        break;
      case "End":
        next = step(options.length, -1);
        break;
      default:
        return;
    }
    e.preventDefault();
    select(next);
    buttons[next]?.focus();
  }
</script>

<div class="segmented seg-{size} {className}" role="radiogroup" aria-label={ariaLabel}>
  {#each options as opt, i (opt.value)}
    <button
      bind:this={buttons[i]}
      type="button"
      role="radio"
      class="segment"
      aria-checked={opt.value === value}
      tabindex={i === tabStop ? 0 : -1}
      disabled={disabled || opt.disabled}
      onclick={() => select(i)}
      onkeydown={(e) => onkeydown(e, i)}
    >
      {#if opt.icon}<Icon name={opt.icon} size={size === "sm" ? 14 : 16} />{/if}
      <span>{opt.label}</span>
    </button>
  {/each}
</div>

<style>
  .segmented {
    display: inline-flex;
    gap: 2px;
    padding: 2px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--sunken);
  }
  .segment {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--sp-1);
    padding: 0 var(--sp-3);
    border: 0;
    border-radius: calc(var(--r-sm) - 2px);
    background: transparent;
    color: var(--muted);
    font: inherit;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--dur-1) var(--ease),
      color var(--dur-1) var(--ease);
  }
  .seg-md .segment {
    min-height: 1.875rem;
  }
  .seg-sm .segment {
    min-height: 1.5rem;
    font-size: var(--fs-sm);
  }
  .segment:hover:not(:disabled) {
    color: var(--text);
  }
  .segment[aria-checked="true"] {
    background: var(--raised);
    color: var(--text);
    box-shadow: var(--e1);
  }
  .segment:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  @media (prefers-reduced-motion: reduce) {
    .segment {
      transition: none;
    }
  }
</style>
