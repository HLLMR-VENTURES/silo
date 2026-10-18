<script lang="ts">
  // Single-line text input: <TextField bind:value placeholder="Search mods" />.
  // Pair with <Field> for a label, hint and error. Focus it from a parent with `bind:ref` or
  // `bind:this` + `.focus()`.
  import type { HTMLInputAttributes } from "svelte/elements";

  interface Props extends Omit<HTMLInputAttributes, "class" | "value" | "size"> {
    value?: string;
    type?: "text" | "search" | "url" | "email" | "password" | "number" | "tel";
    invalid?: boolean;
    size?: "sm" | "md";
    /** The <input> element. */
    ref?: HTMLInputElement;
    class?: string;
  }

  let {
    value = $bindable(""),
    type = "text",
    invalid = false,
    size = "md",
    ref = $bindable(),
    class: className = "",
    ...rest
  }: Props = $props();

  /** Move keyboard focus into the input. */
  export function focus() {
    ref?.focus();
  }
</script>

<input
  {...rest}
  bind:this={ref}
  bind:value
  {type}
  class="text-field tf-{size} {className}"
  aria-invalid={invalid ? "true" : undefined}
/>

<style>
  .text-field {
    width: 100%;
    min-width: 0;
    padding: 0 var(--sp-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
  }
  .tf-md {
    min-height: 2.25rem;
  }
  .tf-sm {
    min-height: 1.75rem;
    font-size: var(--fs-sm);
  }
  .text-field::placeholder {
    color: var(--muted);
    opacity: 1;
  }
  .text-field:hover:not(:disabled) {
    border-color: var(--text);
  }
  .text-field[aria-invalid="true"] {
    border-color: var(--danger);
  }
  .text-field:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
