<script lang="ts" generics="T extends string">
  // Styled native <select>: <Select {options} bind:value /> or pass <option>s as children.
  // `class` goes on the wrapper; other attributes (id, aria-*) go on the <select>.
  import type { Snippet } from "svelte";
  import type { HTMLSelectAttributes } from "svelte/elements";
  import Icon from "./Icon.svelte";
  import type { ChoiceOption } from "./types";

  interface Props extends Omit<HTMLSelectAttributes, "class" | "value" | "children" | "size"> {
    options?: ChoiceOption<T>[];
    value?: T;
    size?: "sm" | "md";
    invalid?: boolean;
    class?: string;
    /** Raw <option>/<optgroup> markup, used when `options` is not given. */
    children?: Snippet;
  }

  let {
    options,
    value = $bindable(),
    size = "md",
    invalid = false,
    class: className = "",
    children,
    ...rest
  }: Props = $props();
</script>

<span class="select select-{size} {className}">
  <select {...rest} bind:value aria-invalid={invalid ? "true" : undefined}>
    {#if options}
      {#each options as opt (opt.value)}
        <option value={opt.value} disabled={opt.disabled}>{opt.label}</option>
      {/each}
    {:else}
      {@render children?.()}
    {/if}
  </select>
  <span class="chevron" aria-hidden="true"><Icon name="chevronDown" size={14} /></span>
</span>

<style>
  .select {
    position: relative;
    display: inline-flex;
    align-items: center;
    min-width: 0;
  }
  select {
    appearance: none;
    width: 100%;
    padding: 0 calc(var(--sp-3) + 1.25rem) 0 var(--sp-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  /* The option list (global.css sets the same for every native select). */
  select :global(option),
  select :global(optgroup) {
    background: var(--raised);
    color: var(--text);
  }
  .select-md select {
    min-height: 2.25rem;
  }
  .select-sm select {
    min-height: 1.75rem;
    font-size: var(--fs-sm);
  }
  select:hover:not(:disabled) {
    border-color: var(--text);
  }
  select[aria-invalid="true"] {
    border-color: var(--danger);
  }
  select:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .chevron {
    position: absolute;
    right: var(--sp-3);
    display: inline-flex;
    color: var(--muted);
    pointer-events: none;
  }
</style>
