<script lang="ts">
  // One Share dialog for a mod set (or the active mods): an unlisted link on your GitHub, a .silo
  // file, or a Multiplayer check file (.silomp). Links need GitHub; files need nothing.
  //   <ShareDialog bind:open name="Harvest 2026" mods={set.mods} library={mods} onConnect={...} />
  import Dialog from "../../ui/Dialog.svelte";
  import Button from "../../ui/Button.svelte";
  import RadioGroup from "../../ui/RadioGroup.svelte";
  import TextField from "../../ui/TextField.svelte";
  import { collectionExport, copyText, exportSetFile, ghStatus, mpExport, openExternal } from "../../api";
  import { inlineError } from "../../state/notify.svelte";
  import { start } from "../../state/ops.svelte";
  import { toast } from "../../ui";
  import type { ChoiceOption } from "../../ui/types";
  import type { CollectionExportResult, ModEntry, MpModRef } from "../../types";
  import { formatCount, plural } from "../../plural";

  type How = "link" | "file" | "multiplayer";

  interface Props {
    open?: boolean;
    /** The set's name ("My mods" for the active mods); the link and files are named after it. */
    name: string;
    /** The set's tech names. */
    mods: string[];
    /** The library, to find each mod's file (links and Multiplayer check files fingerprint them). */
    library: ModEntry[];
    /** Go to Settings → Accounts to connect GitHub. */
    onConnect: () => void;
    /** A link was created (so a list of shared links can refresh). */
    onShared?: () => void;
  }

  let { open = $bindable(false), name, mods, library, onConnect, onShared }: Props = $props();

  let how = $state<How>("link");
  let linkName = $state("");
  let canGist = $state<boolean | null>(null);
  let busy = $state(false);
  let error = $state("");
  let note = $state("");
  let result = $state<CollectionExportResult | null>(null);

  const OPTIONS: ChoiceOption<How>[] = [
    {
      value: "link",
      label: "Link",
      hint: "Friends with Silo open it and see what they have and what they need. Silo saves the list to your GitHub as an unlisted page; anyone with the link can see the list.",
    },
    { value: "file", label: "File", hint: "A .silo file you can send any way you like. No account needed." },
    {
      value: "multiplayer",
      label: "Multiplayer check file",
      hint: "A file with each mod's exact fingerprint, so friends can check they match your server before joining.",
    },
  ];

  /** The set's mods Silo has files for, and the ones it doesn't. */
  const refs = $derived.by(() => {
    const byTech = new Map(library.map((m) => [m.techName, m]));
    const have: MpModRef[] = [];
    const missing: string[] = [];
    for (const t of mods) {
      const m = byTech.get(t);
      if (m) have.push({ techName: m.techName, path: m.path, kind: m.kind, version: m.version });
      else missing.push(t);
    }
    return { have, missing };
  });
  const folders = $derived(refs.have.filter((r) => r.kind !== "zip").length);
  const count = (n: number) => `${formatCount(n)} ${plural(n, { one: "mod", other: "mods" })}`;

  // Each time it opens: start from the top, name the link after the set, check GitHub.
  $effect(() => {
    if (!open) return;
    how = "link";
    linkName = name;
    error = "";
    note = "";
    result = null;
    canGist = null;
    ghStatus()
      .then((s) => (canGist = s.canGist))
      .catch(() => (canGist = false));
  });

  async function createLink() {
    if (!linkName.trim() || busy) return;
    busy = true;
    error = "";
    const op = start("Creating a share link");
    try {
      result = await collectionExport(linkName.trim(), null, false, refs.have);
      onShared?.();
    } catch (e) {
      error = inlineError(e, "Creating the share link");
    } finally {
      busy = false;
      op.end();
    }
  }

  async function copyLink() {
    if (!result) return;
    try {
      await copyText(result.url);
      toast({ message: "Link copied." });
    } catch {
      error = "Silo couldn't use the clipboard. Select the link and copy it.";
    }
  }

  async function saveSetFile() {
    busy = true;
    error = "";
    try {
      if (await exportSetFile(name, mods)) note = `Saved ${count(mods.length)} to a .silo file.`;
    } catch (e) {
      error = inlineError(e, "Saving the .silo file");
    } finally {
      busy = false;
    }
  }

  async function saveCheckFile() {
    busy = true;
    error = "";
    const op = start("Fingerprinting your mods");
    try {
      const n = await mpExport(refs.have, name);
      if (n != null) note = `Saved a Multiplayer check file for ${count(n)}. Send it to the friends joining your server.`;
    } catch (e) {
      error = inlineError(e, "Saving the Multiplayer check file");
    } finally {
      busy = false;
      op.end();
    }
  }
