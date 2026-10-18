<script lang="ts">
  // Yes/no dialog with a verb button. Usually reached through `ask()`; standalone:
  //   <ConfirmDialog bind:open title="Delete?" confirm="Delete" tone="danger" onresult={...} />
  // Layout: body, optional facts table, optional scrolling list, footer [Cancel] [Verb].
  // The body is the dialog's accessible description; a danger confirm is an alertdialog.
  // The verb button only answers ARM_MS after opening (see dialogs.svelte.ts), and never to a
  // held-down (repeating) Enter or Space.
  import type { Snippet } from "svelte";
  import Button from "./Button.svelte";
  import Dialog from "./Dialog.svelte";
  import { armTimer, type ConfirmFact } from "./dialogs.svelte";

  interface Props {
    open?: boolean;
    title: string;
    body?: string | Snippet;
    facts?: ConfirmFact[];
    list?: string[];
    confirm: string;
    cancel?: string;
    tone?: "neutral" | "danger";
    /** true = confirm button; false = Cancel, Esc, scrim, or closed from outside. */
    onresult?: (ok: boolean) => void;
  }

  let {
    open = $bindable(false),
    title,
    body,
    facts,
    list,
    confirm,
    cancel = "Cancel",
    tone = "neutral",
    onresult,
  }: Props = $props();

  const uid = $props.id();
  const bodyId = `${uid}-body`;
  const danger = $derived(tone === "danger");
  const arm = armTimer();

  $effect(() => {
    if (!open) return;
    arm.rearm();
    return () => arm.stop();
  });
  /** Set by the buttons so the Dialog's follow-up "programmatic" close isn't reported twice. */
  let answered = false;

  function answer(ok: boolean) {
    answered = true;
    open = false;
    onresult?.(ok);
  }

  function onclose() {
    if (!answered) onresult?.(false);
    answered = false;
  }

  function onVerbClick() {
    if (arm.armed()) answer(true);
  }

  /** Enter/Space before the button is armed, or auto-repeating from a held key, does nothing. */
  function onVerbKeydown(e: KeyboardEvent) {
    if ((e.key === "Enter" || e.key === " ") && (e.repeat || !arm.armed())) e.preventDefault();
  }
</script>

<Dialog
  bind:open
  {title}
  size="sm"
  role={danger ? "alertdialog" : "dialog"}
  describedby={body ? bodyId : undefined}
  {onclose}
>
  {#if body}
    <div class="body-text" id={bodyId}>
      {#if typeof body === "string"}<p>{body}</p>{:else}{@render body()}{/if}
    </div>
  {/if}
  {#if facts?.length}
    <dl class="facts">
      {#each facts as fact, i (i)}
        <dt>{fact.label}</dt>
        <dd>{fact.value}</dd>
      {/each}
    </dl>
  {/if}
  {#if list?.length}
    <!-- A long list scrolls, so it must be reachable by keyboard to scroll it. -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <ul
      class="list"
      tabindex={list.length > 8 ? 0 : undefined}
      aria-label={list.length > 8 ? `${list.length} items` : undefined}
    >
      {#each list as item, i (i)}<li>{item}</li>{/each}
    </ul>
  {/if}
  {#snippet footer()}
    <Button data-autofocus={danger ? "" : undefined} onclick={() => answer(false)}>{cancel}</Button>
    <Button
      variant={danger ? "danger" : "primary"}
      data-autofocus={danger ? undefined : ""}
      onclick={onVerbClick}
      onkeydown={onVerbKeydown}
    >
      {confirm}
    </Button>
  {/snippet}
</Dialog>

<style>
  .body-text :global(p) {
    margin: 0 0 var(--sp-2);
  }
  .body-text :global(p:last-child) {
    margin-bottom: 0;
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--sp-1) var(--sp-4);
    margin: var(--sp-3) 0 0;
    font-size: var(--fs-sm);
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .list {
    max-height: calc(8 * 1.75rem);
    margin: var(--sp-3) 0 0;
    padding: var(--sp-1) var(--sp-3);
    overflow: auto;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    background: var(--sunken);
    font-size: var(--fs-sm);
  }
  .list li {
    padding: var(--sp-1) 0;
    line-height: 1.25rem;
    overflow-wrap: anywhere;
  }
</style>
