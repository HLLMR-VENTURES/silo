<script lang="ts">
  // Category override: the discoverable way to fix a miscategorized mod. The ⋯ menu's
  // "Change category…" scrolls here and focuses the picker (focusPicker()).
  import Button from "../../ui/Button.svelte";
  import type { ModEntry } from "../../types";

  interface Props {
    mod: ModEntry;
    categories: string[];
    isOverridden: boolean;
    onSetCategory: (category: string, subcategory: string | null) => void;
    onResetCategory: () => void;
  }

  let { mod, categories, isOverridden, onSetCategory, onResetCategory }: Props = $props();
  const uid = $props.id();

  // Drafts follow the prop (and re-sync when the drawer switches mods).
  let catDraft = $state("");
  let subDraft = $state("");
  $effect(() => {
    catDraft = mod.category;
    subDraft = mod.subcategory ?? "";
  });
  const dirty = $derived(catDraft !== mod.category || (subDraft.trim() || null) !== (mod.subcategory ?? null));

  let section = $state<HTMLElement | null>(null);
  let picker = $state<HTMLSelectElement | null>(null);

  /** Bring the editor into view and focus the category picker. */
  export function focusPicker(): void {
    section?.scrollIntoView?.({ block: "nearest" });
    picker?.focus();
  }
</script>

<section class="sec" aria-labelledby="{uid}-category" bind:this={section}>
  <h3 id="{uid}-category" class="h">
    Category {#if isOverridden}<span class="ovr">Set by you</span>{/if}
  </h3>
  <div class="row">
    <select class="field sel" aria-label="Category" bind:this={picker} bind:value={catDraft}>
      {#each categories as c (c)}<option value={c}>{c}</option>{/each}
    </select>
    <input class="field sub" aria-label="Subcategory" placeholder="Subcategory (optional)" bind:value={subDraft} />
  </div>
  <div class="row acts">
    <Button size="sm" variant="primary" disabled={!dirty} onclick={() => onSetCategory(catDraft, subDraft.trim() || null)}>
      Save category
    </Button>
    {#if isOverridden}
      <Button size="sm" icon="undo" onclick={onResetCategory}>Reset to automatic</Button>
    {/if}
  </div>
</section>

<style>
  .sec {
    padding: var(--sp-3) 0;
    border-top: 1px solid var(--border);
  }
  .h {
    margin: 0 0 var(--sp-2);
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
  }
  .ovr {
    font-size: var(--fs-xs);
    text-transform: none;
    letter-spacing: 0;
    color: var(--accent-text);
    background: var(--accent-soft);
    padding: 1px 6px;
    border-radius: 999px;
    margin-left: 6px;
  }
  .row {
    display: flex;
    gap: var(--sp-2);
  }
  .acts {
    margin-top: var(--sp-2);
  }
  .field {
    padding: 7px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }
  .sel {
    flex: 0 0 auto;
  }
  .sub {
    flex: 1 1 auto;
    min-width: 0;
  }
</style>
