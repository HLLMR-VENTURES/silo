<script lang="ts" generics="T extends string">
  // Native radios in a fieldset: <RadioGroup legend="When FS25 updates" {options} bind:value />.
  // Arrow keys move between options natively.
  import type { ChoiceOption } from "./types";

  interface Props {
    legend: string;
    /** Hide the legend visually (it stays the group's accessible name). */
    hideLegend?: boolean;
    options: ChoiceOption<T>[];
    value?: T;
    /** Form name; generated when omitted. */
    name?: string;
    disabled?: boolean;
    onchange?: (value: T) => void;
    class?: string;
  }

  let {
    legend,
    hideLegend = false,
    options,
    value = $bindable(),
    name,
    disabled = false,
    onchange,
    class: className = "",
  }: Props = $props();

  const uid = $props.id();
  const groupName = $derived(name ?? `${uid}-radio`);
</script>

<fieldset class="radio-group {className}" role="radiogroup" {disabled}>
  <legend class:sr-only={hideLegend}>{legend}</legend>
  {#each options as opt (opt.value)}
    <label class="option" class:disabled={disabled || opt.disabled}>
      <input
        type="radio"
        name={groupName}
        value={opt.value}
        bind:group={value}
        disabled={opt.disabled}
        onchange={() => onchange?.(opt.value)}
      />
      <span class="text">
        <span class="label">{opt.label}</span>
        {#if opt.hint}<span class="hint">{opt.hint}</span>{/if}
      </span>
    </label>
  {/each}
</fieldset>

<style>
  .radio-group {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin: 0;
    padding: 0;
    border: 0;
    min-width: 0;
  }
  legend {
    padding: 0;
    margin-bottom: var(--sp-2);
    font-weight: 600;
    color: var(--text);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .option {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-2);
    color: var(--text);
    cursor: pointer;
  }
  input {
    width: 1rem;
    height: 1rem;
    margin: 0.125rem 0 0;
    accent-color: var(--primary);
    cursor: inherit;
  }
  .text {
    display: flex;
    flex-direction: column;
  }
  .hint {
    color: var(--muted);
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
  }
  .disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
</style>
