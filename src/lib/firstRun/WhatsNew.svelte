<script lang="ts">
  // "What's new in Silo 1.0" (#250): shown once to an upgrading user with an organized library
  // (firstRun.ts decides), and again from Help → What's new.
  //   <WhatsNew bind:open onclose={() => setFirstRunFlag("whats_new_seen")} onshowshortcuts={...} />
  import Button from "../ui/Button.svelte";
  import Dialog from "../ui/Dialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import { WHATS_NEW } from "./whatsNew";

  interface Props {
    open?: boolean;
    /** Called once each time it closes (the app stores "seen" the first time). */
    onclose?: () => void;
    /** Open the Keyboard shortcuts dialog. */
    onshowshortcuts?: () => void;
  }

  let { open = $bindable(false), onclose, onshowshortcuts }: Props = $props();
</script>

<Dialog bind:open title="What's new in Silo 1.0" size="md" onclose={() => onclose?.()}>
  <ul class="list">
    {#each WHATS_NEW as item (item.title)}
      <li>
        <span class="icon" aria-hidden="true"><Icon name={item.icon} size={20} /></span>
        <div>
          <h3>{item.title}</h3>
          <p>{item.body}</p>
        </div>
      </li>
    {/each}
  </ul>
  {#snippet footer()}
    {#if onshowshortcuts}
      <Button
        variant="ghost"
        onclick={() => {
          open = false;
          onshowshortcuts?.();
        }}>See keyboard shortcuts</Button
      >
    {/if}
    <Button variant="primary" onclick={() => (open = false)}>Got it</Button>
  {/snippet}
</Dialog>

<style>
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  li {
    display: flex;
    gap: var(--sp-3);
    align-items: flex-start;
  }
  .icon {
    flex: 0 0 auto;
    display: inline-flex;
    padding: var(--sp-2);
    border-radius: var(--r-md);
    background: var(--accent-soft);
    color: var(--primary-text);
  }
  h3 {
    margin: 0 0 2px;
    font-size: var(--fs-md);
    font-weight: 600;
  }
  p {
    margin: 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
</style>
