<script lang="ts" module>
  // The last deep link handled, kept across remounts (the tab unmounts when you leave it), so
  // coming back doesn't preview the same link again.
  let handledRequest = 0;
</script>

<script lang="ts">
  // Mod sets → Open a shared set: paste a link, or choose a .silo / .silomp file. A link or .silo
  // file shows a preview (what you have, what Silo can download, what to get yourself); nothing is
  // downloaded or saved until you click. A Multiplayer check file (.silomp) is checked against
  // your active mods only when you click Check my mods. A silo://collection link lands here.
  import Button from "../../ui/Button.svelte";
  import TextField from "../../ui/TextField.svelte";
  import Icon from "../../ui/Icon.svelte";
  import { toast } from "../../ui";
  import {
    collectionApply,
    collectionImportPreview,
    detectConflicts,
    mpVerifyPath,
    onCollectionProgress,
    pickSharedSetFile,
    sharedFileKindOf,
  } from "../../api";
  import { errorCode } from "../../errors";
  import { inlineError } from "../../state/notify.svelte";
  import { blocked, tryStart } from "../../state/ops.svelte";
  import type {
    ApplyReport,
    CollectionProgress,
    Conflict,
    ImportPlan,
    ImportPlanRow,
    ModEntry,
    MpModRef,
    MpVerifyReport,
  } from "../../types";
  import { formatCount, plural } from "../../plural";
  import { displayVersion } from "../../format";
  import { baseName, sharedFileKind } from "../../modsets/modSets";
  import { conflictTitle } from "../../diagnose/conflictCopy";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, untrack } from "svelte";

  type Source = { kind: "link"; url: string } | { kind: "file"; path: string } | { kind: "multiplayer"; path: string };

  interface Props {
    library: ModEntry[];
    /** Tech names active now. */
    active: ReadonlySet<string>;
    /** The active mods as refs, for the Multiplayer check. */
    activeRefs: MpModRef[];
    /** A link to open at once (a silo://collection deep link); `n` changes for each new one. */
    request?: { url: string; n: number } | null;
    /** Save the shared set's mods as a new set (no downloads). Resolves to its id, or null. */
    onSaveAsSet: (name: string, mods: string[]) => Promise<number | null>;
    /** Switch to a set by id (with the usual confirmation). */
    onSwitchTo: (id: number) => void;
    /** Mods were downloaded: rescan the library and reload the sets. */
    onImported: () => void;
    onTurnOn: (techName: string) => void;
    onPark: (techName: string) => void;
    onFindInBrowse: (techName: string) => void;
  }

  let {
    library,
    active,
    activeRefs,
    request = null,
    onSaveAsSet,
    onSwitchTo,
    onImported,
    onTurnOn,
    onPark,
    onFindInBrowse,
  }: Props = $props();

  let linkText = $state("");
  let source = $state<Source | null>(null);
  let reading = $state(false);
  let error = $state("");
  let plan = $state<ImportPlan | null>(null);
  let preflightDeps = $state<{ mod: string; missing: string[] }[]>([]);
  let preflightConflicts = $state<Conflict[]>([]);
  let applying = $state(false);
  let progress = $state<CollectionProgress | null>(null);
  let report = $state<ApplyReport | null>(null);
  let savedId = $state<number | null>(null);
  let checking = $state(false);
  let mpReport = $state<MpVerifyReport | null>(null);
  let saving = $state(false);
  /** The download was refused because the set changed after the preview. */
  let changed = $state(false);
  let unlisten: UnlistenFn | undefined;

  const installed = $derived(library.map((m) => ({ techName: m.techName, version: m.version })));
  const libraryTechs = $derived(new Set(library.map((m) => m.techName)));
  const titleOf = $derived(new Map(library.map((m) => [m.techName, m.title ?? m.techName])));
  const count = (n: number) => `${formatCount(n)} ${plural(n, { one: "mod", other: "mods" })}`;

  // A deep link: open it at once (preview only), but not over a download that is running. One
  // that arrived before Silo read the library (a cold start from the link) is previewed again
  // once the library is there, unless something was already saved or downloaded from it.
  let previewedEmpty = false;
  $effect(() => {
    const r = request;
    const hasLibrary = library.length > 0;
    const busy = applying || saving;
    untrack(() => {
      if (!r || busy) return;
      if (r.n !== handledRequest) {
        handledRequest = r.n;
        previewedEmpty = !hasLibrary;
        linkText = r.url;
        void open({ kind: "link", url: r.url });
      } else if (
        previewedEmpty &&
        hasLibrary &&
        source?.kind === "link" &&
        source.url === r.url &&
        report === null &&
        savedId === null
      ) {
        previewedEmpty = false;
        void open({ kind: "link", url: r.url });
      }
    });
  });

  function reset() {
    error = "";
    plan = null;
    report = null;
    savedId = null;
    mpReport = null;
    changed = false;
    preflightDeps = [];
    preflightConflicts = [];
  }

  /** Bumped per opened source, so a slow preview that was replaced is dropped. */
  let seq = 0;

  async function open(s: Source) {
    reset();
    source = s;
    const my = ++seq;
    if (s.kind === "multiplayer") {
      reading = false;
      return; // checked on click
    }
    reading = true;
    try {
      const p = await collectionImportPreview(
        s.kind === "link" ? s.url : "",
        installed,
        s.kind === "file" ? s.path : null,
      );
      if (my !== seq) return;
      plan = p;
      await computePreflight(p, my);
    } catch (e) {
      if (my === seq) error = inlineError(e, "Opening the shared set");
    } finally {
      if (my === seq) reading = false;
    }
  }

  // Enter in the link field reaches these too, so they check the same things the buttons do (#358).
  const pickLocked = () => reading || applying || saving;

  async function openLink() {
    if (pickLocked()) return;
    const url = linkText.trim();
    if (!url) return;
    await open({ kind: "link", url });
  }

  async function chooseFile() {
    if (pickLocked()) return;
    let path: string | null;
    try {
      path = await pickSharedSetFile();
    } catch (e) {
      error = inlineError(e, "Choosing a file");
      return;
    }
    if (!path) return;
    // By content, so a check file saved as .json still opens as one; by extension if unreadable.
    let kind = sharedFileKind(path);
    try {
      kind = await sharedFileKindOf(path);
    } catch {
      /* the preview will say what's wrong with the file */
    }
    if (pickLocked()) return; // a download or save started while the file dialog was open
    await open(kind === "multiplayer" ? { kind: "multiplayer", path } : { kind: "file", path });
  }

  /** Every mod the shared set lists, in the preview's buckets. */
  function allMods(p: ImportPlan): string[] {
    const out = new Set<string>();
    for (const b of [p.alreadyPresent, p.versionDrift, p.willInstall, p.openPage, p.unresolved])
      for (const r of b) out.add(r.techName);
    return [...out];
  }

  // Advisory: dependency gaps and conflicts among the set's mods you already have. `my` is the
  // preview's seq: a conflict scan that lands after another set was opened is dropped (#358).
  async function computePreflight(p: ImportPlan, my: number) {
    const setTechs = new Set(allMods(p));
    const byTech = new Map(library.map((m) => [m.techName, m]));
    const have = [...setTechs].map((t) => byTech.get(t)).filter((m): m is ModEntry => m != null);
    preflightDeps = have
      .map((m) => ({
        mod: m.title ?? m.techName,
        missing: m.dependencies.filter((d) => !libraryTechs.has(d) && !setTechs.has(d)),
      }))
      .filter((d) => d.missing.length > 0);
    if (have.length < 2) {
      preflightConflicts = [];
      return;
    }
    try {
      const cs = await detectConflicts(
        have.map((m) => ({ techName: m.techName, title: m.title, path: m.path, kind: m.kind })),
      );
      if (my === seq) preflightConflicts = cs.filter((c) => c.severity !== "info");
    } catch {
      if (my === seq) preflightConflicts = [];
    }
  }

  async function download() {
    if (!plan || !source || source.kind === "multiplayer" || applying) return;
    const s = source;
    // Read now: the plan this download was asked for, even if another preview replaces it.
    const fingerprint = plan.fingerprint;
    // It downloads mods into the library: a library write, one at a time.
    const op = tryStart("Downloading a shared set's mods", { exclusive: "library-write", progress: true });
    if (!op) {
      error = `${blocked("library-write").reason} Try again when it's done.`;
      return;
    }
    applying = true;
    error = "";
    progress = null;
    let done = false;
    // Another set opened meanwhile (Open and Choose are off while this runs, but a deep link may
    // land): the result belongs to this preview only, so it isn't shown on that one.
    const my = seq;
    try {
      unlisten = await onCollectionProgress((p) => {
        progress = p;
        op.progress(p.done, p.total, p.current);
      });
      const rep = await collectionApply(
        s.kind === "link" ? s.url : "",
        installed,
        fingerprint,
        s.kind === "file" ? s.path : null,
      );
      if (my === seq) {
        report = rep;
        savedId = rep.loadoutId;
      }
      done = true;
    } catch (e) {
      const msg = inlineError(e, "Downloading the shared set's mods");
      if (my === seq) {
        error = msg;
        changed = errorCode(e) === "CHANGED_SINCE_PREVIEW";
      }
    } finally {
      applying = false;
      progress = null;
      unlisten?.();
      unlisten = undefined;
      op.end();
    }
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (done) onImported();
  }

  async function saveOnly() {
    if (!plan || saving || applying) return;
    const p = plan;
    const my = seq;
    saving = true;
    try {
      const id = await onSaveAsSet(p.name, allMods(p));
      if (id !== null && my === seq) {
        savedId = id;
        toast({ message: `Saved ${p.name} as a set.` });
      }
    } finally {
      saving = false;
    }
  }

  function previewAgain() {
    if (source) void open(source.kind === "link" ? { kind: "link", url: source.url } : { ...source });
  }

  async function check() {
    if (!source || source.kind !== "multiplayer" || checking) return;
    checking = true;
    error = "";
    try {
      mpReport = await mpVerifyPath(source.path, activeRefs);
    } catch (e) {
      error = inlineError(e, "Checking your mods");
    } finally {
      checking = false;
    }
  }

  onDestroy(() => unlisten?.());

  const label = (t: string) => titleOf.get(t) ?? t;
