<script lang="ts">
  // One group of settings on a Settings tab: a titled card, styled like the Tools panels.
  //   <SettingsCard title="Mods folder" description="Where FS25 loads mods from.">
  //     {#snippet tools()}<Button size="sm">Change…</Button>{/snippet}
  //     <p class="path">…</p>
  //     {#snippet actions()}<Button>Use the game's folder</Button>{/snippet}
  //   </SettingsCard>
  // `tools` sit on the right of the title (Open folder, Change…); `actions` along the bottom.
  // Inside the body, these classes are styled for every tab: `.path` (a folder path in a mono,
  // read-only field; `.muted` when it's missing or there's none), `.hint` (secondary copy), `.reason` (why a control is off, under it), `.row`
  // (a `.label` on the left, its control on the right) and `.btnrow` (a wrapping row of buttons).
  // `id` makes the card a focus target (tabindex -1) for deep links like go("settings", "accounts").
  import type { Snippet } from "svelte";

  interface Props {
    title: string;
    description?: string;
    /** Element id; also makes the card focusable from script (tabindex -1). */
    id?: string;
    /** Small buttons on the right of the title. */
    tools?: Snippet;
    children?: Snippet;
    /** Buttons at the bottom of the card. */
    actions?: Snippet;
  }

  let { title, description, id, tools, children, actions }: Props = $props();

  const uid = $props.id();
  const titleId = `${uid}-title`;
</script>

<!-- tabindex is only ever -1 (focusable from script for a deep link, never in the Tab order). -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
<section class="settings-card" {id} tabindex={id ? -1 : undefined} aria-labelledby={titleId}>
  <div class="card-head">
    <div class="card-titles">
      <h3 class="card-title" id={titleId}>{title}</h3>
      {#if description}<p class="card-desc">{description}</p>{/if}
    </div>
    {#if tools}<div class="card-tools">{@render tools()}</div>{/if}
  </div>
  {#if children}<div class="card-body">{@render children()}</div>{/if}
  {#if actions}<div class="card-actions">{@render actions()}</div>{/if}
</section>

<style>
  .settings-card {
    background: var(--raised);
    border: 1px solid var(--border);
    border-radius: var(--r-md);
    padding: var(--sp-4);
  }
  .settings-card:focus {
    outline: none;
  }
  .settings-card:focus-visible {
    outline: 2px solid var(--focus);
    outline-offset: 2px;
  }
  .card-head {
    display: flex;
    align-items: flex-start;
    gap: var(--sp-3);
  }
  .card-titles {
    flex: 1;
    min-width: 0;
  }
  .card-tools {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
  }
  .card-title {
    margin: 0;
    font-size: var(--fs-lg);
    line-height: 1.3;
  }
  .card-desc {
    margin: var(--sp-1) 0 0;
    color: var(--muted);
    max-width: 65ch;
  }
  .card-body {
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
  .card-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
  .card-body :global(p) {
    margin: 0;
  }
  .card-body :global(.path) {
    font-family: var(--font-mono);
    font-size: var(--fs-sm);
    color: var(--text);
    background: var(--sunken);
    border: 1px solid var(--border);
    border-radius: var(--r-sm);
    padding: var(--sp-2) var(--sp-3);
    word-break: break-all;
  }
  .card-body :global(.path.muted) {
    font-style: italic;
    color: var(--muted);
  }
  .settings-card :global(.hint) {
    font-size: var(--fs-sm);
    color: var(--muted);
    line-height: 1.5;
    max-width: 65ch;
  }
  .settings-card :global(.reason) {
    display: flex;
    align-items: center;
    gap: var(--sp-1);
    flex-basis: 100%;
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--muted);
  }
  .card-body :global(.label) {
    font-weight: 600;
    color: var(--text);
  }
  .card-body :global(.row) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--sp-4);
  }
  .card-body :global(.btnrow) {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-2);
  }
</style>
