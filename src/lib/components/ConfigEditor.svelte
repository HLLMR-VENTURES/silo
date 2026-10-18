<script lang="ts">
  import { getConfig, setConfig } from "../api";
  import type { CfgField, CfgPreset } from "../configSchemas";

  interface Props {
    title: string;
    path: string;
    fields: CfgField[];
    presets?: CfgPreset[];
    footnote?: string;
    onClose: () => void;
  }
  let { title, path, fields, presets = [], footnote, onClose }: Props = $props();

  let orig = $state<Record<string, string>>({});
  let vals = $state<Record<string, string>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);
  let saved = $state(false);

  async function load() {
    loading = true;
    try {
      const got = await getConfig(
        path,
        fields.map((f) => f.path),
      );
      orig = got;
      vals = { ...got };
    } catch (e) {
      error = String(e);
    }
    loading = false;
  }
  load();

  const dirty = $derived(fields.some((f) => (vals[f.path] ?? "") !== (orig[f.path] ?? "")));

  function applyPreset(p: CfgPreset) {
    vals = { ...vals, ...p.values };
  }

  async function save() {
    const edits = fields
      .filter((f) => (vals[f.path] ?? "") !== (orig[f.path] ?? ""))
      .map((f) => ({ path: f.path, value: vals[f.path] ?? "" }));
    if (edits.length === 0) return;
    try {
      await setConfig(path, edits);
      orig = { ...vals };
      saved = true;
      setTimeout(() => (saved = false), 1800);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="ms-panel">
  <div class="ms-head">
    <div>
      <div class="ms-title">{title}</div>
      <div class="ms-sub tnum">{path}</div>
    </div>
    <button class="ms-x" onclick={onClose} aria-label="Close">✕</button>
  </div>

  {#if loading}
    <div class="ms-empty">Loading…</div>
  {:else if error}
    <div class="ms-error">{error}</div>
  {:else}
    {#if presets.length > 0}
      <div class="ce-presets">
        <span class="ce-presets-label">Presets:</span>
        {#each presets as p (p.name)}
          <button class="ce-preset" onclick={() => applyPreset(p)}>{p.name}</button>
        {/each}
      </div>
    {/if}

    {#each fields as f (f.path)}
      <div class="ce-field">
        <div class="ce-flabel">
          {f.label}
          {#if f.hint}<span class="ce-hint">{f.hint}</span>{/if}
        </div>
        {#if f.kind === "bool"}
          <button
            class="switch"
            class:on={vals[f.path] === "true"}
            role="switch"
            aria-checked={vals[f.path] === "true"}
            aria-label={f.label}
            onclick={() => (vals[f.path] = vals[f.path] === "true" ? "false" : "true")}
          >
            <span class="knob"></span>
          </button>
        {:else if f.kind === "select"}
          <select class="ce-input" bind:value={vals[f.path]}>
            {#each f.options ?? [] as o (o)}
              <option value={o}>{f.optionLabels?.[o] ?? o}</option>
            {/each}
          </select>
        {:else if f.kind === "number"}
          <input class="ce-input tnum" type="number" bind:value={vals[f.path]} />
        {:else}
          <input class="ce-input" type="text" bind:value={vals[f.path]} />
        {/if}
      </div>
    {/each}

    <div class="ce-foot">
      <button class="ms-save" onclick={save} disabled={!dirty}>Save changes</button>
      {#if saved}<span class="ms-saved">saved ✓</span>{/if}
    </div>
    {#if footnote}<div class="ms-foot">{footnote} Silo keeps the original and the three versions before your latest edit as <code>.bak</code> files.</div>{/if}
  {/if}
</div>

<style>
  .ms-panel {
    position: fixed;
    z-index: 50;
    top: 8vh;
    left: 50%;
    transform: translateX(-50%);
    width: 520px;
    max-width: calc(100vw - 40px);
    max-height: 84vh;
    overflow-y: auto;
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    padding: 16px;
    scrollbar-width: thin;
  }
  .ms-head {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 10px;
  }
  .ms-title {
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .ms-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    word-break: break-all;
  }
  .ms-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    font-size: 0.9375rem;
  }
  .ms-x:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
  }
  .ce-presets {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 8px 0 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 8px;
  }
  .ce-presets-label {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .ce-preset {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 6px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .ce-preset:hover {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .ce-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 8px 0;
  }
  .ce-flabel {
    font-size: var(--fs-sm);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ce-hint {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .ce-input {
    flex: 0 0 190px;
    padding: 7px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
    font-family: inherit;
    font-size: var(--fs-sm);
  }
  .switch {
    flex: 0 0 auto;
    width: 40px;
    height: 23px;
    border-radius: 999px;
    border: 1px solid var(--border);
    background: var(--bg);
    position: relative;
  }
  .switch .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: var(--text-muted);
    transition: transform 0.15s ease, background 0.15s ease;
  }
  .switch.on {
    background: color-mix(in srgb, var(--primary) 30%, transparent);
    border-color: var(--primary);
  }
  .switch.on .knob {
    transform: translateX(17px);
    background: var(--primary);
  }
  .ce-foot {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 14px;
  }
  .ms-save {
    border: 1px solid var(--primary);
    background: var(--primary);
    color: var(--on-primary);
    padding: 8px 16px;
    border-radius: var(--radius-sm);
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .ms-save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ms-saved {
    color: var(--primary-text);
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .ms-empty,
  .ms-error {
    padding: 14px 0;
    font-size: var(--fs-sm);
  }
  .ms-error {
    color: var(--danger);
  }
  .ms-foot {
    margin-top: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
</style>
