<script lang="ts">
  import { untrack } from "svelte";
  import Icon from "../ui/Icon.svelte";
  import type { CurationRow } from "../types";
  import { setCuration, setTags } from "../api";
  import { tooltip } from "../ui/tooltip";
  import { reportError } from "../state/notify.svelte";

  let {
    techName,
    curation,
    tags,
    onCurationChange,
    onCurationFailed,
    onTagsChange,
    onFilterTag,
  }: {
    techName: string;
    curation: CurationRow;
    tags: string[];
    onCurationChange: (row: CurationRow) => void;
    /** Saving `row` failed: put `prev` back (unless a newer edit replaced `row`) (#358). */
    onCurationFailed?: (row: CurationRow, prev: CurationRow) => void;
    onTagsChange: (tags: string[]) => void;
    onFilterTag: (tag: string) => void;
  } = $props();

  const uid = $props.id();
  let noteDraft = $state("");
  /** The row the note draft belongs to: the drawer can move to another mod with a draft unsaved. */
  let noteRow: CurationRow | null = null;
  /** A note whose save failed: it stays in the field (unsaved, so leaving the field tries again). */
  let unsavedNote: { techName: string; note: string } | null = null;
  $effect(() => {
    const c = curation;
    untrack(() => {
      if (noteRow && noteRow.techName !== c.techName) saveNote();
      noteRow = c;
      const kept = unsavedNote?.techName === c.techName ? unsavedNote.note : null;
      unsavedNote = null;
      noteDraft = kept ?? c.note ?? "";
    });
  });
  // Esc or a view switch removes the field, which doesn't reliably blur it first: keep the draft.
  $effect(() => () => saveNote());

  let newTag = $state("");

  function persistCuration(patch: Partial<CurationRow>) {
    const prev = curation;
    const next: CurationRow = { ...prev, ...patch };
    onCurationChange(next);
    setCuration(next).catch((e) => {
      onCurationFailed?.(next, prev);
      reportError(e, "Saving your mod flags");
    });
  }
  function setRating(r: number) {
    persistCuration({ rating: curation.rating === r ? 0 : r });
  }
  /** Bumped per note sent: only the newest one's failure keeps its text in the field. */
  let noteSeq = 0;
  function saveNote() {
    const row = noteRow;
    if (!row || (row.note ?? "") === noteDraft) return;
    const next: CurationRow = { ...row, note: noteDraft };
    const my = ++noteSeq;
    noteRow = next;
    onCurationChange(next);
    setCuration(next).catch((e) => {
      // The row goes back to the saved note; the typed one stays in the field while this mod is
      // open, so leaving the field tries again.
      if (my === noteSeq && noteRow?.techName === next.techName)
        unsavedNote = { techName: next.techName, note: next.note ?? "" };
      onCurationFailed?.(next, row);
      reportError(e, "Saving your mod flags");
    });
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
    setTags(techName, next).catch((e) => reportError(e, "Saving your tags"));
  }
  function removeTag(t: string) {
    const next = tags.filter((x) => x !== t);
    onTagsChange(next);
    setTags(techName, next).catch((e) => reportError(e, "Saving your tags"));
  }
</script>

<div class="d-section">
  <h4 class="d-label" id="{uid}-rating">Rating</h4>
  <div class="d-stars" role="group" aria-labelledby="{uid}-rating">
    {#each [1, 2, 3, 4, 5] as s (s)}
      <button class="star" class:on={curation.rating >= s} onclick={() => setRating(s)} aria-label={`${s} ${s === 1 ? "star" : "stars"}`} aria-pressed={curation.rating === s}><Icon name="favorite" size={22} filled={curation.rating >= s} /></button>
    {/each}
  </div>
</div>

<div class="d-section">
  <h4 class="d-label">Tags</h4>
  <div class="d-tags">
    {#each tags as t (t)}
      <span class="d-tag">
        <button class="d-tag-name" onclick={() => onFilterTag(t)} use:tooltip={"Show mods with this tag"}>{t}</button>
        <button class="d-tag-x" onclick={() => removeTag(t)} use:tooltip={"Remove tag"} aria-label="Remove tag: {t}"><Icon name="close" size={12} /></button>
      </span>
    {/each}
    <input
      class="d-tag-input"
      aria-label="Add a tag"
      placeholder="Add a tag…"
      bind:value={newTag}
      onkeydown={(e) => e.key === "Enter" && addTag()}
      onblur={addTag}
    />
  </div>
</div>

<div class="d-section">
  <h4 class="d-label"><label for="{uid}-notes">Notes</label></h4>
  <textarea id="{uid}-notes" class="d-note" bind:value={noteDraft} onblur={saveNote} placeholder="Personal notes about this mod…"></textarea>
</div>

<style>
  .d-section {
    padding: 6px 0;
  }
  .d-label {
    margin: 0 0 6px;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text);
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
    min-height: 24px;
    padding: 4px 4px 4px 10px;
  }
  .d-tag-x {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    min-width: 24px;
    min-height: 24px;
    padding: 4px 8px 4px 4px;
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
