<script lang="ts">
  // Native checkbox with a label: <Checkbox bind:checked label="Select all" indeterminate={some} />.
  // `class` goes on the <label> wrapper; other attributes go on the <input>.
  import type { Snippet } from "svelte";
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLInputAttributes, "class" | "type" | "checked" | "children"> {
    checked?: boolean;
    /** Mixed state ("some selected"); wired to the element property, not an attribute. */
    indeterminate?: boolean;
    label?: string;
    disabled?: boolean;
    class?: string;
    /** Rich label content, used instead of `label`. */
    children?: Snippet;
  }

  let {
    checked = $bindable(false),
    indeterminate = $bindable(false),
    label,
    disabled = false,
    class: className = "",
    children,
    ...rest
  }: Props = $props();
</script>

<label class="checkbox {className}" class:disabled>
  <input {...rest} type="checkbox" bind:checked bind:indeterminate {disabled} />
  {#if children}
    <span class="label">{@render children()}</span>
  {:else if label}
    <span class="label">{label}</span>
  {/if}
</label>

<style>
  .checkbox {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-2);
    min-height: 1.75rem;
    color: var(--text);
    cursor: pointer;
  }
  input {
    width: 1rem;
    height: 1rem;
    margin: 0;
    accent-color: var(--primary);
    cursor: inherit;
  }
  .disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
