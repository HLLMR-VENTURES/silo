<script lang="ts">
  // Label + control + hint + error. The control snippet receives the ids to wire up:
  //   <Field label="Mods folder" hint="Where FS25 loads mods from" {error}>
  //     {#snippet control({ id, describedBy, invalid })}
  //       <TextField {id} aria-describedby={describedBy} {invalid} bind:value />
  //     {/snippet}
  //   </Field>
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import type { FieldControlProps } from "./types";

  interface Props {
    label: string;
    hint?: string;
    /** Error message; a non-empty string marks the control invalid. */
    error?: string;
    /** Adds a "required" marker to the label (the control sets its own `required`). */
    required?: boolean;
    control: Snippet<[FieldControlProps]>;
    class?: string;
  }

  let { label, hint, error, required = false, control, class: className = "" }: Props = $props();

  const uid = $props.id();
  const id = `${uid}-control`;
  const hintId = `${uid}-hint`;
  const errorId = `${uid}-error`;

  const invalid = $derived(!!error);
  const describedBy = $derived(
    [hint ? hintId : "", error ? errorId : ""].filter(Boolean).join(" ") || undefined,
  );
</script>

<div class="field {className}">
  <label class="label" for={id}>
    {label}{#if required}<span class="required" aria-hidden="true"> *</span>{/if}
  </label>
  {@render control({ id, describedBy, invalid })}
  {#if hint}<p class="hint" id={hintId}>{hint}</p>{/if}
  <!-- Always present so screen readers announce an error when it appears. -->
  <p class="error" class:shown={invalid} id={errorId} aria-live="polite">
    {#if error}<Icon name="problem" size={14} />{error}{/if}
  </p>
</div>

<style>
  .field {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    margin-bottom: var(--sp-1);
    font-weight: 600;
    color: var(--text);
  }
  .required {
    color: var(--danger);
  }
  .hint,
  .error {
    margin: 0;
    font-size: var(--fs-sm);
    line-height: var(--lh-sm);
  }
  .hint {
    margin-top: var(--sp-1);
    color: var(--muted);
  }
  .error {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    color: var(--danger);
  }
  .error.shown {
    margin-top: var(--sp-1);
  }
</style>
