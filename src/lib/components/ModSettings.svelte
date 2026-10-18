<script lang="ts">
  // A mod's settings files as forms (or raw XML), in a modal Dialog: Esc or Close leaves without
  // saving; focus is trapped while it's open and goes back after.
  import Button from "../ui/Button.svelte";
  import Dialog from "../ui/Dialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import Switch from "../ui/Switch.svelte";
  import { inlineError } from "../state/notify.svelte";
  import {
    getModSettings,
    saveModSettings,
    saveModSettingsRaw,
  } from "../api";
  import type { SettingsFile, SettingsEdit } from "../types";

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
  let open = $state(true);
  const uid = $props.id();

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

<Dialog bind:open {title} description="Mod settings" size="md" onclose={() => onClose()}>
  {#if loading}
    <p class="ms-empty" role="status">Loading…</p>
  {:else if error}
    <p class="ms-error" role="alert">{error}</p>
  {:else if files.length === 0}
    <p class="ms-empty">No settings files found for this mod yet.</p>
  {/if}

  {#each files as file (file.path)}
    <section class="ms-file" aria-label={file.name}>
      <div class="ms-file-head">
        <span class="ms-file-name tnum">{file.name}</span>
        <div class="ms-file-actions">
          <span class="ms-saved" role="status">{#if savedPath === file.path}Saved <Icon name="check" size={13} />{/if}</span>
          <Button
            variant="link"
            size="sm"
            aria-pressed={rawMode[file.path] ? "true" : "false"}
            onclick={() => (rawMode[file.path] = !rawMode[file.path])}
          >
            Raw XML
          </Button>
        </div>
      </div>

      {#if rawMode[file.path]}
        <textarea
          class="ms-raw"
          aria-label="Raw XML of {file.name}"
          bind:value={rawText[file.path]}
          spellcheck="false"
        ></textarea>
        <Button variant="primary" class="ms-save" onclick={() => saveRaw(file)}>Save raw XML</Button>
      {:else if file.fields.length === 0}
        <p class="ms-empty small">
          Silo couldn't model this file as a form. Use Raw XML to edit it directly.
        </p>
      {:else}
        {#each file.fields as fld (fld.id)}
          {@const fid = `${uid}-f${fld.id}`}
          <div class="ms-field">
            <label class="ms-flabel" for={fid} id="{fid}-label">{fld.label}</label>
            {#if fld.kind === "bool"}
              <Switch
                id={fid}
                aria-labelledby="{fid}-label"
                checked={values[fld.id] === "true"}
                onchange={(on) => (values[fld.id] = on ? "true" : "false")}
              />
            {:else if fld.kind === "int" || fld.kind === "float"}
              <input
                id={fid}
                class="ms-input tnum"
                type="number"
                step={fld.kind === "float" ? "any" : "1"}
                bind:value={values[fld.id]}
              />
            {:else}
              <input id={fid} class="ms-input" type="text" bind:value={values[fld.id]} />
            {/if}
          </div>
        {/each}
        <Button variant="primary" class="ms-save" onclick={() => saveForm(file)} disabled={!dirty(file)}>
          Save changes
        </Button>
      {/if}
    </section>
  {/each}

  <p class="ms-foot">Silo keeps the original and the three versions before your latest edit as <code>.bak</code> files.</p>
  {#snippet footer()}
    <Button onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Dialog>

<style>
  .ms-file {
    border-top: 1px solid var(--border);
    padding-top: 12px;
    margin-top: 12px;
  }
  .ms-file:first-child {
    border-top: none;
    margin-top: 0;
    padding-top: 0;
  }
  .ms-file-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
  }
  .ms-file-name {
    font-size: var(--fs-xs);
    color: var(--muted);
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
    min-height: 32px;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--sunken);
    color: var(--text);
    font-family: inherit;
    font-size: var(--fs-sm);
  }
  .ms-raw {
    width: 100%;
    min-height: 220px;
    resize: vertical;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    padding: 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--sunken);
    color: var(--text);
  }
  :global(.ms-save) {
    margin-top: 10px;
  }
  .ms-empty {
    margin: 0;
    color: var(--muted);
    padding: 16px 0;
    font-size: var(--fs-sm);
  }
  .ms-empty.small {
    padding: 4px 0 0;
    font-size: var(--fs-xs);
  }
  .ms-error {
    margin: 0;
    color: var(--danger);
    padding: 8px 0;
    font-size: var(--fs-sm);
  }
  .ms-foot {
    margin: 16px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
</style>
