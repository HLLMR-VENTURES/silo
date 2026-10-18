<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import {
    getModSettings,
    saveModSettings,
    saveModSettingsRaw,
  } from "../api";
  import type { SettingsFile, SettingsEdit } from "../types";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    modName: string;
    title: string;
    onClose: () => void;
  }
  let { modName, title, onClose }: Props = $props();

  let files = $state<SettingsFile[]>([]);
  let values = $state<Record<number, string>>({}); // fieldId -> current string value
  let rawMode = $state<Record<string, boolean>>({}); // path -> show raw editor
  let rawText = $state<Record<string, string>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);
  let savedPath = $state<string | null>(null);

  async function load() {
    loading = true;
    try {
      files = await getModSettings(modName);
      const v: Record<number, string> = {};
      for (const f of files) for (const fld of f.fields) v[fld.id] = fld.value;
      values = v;
      rawText = Object.fromEntries(files.map((f) => [f.path, f.raw]));
    } catch (e) {
      error = inlineError(e, "Reading the mod's settings");
    }
    loading = false;
  }
  load();

  async function saveForm(file: SettingsFile) {
    const edits: SettingsEdit[] = file.fields
      .filter((fld) => values[fld.id] !== fld.value)
      .map((fld) => ({ id: fld.id, value: values[fld.id] }));
    if (edits.length === 0) return;
    try {
      await saveModSettings(file.path, edits);
      flashSaved(file.path);
      await load();
    } catch (e) {
      error = inlineError(e, "Saving the mod's settings");
    }
  }

  async function saveRaw(file: SettingsFile) {
    try {
      await saveModSettingsRaw(file.path, rawText[file.path]);
      flashSaved(file.path);
      await load();
      rawMode[file.path] = false;
    } catch (e) {
      error = inlineError(e, "Saving the mod's settings");
    }
  }

  function flashSaved(path: string) {
    savedPath = path;
    setTimeout(() => (savedPath = null), 1800);
  }

  function dirty(file: SettingsFile): boolean {
    return file.fields.some((fld) => values[fld.id] !== fld.value);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="backdrop" onclick={onClose}></div>
<div class="ms-panel">
  <div class="ms-head">
    <div>
      <div class="ms-title">{title}</div>
      <div class="ms-sub">Mod settings</div>
    </div>
    <button class="ms-x" onclick={onClose} use:tooltip={"Close"} aria-label="Close"><Icon name="close" /></button>
  </div>

  {#if loading}
    <div class="ms-empty">Loading…</div>
  {:else if error}
    <div class="ms-error">{error}</div>
  {:else if files.length === 0}
    <div class="ms-empty">No settings files found for this mod yet.</div>
  {/if}

  {#each files as file (file.path)}
    <div class="ms-file">
      <div class="ms-file-head">
        <span class="ms-file-name tnum">{file.name}</span>
        <div class="ms-file-actions">
          {#if savedPath === file.path}<span class="ms-saved">saved <Icon name="check" size={13} /></span>{/if}
          <button
            class="ms-link"
            onclick={() => (rawMode[file.path] = !rawMode[file.path])}
          >
            {rawMode[file.path] ? "form" : "raw XML"}
          </button>
        </div>
      </div>

      {#if rawMode[file.path]}
        <textarea class="ms-raw" bind:value={rawText[file.path]} spellcheck="false"
        ></textarea>
        <button class="ms-save" onclick={() => saveRaw(file)}>Save raw XML</button>
      {:else if file.fields.length === 0}
        <div class="ms-empty small">
          Silo couldn't model this file as a form. Use “raw XML” to edit it directly.
        </div>
      {:else}
        {#each file.fields as fld (fld.id)}
          <div class="ms-field">
            <label class="ms-flabel" for={"f" + fld.id}>{fld.label}</label>
            {#if fld.kind === "bool"}
              <button
                class="switch"
                class:on={values[fld.id] === "true"}
                role="switch"
                aria-checked={values[fld.id] === "true"}
                aria-label={fld.label}
                onclick={() =>
                  (values[fld.id] = values[fld.id] === "true" ? "false" : "true")}
              >
                <span class="knob"></span>
              </button>
            {:else if fld.kind === "int" || fld.kind === "float"}
              <input
                id={"f" + fld.id}
                class="ms-input tnum"
                type="number"
                step={fld.kind === "float" ? "any" : "1"}
                bind:value={values[fld.id]}
              />
            {:else}
              <input
                id={"f" + fld.id}
                class="ms-input"
                type="text"
                bind:value={values[fld.id]}
              />
            {/if}
          </div>
        {/each}
        <button class="ms-save" onclick={() => saveForm(file)} disabled={!dirty(file)}>
          Save changes
        </button>
      {/if}
    </div>
  {/each}

  <div class="ms-foot">Silo keeps the original and the three versions before your latest edit as <code>.bak</code> files.</div>
</div>

<style>
  .ms-panel {
    position: fixed;
    z-index: 50;
    top: 8vh;
    left: 50%;
    transform: translateX(-50%);
    width: 560px;
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
    margin-bottom: 8px;
  }
  .ms-title {
    font-family: var(--font-display);
    font-size: var(--fs-lg);
    font-weight: 600;
  }
  .ms-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .ms-x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    width: 30px;
    height: 30px;
    border-radius: var(--radius-sm);
    display: inline-grid;
    place-items: center;
  }
  .ms-x:hover {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--text);
  }
  .ms-file {
    border-top: 1px solid var(--border);
    padding-top: 12px;
    margin-top: 12px;
  }
  .ms-file-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }
  .ms-file-name {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .ms-file-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .ms-saved {
    font-size: var(--fs-xs);
    color: var(--primary-text);
    font-weight: 600;
  }
  .ms-link {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .ms-field {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 7px 0;
  }
  .ms-flabel {
    font-size: var(--fs-sm);
    color: var(--text);
    min-width: 0;
    word-break: break-word;
  }
  .ms-input {
    flex: 0 0 200px;
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
  .ms-raw {
    width: 100%;
    min-height: 220px;
    resize: vertical;
    font-family: ui-monospace, "Cascadia Code", Consolas, monospace;
    font-size: var(--fs-xs);
    padding: 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg);
    color: var(--text);
  }
  .ms-save {
    margin-top: 10px;
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
  .ms-empty {
    color: var(--text-muted);
    padding: 16px 0;
    font-size: var(--fs-sm);
  }
  .ms-empty.small {
    padding: 4px 0 0;
    font-size: var(--fs-xs);
  }
  .ms-error {
    color: var(--danger);
    padding: 8px 0;
    font-size: var(--fs-sm);
  }
  .ms-foot {
    margin-top: 16px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
</style>
