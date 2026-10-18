<script lang="ts">
  // Something-failed state: <ErrorState title="Couldn't read the mods folder" body="..." {details} />.
  // With `details`, adds a "Copy details" button (copies; saves to a file if the clipboard fails).
  import type { Snippet } from "svelte";
  import { copyText, saveTextFile } from "../api";
  import { copyOrSave } from "../clipboard";
  import Button from "./Button.svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  interface Props {
    icon?: IconName;
    title: string;
    body?: string;
    /** Technical detail for bug reports; enables "Copy details". */
    details?: string;
    /** Heading level for the title, to fit the surrounding outline. */
    level?: 2 | 3 | 4;
    actions?: Snippet;
    class?: string;
  }

  let {
    icon = "problem",
    title,
    body,
    details,
    level = 2,
    actions,
    class: className = "",
  }: Props = $props();

  let copied = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  async function copyDetails() {
    if (!details) return;
    try {
      const out = await copyOrSave(details, "silo-error-details.txt", copyText, saveTextFile);
      if (out === "cancelled") return;
      copied = true;
      clearTimeout(timer);
      timer = setTimeout(() => (copied = false), 2000);
    } catch {
      // Saving failed too; nothing more to offer here.
    }
  }

  $effect(() => () => clearTimeout(timer));
</script>

<div class="state {className}">
  <span class="icon"><Icon name={icon} size={32} /></span>
  <svelte:element this={`h${level}`} class="title">{title}</svelte:element>
  {#if body}<p class="body">{body}</p>{/if}
  {#if actions || details}
    <div class="actions">
      {@render actions?.()}
      {#if details}
        <Button variant="secondary" icon={copied ? "check" : "copy"} onclick={copyDetails}>
          {copied ? "Copied" : "Copy details"}
        </Button>
        <span class="sr-only" role="status">{copied ? "Details copied" : ""}</span>
      {/if}
    </div>
  {/if}
</div>

<style>
  .state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--sp-2);
    max-width: 28rem;
    margin: 0 auto;
    padding: var(--sp-7) var(--sp-5);
    text-align: center;
  }
  .icon {
    display: inline-flex;
    margin-bottom: var(--sp-2);
    color: var(--danger);
  }
  .title {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-xl);
    line-height: var(--lh-xl);
    font-weight: 600;
    color: var(--text);
  }
  .body {
    margin: 0;
    color: var(--muted);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: center;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
</style>
