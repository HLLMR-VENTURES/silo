<script lang="ts">
  // A settings file (game.xml, a savegame's careerSavegame.xml) as a short form, in a modal Dialog:
  // Esc or Close leaves without saving; focus is trapped while it's open and goes back after.
  import Button from "../ui/Button.svelte";
  import Dialog from "../ui/Dialog.svelte";
  import Icon from "../ui/Icon.svelte";
  import Switch from "../ui/Switch.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { getConfig, setConfig } from "../api";
  import { inFileCase, isNumberText, matchOptionCase, type CfgField, type CfgPreset } from "../configSchemas";

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
  /** The values as the file has them (its casing), for writing a choice the same way. */
  let fileVals: Record<string, string> = {};
  let vals = $state<Record<string, string>>({});
  let loading = $state(true);
  /** The file couldn't be read: there's no form to show. */
  let error = $state<string | null>(null);
  /** The last save failed (the game is running…): shown above the form, which stays. */
  let saveError = $state<string | null>(null);
  let saved = $state(false);
  let open = $state(true);
  const uid = $props.id();
  const fieldId = (i: number) => `${uid}-f${i}`;

  async function load() {
    loading = true;
    try {
      const got = await getConfig(
        path,
        fields.map((f) => f.path),
      );
      // The game may write a choice in another case ("Medium" for "medium"): show it as that
      // option. Unchanged, it isn't written back.
      fileVals = got;
      const shown = matchOptionCase(fields, got);
      orig = shown;
      vals = { ...shown };
    } catch (e) {
      error = inlineError(e, "Reading the settings file");
    }
    loading = false;
  }
  load();

  const changed = (f: CfgField) => (vals[f.path] ?? "") !== (orig[f.path] ?? "");
  const dirty = $derived(fields.some(changed));
  /** A changed number field that isn't a number (emptied): Save waits for a real value. */
  const badNumber = $derived(fields.some((f) => f.kind === "number" && changed(f) && !isNumberText(vals[f.path] ?? "")));

  function applyPreset(p: CfgPreset) {
    vals = { ...vals, ...p.values };
  }

  async function save() {
    const edits = fields
      .filter((f) => (vals[f.path] ?? "") !== (orig[f.path] ?? ""))
      .map((f) => ({
        path: f.path,
        value: f.kind === "select" ? inFileCase(vals[f.path] ?? "", fileVals[f.path]) : (vals[f.path] ?? ""),
      }));
    if (edits.length === 0 || badNumber) return;
    saveError = null;
    try {
      await setConfig(path, edits);
      orig = { ...vals };
      for (const e of edits) fileVals[e.path] = e.value;
      saved = true;
      setTimeout(() => (saved = false), 1800);
    } catch (e) {
      saveError = inlineError(e, "Saving the settings file");
    }
  }
</script>

<Dialog bind:open {title} description={path} size="md" onclose={() => onClose()} class="config-editor">
  {#if loading}
    <p class="ms-empty" role="status">Loading…</p>
  {:else if error}
    <p class="ms-error" role="alert">{error}</p>
  {:else}
    {#if saveError}<p class="ms-error" role="alert">{saveError}</p>{/if}
    {#if presets.length > 0}
      <div class="ce-presets" role="group" aria-label="Presets">
        <span class="ce-presets-label" aria-hidden="true">Presets:</span>
        {#each presets as p (p.name)}
          <Button size="sm" onclick={() => applyPreset(p)}>{p.name}</Button>
        {/each}
      </div>
    {/if}

    {#each fields as f, i (f.path)}
      <div class="ce-field">
        <label class="ce-flabel" for={fieldId(i)} id="{fieldId(i)}-label">
          {f.label}
          {#if f.hint}<span class="ce-hint">{f.hint}</span>{/if}
        </label>
        {#if f.kind === "bool"}
          <Switch
            id={fieldId(i)}
            aria-labelledby="{fieldId(i)}-label"
            checked={vals[f.path] === "true"}
            onchange={(on) => (vals[f.path] = on ? "true" : "false")}
          />
        {:else if f.kind === "select"}
          <select id={fieldId(i)} class="ce-input" bind:value={vals[f.path]}>
            {#each f.options ?? [] as o (o)}
              <option value={o}>{f.optionLabels?.[o] ?? o}</option>
            {/each}
          </select>
        {:else if f.kind === "number"}
          <!-- Not bind:value: on a number input that holds a JS number, and Rust takes strings. -->
          <input
            id={fieldId(i)}
            class="ce-input tnum"
            type="number"
            value={vals[f.path] ?? ""}
            oninput={(e) => (vals[f.path] = e.currentTarget.value)}
          />
        {:else}
          <input id={fieldId(i)} class="ce-input" type="text" bind:value={vals[f.path]} />
        {/if}
      </div>
    {/each}

    {#if footnote}<p class="ms-foot">{footnote} Silo keeps backup copies: the original and the three versions before your latest edit.</p>{/if}
  {/if}
  {#snippet footer()}
    <span class="ms-saved" role="status">{#if saved}Saved <Icon name="check" size={13} />{/if}</span>
    <Button onclick={() => (open = false)}>Close</Button>
    <Button variant="primary" onclick={save} disabled={loading || !!error || !dirty || badNumber}>Save changes</Button>
  {/snippet}
</Dialog>

<style>
  .ce-presets {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    padding: 0 0 12px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 8px;
  }
  .ce-presets-label {
    font-size: var(--fs-xs);
    color: var(--muted);
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
    color: var(--muted);
  }
  .ce-input {
    flex: 0 0 190px;
    min-height: 32px;
    padding: 6px 10px;
    border: 1px solid var(--border-strong);
    border-radius: var(--r-sm);
    background: var(--sunken);
    color: var(--text);
    font-family: inherit;
    font-size: var(--fs-sm);
  }
  .ms-saved {
    margin-right: auto;
    align-self: center;
    color: var(--primary-text);
    font-weight: 600;
    font-size: var(--fs-sm);
  }
  .ms-empty,
  .ms-error {
    margin: 0;
    padding: 14px 0;
    font-size: var(--fs-sm);
  }
  .ms-error {
    color: var(--danger);
  }
  .ms-foot {
    margin: 12px 0 0;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    font-size: var(--fs-xs);
    color: var(--muted);
  }
  :global(.config-editor .description) {
    font-size: var(--fs-xs);
    word-break: break-all;
  }
</style>
