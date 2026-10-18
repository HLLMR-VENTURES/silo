<script lang="ts">
  // One-field text prompt. Usually reached through `askText()`; standalone:
  //   <PromptDialog bind:open title="Rename" label="Name" value={old} confirm="Rename" onresult={...} />
  // Enter submits. `validate` gets the trimmed value: a string / {error} shows inline and blocks;
  // {warn, confirm} shows a warning and relabels the button, and a second submit of the same value accepts.
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import Field from "./Field.svelte";
  import Icon from "./Icon.svelte";
  import TextField from "./TextField.svelte";
  import type { PromptValidation } from "./dialogs.svelte";

  interface Props {
    open?: boolean;
    title: string;
    label: string;
    value?: string;
    placeholder?: string;
    confirm: string;
    cancel?: string;
    validate?: (value: string) => PromptValidation;
    /** The trimmed value, or null when cancelled / dismissed. */
    onresult?: (value: string | null) => void;
  }

  let {
    open = $bindable(false),
    title,
    label,
    value = "",
    placeholder,
    confirm,
    cancel = "Cancel",
    validate,
    onresult,
  }: Props = $props();

  const uid = $props.id();
  const warnId = `${uid}-warn`;

  // Seeded once from the prop; the dialog owns the text while it's open.
  // svelte-ignore state_referenced_locally
  let text = $state(value);
  let error = $state<string | undefined>();
  let warning = $state<{ message: string; confirm: string; forValue: string } | null>(null);
  let answered = false;

  const confirmLabel = $derived(warning ? warning.confirm : confirm);

  function oninput(e: Event) {
    error = undefined;
    const now = (e.currentTarget as HTMLInputElement).value.trim();
    if (warning && now !== warning.forValue) warning = null;
  }

  function submit() {
    const v = text.trim();
    const verdict = validate?.(v) ?? null;
    if (typeof verdict === "string") {
      error = verdict;
      return;
    }
    if (verdict && "error" in verdict) {
      error = verdict.error;
      return;
    }
    if (verdict && "warn" in verdict && warning?.forValue !== v) {
      warning = { message: verdict.warn, confirm: verdict.confirm, forValue: v };
      return;
    }
    answered = true;
    open = false;
    onresult?.(v);
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && !e.isComposing) {
      e.preventDefault();
      submit();
    }
  }

  function cancelPrompt() {
    answered = true;
    open = false;
    onresult?.(null);
  }

  function onclose() {
    if (!answered) onresult?.(null);
    answered = false;
  }
</script>

<Dialog bind:open {title} size="sm" {onclose}>
  <Field {label} {error}>
    {#snippet control({ id, describedBy, invalid })}
      <TextField
        {id}
        aria-describedby={[describedBy, warning ? warnId : ""].filter(Boolean).join(" ") || undefined}
        {invalid}
        {placeholder}
        data-autofocus=""
        bind:value={text}
        {oninput}
        {onkeydown}
      />
    {/snippet}
  </Field>
  {#if warning}
    <p class="warn" id={warnId} role="status">
      <Icon name="problem" size={14} />{warning.message}
    </p>
  {/if}
  {#snippet footer()}
    <Button onclick={cancelPrompt}>{cancel}</Button>
    <Button variant={warning ? "danger" : "primary"} onclick={submit}>{confirmLabel}</Button>
  {/snippet}
</Dialog>

<style>
  .warn {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    margin: var(--sp-2) 0 0;
    font-size: var(--fs-sm);
    color: var(--warn-text);
  }
</style>