</script>

<Dialog bind:open title={`Share ${name}`} size="md">
  <RadioGroup legend="How to share" hideLegend options={OPTIONS} bind:value={how} disabled={busy} />

  <div class="how" aria-live="polite">
    {#if how === "link"}
      {#if result}
        <p class="ok">Link created for {count(result.count)}.</p>
        <div class="link-row">
          <a
            class="link"
            href={result.url}
            onclick={(e) => {
              e.preventDefault();
              void openExternal(result!.url);
            }}>{result.url}</a
          >
          <Button size="sm" icon="copy" onclick={copyLink}>Copy link</Button>
        </div>
        {#if result.omitted.length > 0}
          <p class="note">
            {count(result.omitted.length)} in unpacked folders {result.omitted.length === 1 ? "was" : "were"} left
            out: Silo can only share mods packed as .zip files.
          </p>
        {/if}
      {:else if canGist === null}
        <p class="note">Checking your GitHub connection…</p>
      {:else if !canGist}
        <p class="note">
          Links are saved to your GitHub account. Connect GitHub and allow sharing links, then come back.
        </p>
        <Button
          variant="primary"
          onclick={() => {
            open = false;
            onConnect();
          }}>Connect GitHub</Button
        >
      {:else}
        <label class="name">
          <span>Name on the link</span>
          <TextField bind:value={linkName} maxlength={80} />
        </label>
        <Button
          variant="primary"
          icon="link"
          loading={busy}
          disabled={busy || !linkName.trim() || refs.have.length - folders === 0}
          onclick={createLink}>Create link</Button
        >
        {#if folders > 0}
          <p class="note">{count(folders)} in unpacked folders can't go on a link.</p>
        {/if}
      {/if}
    {:else if how === "file"}
      <Button variant="primary" icon="download" loading={busy} disabled={busy || mods.length === 0} onclick={saveSetFile}
        >Save file…</Button
      >
    {:else}
      <Button variant="primary" icon="download" loading={busy} disabled={busy || refs.have.length === 0} onclick={saveCheckFile}
        >Save file…</Button
      >
      {#if folders > 0}
        <p class="note">
          {count(folders)} in unpacked folders can't be fingerprinted, so friends will see {folders === 1 ? "it" : "them"} as not matching.
        </p>
      {/if}
    {/if}
    {#if how !== "file" && refs.missing.length > 0}
      <p class="note">
        {count(refs.missing.length)} in this set {refs.missing.length === 1 ? "isn't" : "aren't"} in your library, so
        {refs.missing.length === 1 ? "it's" : "they're"} left out.
      </p>
    {/if}
    {#if note}<p class="ok">{note}</p>{/if}
    {#if error}<p class="err" role="alert">{error}</p>{/if}
  </div>

  {#snippet footer()}
    <Button onclick={() => (open = false)}>Close</Button>
  {/snippet}
</Dialog>

<style>
  .how {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--sp-2);
    margin-top: var(--sp-4);
    padding-top: var(--sp-3);
    border-top: 1px solid var(--border);
  }
  .name {
    display: flex;
    flex-direction: column;
    gap: 4px;
    width: 100%;
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .link-row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    width: 100%;
  }
  .link {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-family: var(--font-mono);
    font-size: var(--fs-xs);
    color: var(--primary-text);
  }
  .note {
    margin: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .ok {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--primary-text);
  }
  .err {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--danger);
  }
</style>