</script>

<div class="open-shared">
  <div class="pick">
    <label class="field" for="shared-link">Paste a link, or choose a file</label>
    <div class="row">
      <TextField
        id="shared-link"
        type="url"
        placeholder="https://silo.hllmr.com/c/… or a link from GitHub"
        bind:value={linkText}
        onkeydown={(e: KeyboardEvent) => {
          if (e.key === "Enter") void openLink();
        }}
      />
      <Button
        variant="primary"
        disabled={!linkText.trim() || reading || applying || saving}
        loading={reading && source?.kind === "link"}
        onclick={openLink}>Open</Button
      >
      <Button icon="showFile" disabled={reading || applying || saving} onclick={chooseFile}>Choose a file…</Button>
    </div>
    <p class="hint">
      Opening a set only shows what's in it. Nothing is downloaded, saved or switched until you choose to.
    </p>
  </div>

  {#if error}
    <p class="err" role="status">{error}</p>
    {#if changed}<Button size="sm" onclick={previewAgain}>Preview again</Button>{/if}
  {/if}

  {#if source?.kind === "multiplayer"}
    <section class="result" aria-labelledby="mp-h">
      <h3 id="mp-h">Multiplayer check file</h3>
      <p class="muted">{baseName(source.path)}</p>
      <p>Silo compares the file with the {count(active.size)} you have active: names, versions and each file's fingerprint.</p>
      <div class="actions">
        <Button variant="primary" loading={checking} disabled={checking || activeRefs.length === 0} onclick={check}
          >{mpReport ? "Check again" : "Check my mods"}</Button
        >
      </div>
      {#if mpReport}
        {@const r = mpReport}
        {#if r.ok}
          <p class="verdict good"><Icon name="ok" size={16} /> You match. You're ready to join.</p>
        {:else}
          <p class="verdict bad"><Icon name="problem" size={16} /> Your mods don't match yet. Fix these, then check again.</p>
          {#if r.missing.length}
            <h4>The server has these and your active mods don't ({r.missing.length})</h4>
            {#each r.missing as m (m.techName)}
              <div class="mod">
                <span class="mn">{label(m.techName)}</span>
                <span class="rt">{m.version ? displayVersion(m.version) : ""}</span>
                {#if libraryTechs.has(m.techName)}
                  <Button size="sm" aria-label="Turn on: {label(m.techName)}" onclick={() => onTurnOn(m.techName)}>Turn on</Button>
                {:else}
                  <Button size="sm" variant="ghost" aria-label="Find in Browse: {label(m.techName)}" onclick={() => onFindInBrowse(m.techName)}>Find in Browse</Button>
                {/if}
              </div>
            {/each}
          {/if}
          {#if r.versionMismatch.length || r.hashMismatch.length}
            <h4>Different version or file than the server ({r.versionMismatch.length + r.hashMismatch.length})</h4>
            {#each [...r.versionMismatch, ...r.hashMismatch] as m, i (m.techName + i)}
              <div class="mod">
                <span class="mn">{label(m.techName)}</span>
                <span class="rt">{i < r.versionMismatch.length && m.got && m.expected && m.got !== m.expected ? `yours ${m.got}, server ${m.expected}` : "a different file"}</span>
                <Button size="sm" variant="ghost" aria-label="Find in Browse: {label(m.techName)}" onclick={() => onFindInBrowse(m.techName)}>Find in Browse</Button>
              </div>
            {/each}
          {/if}
          {#if r.extra.length}
            <h4>Active for you but not on the server ({r.extra.length})</h4>
            {#each r.extra as t (t)}
              <div class="mod">
                <span class="mn">{label(t)}</span>
                <span class="rt"></span>
                <Button size="sm" aria-label="Park: {label(t)}" onclick={() => onPark(t)}>Park</Button>
              </div>
            {/each}
          {/if}
        {/if}
      {/if}
    </section>
  {:else if reading}
    <p class="muted" role="status">Reading the shared set…</p>
  {:else if plan}
    {@const p = plan}
    <section class="result" aria-labelledby="plan-h">
      <h3 id="plan-h">{p.name}</h3>
      {#if p.author}<p class="muted">Shared by {p.author}</p>{/if}
      {#if p.description}<p>{p.description}</p>{/if}

      {#snippet bucket(title: string, rows: ImportPlanRow[], kind: "plain" | "drift" | "missing" = "plain")}
        {#if rows.length > 0}
          <h4>{title} ({rows.length})</h4>
          {#each rows as m, i (m.techName + i)}
            <div class="mod">
              <span class="mn">{label(m.techName)}</span>
              <span class="rt">
                {#if kind === "drift"}yours {m.installedVersion ? displayVersion(m.installedVersion) : "unknown"}, set {m.version ? displayVersion(m.version) : "unknown"}
                {:else}{m.version ? displayVersion(m.version) : ""}{m.source ? ` · ${m.source === "modhub" ? "ModHub" : m.source === "github" ? "GitHub" : m.source}` : ""}{/if}
              </span>
              {#if kind === "missing"}
                <Button size="sm" variant="ghost" aria-label="Find in Browse: {label(m.techName)}" onclick={() => onFindInBrowse(m.techName)}>Find in Browse</Button>
              {/if}
            </div>
          {/each}
        {/if}
      {/snippet}

      {@render bucket("Silo can download", p.willInstall)}
      {@render bucket("Get from ModHub", p.openPage, "missing")}
      {@render bucket("Different version than yours", p.versionDrift, "drift")}
      {@render bucket("Couldn't find", p.unresolved, "missing")}
      {@render bucket("Already in your library", p.alreadyPresent)}

      {#if preflightDeps.length > 0 || preflightConflicts.length > 0}
        <div class="heads-up">
          <h4>Heads-up</h4>
          {#each preflightConflicts as c, i (c.kind + c.mods.join() + i)}
            <div class="mod"><span class="mn"><Icon name="problem" size={13} /> {conflictTitle(c)}</span><span class="rt">{c.mods.join(", ")}</span></div>
          {/each}
          {#each preflightDeps as d (d.mod)}
            <div class="mod"><span class="mn">{d.mod}</span><span class="rt">needs {d.missing.join(", ")}, which {d.missing.length === 1 ? "isn't" : "aren't"} in the set or your library</span></div>
          {/each}
          <p class="muted">Checked against the mods you already have. Silo checks again after downloading.</p>
        </div>
      {/if}

      {#if report}
        {#if report.loadoutId === null}
          <p class="err">Silo was busy and couldn't save {p.name} as a set. {report.failed > 0 ? `${count(report.failed)} failed. ` : ""}Try again in a moment.</p>
        {:else}
          <p class="verdict good">
            Saved {p.name} as a set. Downloaded {count(report.installed)}{report.failed > 0 ? `; ${count(report.failed)} failed` : ""}.
          </p>
        {/if}
        {#each report.rows.filter((r) => r.status === "failed") as r (r.techName)}
          <div class="mod"><span class="mn">{label(r.techName)}</span><span class="rt err">{r.detail ?? "Couldn't download"}</span></div>
        {/each}
      {:else if savedId === null}
        <div class="actions">
          {#if p.willInstall.length > 0}
            <Button variant="primary" icon="download" loading={applying} disabled={applying || saving} onclick={download}
              >Download {count(p.willInstall.length)} and save as a set</Button
            >
            <Button variant="ghost" disabled={applying || saving} onclick={saveOnly}>Save as a set without downloading</Button>
          {:else}
            <Button variant="primary" loading={saving} disabled={saving} onclick={saveOnly}>Save as a set</Button>
          {/if}
        </div>
        {#if applying && progress}
          <p class="muted" role="status">Downloading {progress.done} of {progress.total}{progress.current ? `: ${progress.current}` : ""}</p>
        {/if}
      {/if}
      {#if savedId !== null}
        <div class="actions">
          <Button onclick={() => onSwitchTo(savedId!)}>Switch to it</Button>
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .open-shared {
    max-width: 52rem;
    display: flex;
    flex-direction: column;
    gap: var(--sp-4);
  }
  .field {
    display: block;
    font-weight: 600;
    font-size: var(--fs-sm);
    margin-bottom: var(--sp-2);
  }
  .row {
    display: flex;
    gap: var(--sp-2);
    align-items: center;
  }
  .row :global(input) {
    flex: 1 1 auto;
    min-width: 0;
  }
  .row :global(:first-child) {
    flex: 1 1 auto;
  }
  .hint,
  .muted {
    margin: var(--sp-1) 0 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .result {
    background: var(--surface-raised);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: var(--sp-4);
    display: flex;
    flex-direction: column;
    gap: var(--sp-2);
  }
  .result p {
    margin: 0;
    font-size: var(--fs-sm);
  }
  h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: var(--fs-lg);
  }
  h4 {
    margin: var(--sp-3) 0 0;
    font-size: var(--fs-xs);
    font-weight: 600;
    color: var(--text-muted);
  }
  .mod {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    padding: 4px 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--fs-sm);
  }
  .mn {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .rt {
    flex: 0 1 auto;
    color: var(--text-muted);
    font-size: var(--fs-xs);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-2);
    margin-top: var(--sp-3);
  }
  .verdict {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    font-weight: 600;
  }
  .verdict.good {
    color: var(--primary-text);
  }
  .verdict.bad {
    color: var(--warn-text);
  }
  .heads-up {
    margin-top: var(--sp-2);
    padding: var(--sp-2) var(--sp-3);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .err {
    margin: 0;
    font-size: var(--fs-sm);
    color: var(--danger);
  }
</style>
