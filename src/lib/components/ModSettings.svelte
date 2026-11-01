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
  import { isNumberText } from "../configSchemas";
  import { afterSave, draftsOf, fieldKey, snapshotOf } from "../modSettingsDraft";

  interface Props {
    modName: string;
    title: string;
    onClose: () => void;
  }
  let { modName, title, onClose }: Props = $props();

  let files = $state<SettingsFile[]>([]);
  let values = $state<Record<string, string>>({}); // fieldKey(path, id) -> current string value
  let rawMode = $state<Record<string, boolean>>({}); // path -> show raw editor
  let rawText = $state<Record<string, string>>({});
  let loading = $state(true);
  let error = $state<string | null>(null);
  let savedPath = $state<string | null>(null);
  /** Files whose save is out: their Save stays off, so a double-click can't write twice. */
  let savingPaths = $state<Record<string, boolean>>({});
  // Field ids restart at 0 in each file: values are keyed by file too, and so are element ids
  // (the file's place in the list), so two files' field 0 stay apart.
  const vkey = (file: SettingsFile, id: number) => fieldKey(file.path, id);
  let open = $state(true);
  const uid = $props.id();

  async function load() {
    loading = true;
    error = null;
    try {
      ({ files, values, rawText } = draftsOf(await getModSettings(modName)));
    } catch (e) {
      error = inlineError(e, "Reading the mod's settings");
    }
    loading = false;
  }
  load();

  async function saveForm(file: SettingsFile) {
    if (savingPaths[file.path]) return;
    const edits: SettingsEdit[] = file.fields
      .filter((fld) => values[vkey(file, fld.id)] !== fld.value)
      .map((fld) => ({ id: fld.id, value: values[vkey(file, fld.id)] }));
    if (edits.length === 0 || badNumber(file)) return;
    await saveFile(file, () => saveModSettings(file.path, edits));
  }

  async function saveRaw(file: SettingsFile) {
    if (savingPaths[file.path]) return;
    const text = rawText[file.path];
    if (await saveFile(file, () => saveModSettingsRaw(file.path, text))) rawMode[file.path] = false;
  }

  /**
   * Write one file, then read it again: only that file's drafts take the new reading, so unsaved
   * edits in the mod's other files (and edits made here while the write was out) stay (#358).
   * @returns true when it was written
   */
  async function saveFile(file: SettingsFile, write: () => Promise<void>): Promise<boolean> {
    const path = file.path;
    const sent = snapshotOf({ files, values, rawText }, path);
    savingPaths[path] = true;
    error = null;
    try {
      await write();
    } catch (e) {
      error = inlineError(e, "Saving the mod's settings");
      return false;
    } finally {
      savingPaths[path] = false;
    }
    flashSaved(path);
    try {
      ({ files, values, rawText } = afterSave({ files, values, rawText }, await getModSettings(modName), path, sent));
    } catch (e) {
      error = inlineError(e, "Reading the mod's settings");
    }
    return true;
  }

  function flashSaved(path: string) {
    savedPath = path;
    setTimeout(() => (savedPath = null), 1800);
  }

  function dirty(file: SettingsFile): boolean {
    return file.fields.some((fld) => values[vkey(file, fld.id)] !== fld.value);
  }

  /** A changed number field that isn't a number (emptied, or a fraction in a whole-number one). */
  function badNumber(file: SettingsFile): boolean {
    return file.fields.some(
      (fld) =>
        (fld.kind === "int" || fld.kind === "float") &&
        values[vkey(file, fld.id)] !== fld.value &&
        !isNumberText(values[vkey(file, fld.id)] ?? "", fld.kind === "int"),
    );
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

  {#each files as file, fi (file.path)}
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
        <Button
          variant="primary"
          class="ms-save"
          loading={!!savingPaths[file.path]}
          disabled={!!savingPaths[file.path]}
          onclick={() => saveRaw(file)}>Save raw XML</Button
        >
      {:else if file.fields.length === 0}
        <p class="ms-empty small">
          Silo can't show this file as a form. Use Raw XML to edit it directly.
        </p>
      {:else}
        {#each file.fields as fld (fld.id)}
          {@const fid = `${uid}-f${fi}-${fld.id}`}
          <div class="ms-field">
            <label class="ms-flabel" for={fid} id="{fid}-label">{fld.label}</label>
            {#if fld.kind === "bool"}
              <Switch
                id={fid}
                aria-labelledby="{fid}-label"
                checked={values[vkey(file, fld.id)] === "true"}
                onchange={(on) => (values[vkey(file, fld.id)] = on ? "true" : "false")}
              />
            {:else if fld.kind === "int" || fld.kind === "float"}
              <input
                id={fid}
                class="ms-input tnum"
                type="number"
                step={fld.kind === "float" ? "any" : "1"}
                value={values[vkey(file, fld.id)] ?? ""}
                oninput={(e) => (values[vkey(file, fld.id)] = e.currentTarget.value)}
              />
            {:else}
              <input id={fid} class="ms-input" type="text" bind:value={values[vkey(file, fld.id)]} />
            {/if}
          </div>
        {/each}
        <Button
          variant="primary"
          class="ms-save"
          onclick={() => saveForm(file)}
          loading={!!savingPaths[file.path]}
          disabled={!!savingPaths[file.path] || !dirty(file) || badNumber(file)}
        >
          Save changes
        </Button>
      {/if}
    </section>
  {/each}

  <p class="ms-foot">Silo keeps backup copies: the original and the three versions before your latest edit.</p>
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
  .ms-file :global(.ms-save) {
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
