<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import type { CurationRow } from "../types";
  import { setCuration, setTags } from "../api";

  let {
    techName,
    curation,
    tags,
    onCurationChange,
    onTagsChange,
    onFilterTag,
  }: {
    techName: string;
    curation: CurationRow;
    tags: string[];
    onCurationChange: (row: CurationRow) => void;
    onTagsChange: (tags: string[]) => void;
    onFilterTag: (tag: string) => void;
  } = $props();

  let noteDraft = $state("");
  $effect(() => {
    noteDraft = curation.note ?? "";
  });

  let newTag = $state("");

  function persistCuration(patch: Partial<CurationRow>) {
    const next: CurationRow = { ...curation, ...patch };
    onCurationChange(next);
    setCuration(next).catch(() => {});
  }
  function setRating(r: number) {
    persistCuration({ rating: curation.rating === r ? 0 : r });
  }
  function saveNote() {
    if ((curation.note ?? "") !== noteDraft) persistCuration({ note: noteDraft });
  }
  function addTag() {
    const t = newTag.trim();
    if (!t || tags.includes(t)) {
      newTag = "";
      return;
    }
    const next = [...tags, t];
    newTag = "";
    onTagsChange(next);
    setTags(techName, next).catch(() => {});
  }
  function removeTag(t: string) {
    const next = tags.filter((x) => x !== t);
    onTagsChange(next);
    setTags(techName, next).catch(() => {});
  }
</script>

<div class="d-section">
  <div class="d-label">Rating</div>
  <div class="d-stars">
    {#each [1, 2, 3, 4, 5] as s (s)}
      <button class="star" class:on={curation.rating >= s} onclick={() => setRating(s)} aria-label={`${s} stars`}><Icon name="favorite" size={22} filled={curation.rating >= s} /></button>
    {/each}
  </div>
</div>

<div class="d-section">
  <div class="d-label">Tags</div>
  <div class="d-tags">
    {#each tags as t (t)}
      <span class="d-tag">
        <button class="d-tag-name" onclick={() => onFilterTag(t)} title="Filter by this tag">{t}</button>
        <button class="d-tag-x" onclick={() => removeTag(t)} title="Remove tag" aria-label="Remove tag"><Icon name="close" size={12} /></button>
      </span>
    {/each}
    <input
      class="d-tag-input"
      placeholder="add tag…"
      bind:value={newTag}
      onkeydown={(e) => e.key === "Enter" && addTag()}
      onblur={addTag}
    />
  </div>
</div>

<div class="d-section">
  <div class="d-label">Notes</div>
  <textarea class="d-note" bind:value={noteDraft} onblur={saveNote} placeholder="Personal notes about this mod…"></textarea>
</div>

<style>
  .d-section {
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .d-label {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
    margin-bottom: 8px;
  }
  .d-stars {
    display: flex;
    gap: 2px;
  }
  .star {
    border: none;
    background: transparent;
    color: var(--border);
    display: inline-grid;
    place-items: center;
    padding: 0 2px;
  }
  .star.on {
    color: var(--accent-text);
  }
  .d-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .d-tag {
    display: inline-flex;
    align-items: center;
    background: color-mix(in srgb, var(--info) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--info) 30%, var(--border));
    border-radius: 999px;
    overflow: hidden;
  }
  .d-tag-name {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 4px 4px 4px 10px;
  }
  .d-tag-x {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    padding: 4px 8px 4px 4px;
    opacity: 0.7;
  }
  .d-tag-x:hover {
    opacity: 1;
  }
  .d-tag-input {
    border: 1px dashed var(--border);
    background: transparent;
    color: var(--text);
    border-radius: 999px;
    padding: 5px 12px;
    font-size: var(--fs-xs);
    font-family: inherit;
    width: 100px;
  }
  .d-note {
    width: 100%;
    min-height: 70px;
    resize: vertical;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    padding: 8px 10px;
    font-family: inherit;
    font-size: var(--fs-xs);
  }
</style>
