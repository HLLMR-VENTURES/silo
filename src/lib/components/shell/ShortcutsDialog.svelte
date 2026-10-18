<script lang="ts">
  // The Keyboard shortcuts dialog (opened by ? anywhere, and from Help):
  //   <ShortcutsDialog bind:open={shortcutsOpen} />
  // Lists SHORTCUT_DOCS (lib/state/shortcuts): each row's key combos, then what they do.
  import Button from "../../ui/Button.svelte";
  import Dialog from "../../ui/Dialog.svelte";
  import Kbd from "../../ui/Kbd.svelte";
  import { SHORTCUT_DOCS } from "../../state/shortcuts";

  let { open = $bindable(false) }: { open?: boolean } = $props();
</script>

<Dialog bind:open title="Keyboard shortcuts" size="md">
  <dl class="list">
    {#each SHORTCUT_DOCS as doc (doc.label)}
      <div class="row">
        <dt class="keys">
          {#each doc.keys as combo, i (i)}
            {#if i > 0}<span class="or">or</span>{/if}
            <span class="combo">
              {#each combo as cap, j (j)}
                {#if j > 0}<span class="plus" aria-hidden="true">+</span>{/if}<Kbd>{cap}</Kbd>
              {/each}
            </span>
          {/each}
        </dt>
        <dd class="label">{doc.label}</dd>
      </div>
    {/each}
  </dl>
  <p class="note">Search, Rescan, Settings and F6 work while you type; the others wait until you leave the field.</p>
  {#snippet footer()}
    <Button variant="primary" onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Dialog>

<style>
  .list {
    display: flex;
    flex-direction: column;
    margin: 0;
  }
  .row {
    display: grid;
    grid-template-columns: minmax(10rem, auto) 1fr;
    align-items: center;
    gap: var(--sp-4);
    padding: var(--sp-2) 0;
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .keys {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--sp-1);
  }
  .combo {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .plus,
  .or {
    color: var(--muted);
    font-size: var(--fs-xs);
  }
  .label {
    margin: 0;
  }
  .note {
    margin: var(--sp-3) 0 0;
    color: var(--muted);
    font-size: var(--fs-sm);
  }
</style>
