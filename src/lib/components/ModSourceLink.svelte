<script lang="ts">
  // "Where it came from" (#196): the page the user downloaded this mod from. Silo only keeps
  // the address. It never requests the page; Open page hands it to the user's browser.
  import type { ModEntry, SourceRow } from "../types";
  import { setModSource, getModSources, markSourceChecked, openExternal } from "../api";
  import { sourceHost, checkedDate } from "../sources";

  let {
    mod,
    source,
    onSourceChange,
    onRepoChange,
  }: {
    mod: ModEntry;
    source: SourceRow | null;
    onSourceChange: (row: SourceRow | null) => void;
    onRepoChange: (r: { owner: string; repo: string } | null) => void;
  } = $props();

  let editing = $state(false);
  let draft = $state("");
  let err = $state<string | null>(null);
  let note = $state<string | null>(null);
  let saving = $state(false);

  // A different mod (or a fresh row) resets the editor.
  $effect(() => {
    void mod.techName;
    editing = false;
    draft = "";
    err = null;
    note = null;
  });

  const showInput = $derived(!source || editing);

  async function save() {
    const techName = mod.techName;
    saving = true;
    err = null;
    note = null;
    try {
      const linked = await setModSource(techName, draft.trim());
      // Read back what was stored: the cleaned address, and its date (kept for the same page,
      // cleared for a new one).
      const rows = await getModSources();
      onSourceChange(rows.find((r) => r.techName === techName) ?? null);
      if (linked) {
        onRepoChange({ owner: linked.owner, repo: linked.repo });
        note = `Also linked github.com/${linked.owner}/${linked.repo} for GitHub update checks.`;
      }
      editing = false;
      draft = "";
    } catch (e) {
      err = String(e);
    }
    saving = false;
  }

  async function remove() {
    err = null;
    note = null;
    try {
      await setModSource(mod.techName, null);
      onSourceChange(null);
    } catch (e) {
      err = String(e);
    }
  }

  async function markChecked() {
    if (!source) return;
    err = null;
    try {
      const at = await markSourceChecked(source.techName);
      onSourceChange({ ...source, lastChecked: at });
    } catch (e) {
      err = String(e);
    }
  }

  function startChange() {
    draft = source?.url ?? "";
    editing = true;
    err = null;
    note = null;
  }
</script>

<div class="d-section">
  <div class="d-label">Where it came from</div>
  {#if showInput}
    <div class="src-link">
      <input
        class="src-input"
        placeholder="Paste the page you downloaded this mod from"
        bind:value={draft}
        onkeydown={(e) => e.key === "Enter" && draft.trim() && save()}
      />
      <button class="src-btn" onclick={save} disabled={saving || !draft.trim()}>Save</button>
      {#if source}
        <button class="src-btn" onclick={() => ((editing = false), (err = null))}>Cancel</button>
      {/if}
    </div>
    <div class="src-help">Silo remembers it so you can check for updates later.</div>
  {:else if source}
    <div class="src-row">
      <button class="src-host" title={source.url} onclick={() => openExternal(source.url)}>
        {sourceHost(source.url)}
      </button>
      <button class="src-open" onclick={() => openExternal(source.url)}>Open page ↗</button>
      <button class="src-btn" onclick={startChange}>Change</button>
      <button class="src-btn" onclick={remove}>Remove</button>
    </div>
    <div class="src-row">
      <span class="src-when">
        {source.lastChecked ? `You last checked it on ${checkedDate(source.lastChecked)}` : "Not checked yet"}
      </span>
      <button class="src-btn" onclick={markChecked}>I checked it</button>
    </div>
  {/if}
  {#if err}<div class="src-err">{err}</div>{/if}
  {#if note}<div class="src-help src-note">{note}</div>{/if}
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
  .src-link {
    display: flex;
    gap: 6px;
  }
  .src-input {
    flex: 1 1 auto;
    min-width: 0;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    border-radius: var(--radius-sm);
    padding: 7px 10px;
    font-size: var(--fs-xs);
    font-family: inherit;
  }
  .src-btn {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 7px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .src-btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .src-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .src-row {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .src-row + .src-row {
    margin-top: 8px;
  }
  .src-host {
    border: none;
    background: transparent;
    color: var(--text);
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 0;
    margin-right: 4px;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .src-host:hover {
    color: var(--info);
    text-decoration: underline;
  }
  .src-open {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .src-open:hover {
    text-decoration: underline;
  }
  .src-when {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    margin-right: 4px;
  }
  .src-help {
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .src-note {
    color: var(--info);
  }
  .src-err {
    margin-top: 8px;
    color: var(--danger);
    font-size: var(--fs-xs);
  }
</style>
