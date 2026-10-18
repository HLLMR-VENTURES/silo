<script lang="ts">
  // Nothing-here state: <EmptyState icon="library" title="No mods yet" body="...">{#snippet actions()}...{/snippet}</EmptyState>.
  // Keep `body` to one sentence that says what to do next.
  import type { Snippet } from "svelte";
  import Icon from "./Icon.svelte";
  import type { IconName } from "./icons";

  interface Props {
    icon?: IconName;
    title: string;
    body?: string;
    /** Heading level for the title, to fit the surrounding outline. */
    level?: 2 | 3 | 4;
    actions?: Snippet;
    class?: string;
  }

  let { icon = "info", title, body, level = 2, actions, class: className = "" }: Props = $props();
</script>

<div class="state {className}">
  <span class="icon"><Icon name={icon} size={32} /></span>
  <svelte:element this={`h${level}`} class="title">{title}</svelte:element>
  {#if body}<p class="body">{body}</p>{/if}
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
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
    color: var(--muted);
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
