<script lang="ts">
  // Collections: share a curated set of mods as a link (to your own GitHub), or open
  // a link someone sent you — preview what you have / need, then install + verify the set.
  // Unlike Multiplayer sync (which matches your ACTIVE set to a host's for a session),
  // Collections are about sharing groups of mods, independent of any session.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import {
    collectionExport,
    collectionImportPreview,
    collectionApply,
    onCollectionProgress,
    detectConflicts,
    ghStatus,
    openExternal,
    collectionsList,
    collectionDelete,
    collectionUpdate,
    copyText,
  } from "../api";
  import { ask } from "../ui";
  import { blocked, tryStart } from "../state/ops.svelte";
  import type { CollectionSummary } from "../api";
  import type {
    MpModRef,
    CollectionExportResult,
    ImportPlan,
    ApplyReport,
    CollectionProgress,
    GhStatus,
    ModEntry,
    Conflict,
  } from "../types";
  import type { UnlistenFn } from "@tauri-apps/api/event";
  import { onDestroy, onMount } from "svelte";

  interface Props {
    /** The active set as (techName, path, kind, version) refs — the source for "share". */
    active: MpModRef[];
    /** The whole installed library — for import bucketing + dependency/conflict pre-flight. */
    library: ModEntry[];
    /** A collection URL to import immediately (from a silo:// deep link). Pre-fills + previews. */
    initialImportUrl?: string | null;
    /** Called after a successful import so the parent can rescan the library. */
    onImported?: () => void;
    /** Shows a close button (the page has none). */
    onClose?: () => void;
  }
  let { active, library, initialImportUrl = null, onImported, onClose }: Props = $props();
  const installed = $derived(library.map((m) => ({ techName: m.techName, version: m.version })));

  const zipCount = $derived(active.filter((m) => m.kind === "zip").length);

  // ── Share as a link ──
  let canGist = $state(false);
  let canWrite = $state(false);
  let sharePublic = $state(false);
  let collName = $state("");
  let shareBusy = $state(false);
  let shareErr = $state<string | null>(null);
  let shareResult = $state<CollectionExportResult | null>(null);
  let copied = $state(false);

  // Private (secret gist) needs the gist scope; public (repo) needs public_repo.
  const missingScope = $derived(sharePublic ? !canWrite : !canGist);

  async function refreshGh() {
    try {
      const s: GhStatus = await ghStatus();
      canGist = s.canGist;
      canWrite = s.canWrite;
    } catch {
      canGist = false;
      canWrite = false;
    }
    // Only worth listing when a scope that can publish a collection is present — otherwise
    // the list is always empty (and the command would need a token anyway).
    if (canGist || canWrite) void loadMyCollections();
  }
  refreshGh();

  // ── Your collections (management) ──
  let myColls = $state<CollectionSummary[]>([]);
  let myLoading = $state(false);
  let myErr = $state<string | null>(null);
  let copiedRef = $state<string | null>(null);
  let deletingRef = $state<string | null>(null);
  let updatingRef = $state<string | null>(null);
  let updatedNote = $state<string | null>(null);

  async function loadMyCollections() {
    myLoading = true;
    myErr = null;
    try {
      myColls = await collectionsList();
    } catch (e) {
      myErr = inlineError(e, "Loading your collections");
    } finally {
      myLoading = false;
    }
  }

  async function copyShareLink(c: CollectionSummary) {
    try {
      await copyText(c.pageUrl);
      copiedRef = c.reference;
      setTimeout(() => {
        if (copiedRef === c.reference) copiedRef = null;
      }, 1500);
    } catch {
      /* clipboard blocked — ignore */
    }
  }

  async function doUpdate(c: CollectionSummary) {
    if (updatingRef) return;
    if (
      !(await ask({
        title: `Update “${c.name}” to your current active set?`,
        body:
          `Its mod list is replaced with your ${zipCount} packaged mod${zipCount === 1 ? "" : "s"}; the share link stays the same. ` +
          "To go back, switch to the old set and update it again.",
        confirm: "Update collection",
      }))
    )
      return;
    updatingRef = c.reference;
    myErr = null;
    updatedNote = null;
    try {
      const r = await collectionUpdate(c.reference, active);
      await loadMyCollections();
      updatedNote = `Updated “${c.name}” to ${r.count} mod${r.count === 1 ? "" : "s"}.`;
      setTimeout(() => {
        updatedNote = null;
      }, 4000);
    } catch (e) {
      myErr = inlineError(e, "Updating the collection");
    } finally {
      updatingRef = null;
    }
  }

  async function doDelete(c: CollectionSummary) {
    if (!c.canDelete || deletingRef) return;
    if (
      !(await ask({
        title: `Delete “${c.name}”?`,
        body: "It's removed from your GitHub and its share link stops working. This can't be undone; you can share the mods again as a new collection with a new link.",
        confirm: "Delete",
        tone: "danger",
      }))
    )
      return;
    deletingRef = c.reference;
    myErr = null;
    try {
      await collectionDelete(c.kind, c.reference);
      myColls = myColls.filter((x) => x.reference !== c.reference);
    } catch (e) {
      myErr = inlineError(e, "Deleting the collection");
    } finally {
      deletingRef = null;
    }
  }

  async function doShare() {
    if (!collName.trim()) return;
    shareBusy = true;
    shareErr = null;
    shareResult = null;
    copied = false;
    try {
      shareResult = await collectionExport(collName.trim(), null, sharePublic, active);
      void loadMyCollections();
    } catch (e) {
      shareErr = inlineError(e, "Sharing the collection");
    } finally {
      shareBusy = false;
    }
  }

  async function copyLink() {
    if (!shareResult) return;
    try {
      await copyText(shareResult.url);
      copied = true;
    } catch {
      copied = false;
    }
  }

  // ── Open a shared link (import preview) ──
  let importUrl = $state("");
  let importBusy = $state(false);
  let importErr = $state<string | null>(null);
  let plan = $state<ImportPlan | null>(null);

  // Pre-flight (advisory): dependency gaps + conflicts, checkable only against the
  // collection's mods you ALREADY have — the rest get the full check after import + rescan.
  let preflightDeps = $state<{ mod: string; missing: string[] }[]>([]);
  let preflightConflicts = $state<Conflict[]>([]);

  async function computePreflight(p: ImportPlan) {
    const collTechs = new Set<string>();
    for (const bucket of [
      p.willInstall,
      p.openPage,
      p.alreadyPresent,
      p.versionDrift,
      p.unresolved,
    ]) {
      for (const r of bucket) collTechs.add(r.techName);
    }
    const libByTech = new Map(library.map((m) => [m.techName, m]));
    const libTechs = new Set(library.map((m) => m.techName));
    // The collection's mods we can actually inspect (on disk).
    const have = [...collTechs]
      .map((t) => libByTech.get(t))
      .filter((m): m is ModEntry => m != null);

    // A dependency is satisfied if it's in your library OR elsewhere in the collection.
    preflightDeps = have
      .map((m) => ({
        mod: m.techName,
        missing: m.dependencies.filter((d) => !libTechs.has(d) && !collTechs.has(d)),
      }))
      .filter((d) => d.missing.length > 0);

    // Conflicts among the collection's mods you have (they'd be active together).
    if (have.length >= 2) {
      try {
        const cs = await detectConflicts(
          have.map((m) => ({ techName: m.techName, title: m.title, path: m.path, kind: m.kind })),
        );
        preflightConflicts = cs.filter((c) => c.severity !== "info");
      } catch {
        preflightConflicts = [];
      }
    } else {
      preflightConflicts = [];
    }
  }

  async function doPreview() {
    if (!importUrl.trim()) return;
    importBusy = true;
    importErr = null;
    plan = null;
    applyReport = null;
    preflightDeps = [];
    preflightConflicts = [];
    try {
      plan = await collectionImportPreview(importUrl.trim(), installed);
      await computePreflight(plan);
    } catch (e) {
      importErr = inlineError(e, "Opening the collection link");
    } finally {
      importBusy = false;
    }
  }

  let applyBusy = $state(false);
  let applyProgress = $state<CollectionProgress | null>(null);
  let applyReport = $state<ApplyReport | null>(null);
  let unlisten: UnlistenFn | undefined;

  async function doApply() {
    if (!importUrl.trim() || applyBusy) return;
    // Only apply what was previewed: apply refuses if the collection changed since.
    if (!plan) return;
    const fingerprint = plan.fingerprint;
    // It downloads mods into the library: a library write, one at a time.
    const op = tryStart("Adding a collection's mods", { exclusive: "library-write", progress: true });
    if (!op) {
      importErr = `${blocked("library-write").reason} Try again when it's done.`;
      return;
    }
    applyBusy = true;
    importErr = null;
    applyProgress = null;
    let applied = false;
    try {
      unlisten = await onCollectionProgress((p) => {
        applyProgress = p;
        op.progress(p.done, p.total, p.current);
      });
      applyReport = await collectionApply(importUrl.trim(), installed, fingerprint);
      applied = true;
    } catch (e) {
      importErr = inlineError(e, "Adding the collection's mods");
    } finally {
      applyBusy = false;
      applyProgress = null;
      unlisten?.();
      unlisten = undefined;
      op.end();
    }
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (applied) onImported?.();
  }

  onDestroy(() => unlisten?.());

  // Deep-link entry: if opened via silo://collection?url=…, pre-fill and preview at once.
  onMount(() => {
    if (initialImportUrl) {
      importUrl = initialImportUrl;
      void doPreview();
    }
  });
</script>

<div class="coll">
  <div class="coll-head">
    <div>
      <h3 class="coll-title">Collections</h3>
      <p class="sub">
        Share a curated set of mods as a link — anyone who opens it in Silo can preview what
        they already have, install the rest, and verify they got the exact set. Only the list
        is shared; no mod files are uploaded.
      </p>
    </div>
    {#if onClose}
      <button class="x" title="Close" aria-label="Close" onclick={onClose}><Icon name="close" /></button>
    {/if}
  </div>

  <div class="share">
    <div class="card-title">Share as a link</div>
    <p class="card-body">
      Publishes a collection — the list of your {zipCount} packaged mods, each pinned to a
      version and a content hash — to your own GitHub, and gives you a link. Whoever opens it
      in Silo can install and verify the exact set. No mod files are uploaded, only the list.
    </p>

    {#if shareResult}
      {@const r = shareResult}
      <div class="ok-note">
        Collection created with {r.count} mod{r.count === 1 ? "" : "s"}.
        {#if r.omitted.length > 0}
          {r.omitted.length} dev-mod folder{r.omitted.length === 1 ? " was" : "s were"}
          left out (no fixed bytes to pin).
        {/if}
      </div>
      <div class="share-link">
        <a class="mn link" href={r.url} onclick={(e) => { e.preventDefault(); openExternal(r.url); }}>
          {r.url}
        </a>
        <button class="btn" onclick={copyLink}>{#if copied}Copied <Icon name="check" size={14} />{:else}Copy link{/if}</button>
      </div>
      <p class="caveat">
        {#if sharePublic}
          <b>Public</b>: anyone with the link can view, install, and fork it — good for sharing
          widely. It's a normal repo on your GitHub; delete it there to unpublish.
        {:else}
          <b>Secret</b> gist: unlisted, but <b>not</b> password-protected — anyone with the link
          can see the list. Share it only with your group.
        {/if}
      </p>
    {:else}
      <input
        class="share-input"
        placeholder="Collection name (e.g. Weekend Co-op Pack)"
        bind:value={collName}
        maxlength="80"
      />
      <div class="vis-row">
        <label><input type="radio" bind:group={sharePublic} value={false} /> Private — secret gist</label>
        <label><input type="radio" bind:group={sharePublic} value={true} /> Public — forkable repo</label>
      </div>
      {#if missingScope}
        <p class="caveat">
          {#if sharePublic}
            Public collections need <b>Enable actions</b> in Settings → GitHub (the
            <code>public_repo</code> permission).
          {:else}
            Private collections need <b>Enable collection sharing</b> in Settings → GitHub (the
            <code>gist</code> permission).
          {/if}
        </p>
      {:else}
        <button
          class="btn primary"
          onclick={doShare}
          disabled={shareBusy || !collName.trim() || zipCount === 0}
        >
          {shareBusy ? "Publishing…" : "Create share link →"}
        </button>
      {/if}
      {#if shareErr}<div class="err">{shareErr}</div>{/if}
    {/if}
  </div>

  {#if canGist || canWrite}
    <div class="share">
      <div class="card-title">Your collections</div>
      <p class="card-body">
        Collections you've published to your GitHub. Copy a share link again, open its page,
        or remove one.
      </p>
      {#if updatedNote}<div class="ok-note">{updatedNote}</div>{/if}
      {#if myLoading}
        <div class="busy">Loading your collections…</div>
      {:else if myErr}
        <div class="err">{myErr}</div>
      {:else if myColls.length === 0}
        <p class="caveat" style="margin-top:6px">Nothing published yet — create one above.</p>
      {:else}
        {#each myColls as c (c.reference)}
          <div class="coll-row">
            <div class="coll-info">
              <span class="coll-title" title={c.name}>{c.name}</span>
              <span class="coll-sub">
                <span class="kind {c.kind}">{c.kind === "repo" ? "Public" : "Secret"}</span>
                {c.modCount} mod{c.modCount === 1 ? "" : "s"}
              </span>
            </div>
            <div class="coll-actions">
              <button class="mini" onclick={() => copyShareLink(c)}>
                {#if copiedRef === c.reference}Copied <Icon name="check" size={12} />{:else}Copy link{/if}
              </button>
              <button class="mini" onclick={() => openExternal(c.pageUrl)} title="Open the share page">Page <Icon name="external" size={12} /></button>
              <button class="mini" onclick={() => openExternal(c.sourceUrl)} title="Open on GitHub">GitHub <Icon name="external" size={12} /></button>
              <button
                class="mini"
                onclick={() => doUpdate(c)}
                disabled={updatingRef === c.reference || zipCount === 0}
                title="Re-pin this collection to your current active set (same link)"
              >
                {updatingRef === c.reference ? "Updating…" : "Update"}
              </button>
              {#if c.canDelete}
                <button
                  class="mini danger"
                  onclick={() => doDelete(c)}
                  disabled={deletingRef === c.reference}
                >
                  {deletingRef === c.reference ? "Deleting…" : "Delete"}
                </button>
              {:else}
                <button
                  class="mini"
                  onclick={() => openExternal(c.sourceUrl)}
                  title="Public collection repos are deleted on GitHub"
                >
                  Delete on GitHub <Icon name="external" size={12} />
                </button>
              {/if}
            </div>
          </div>
        {/each}
      {/if}
    </div>
  {/if}

  <div class="share">
    <div class="card-title">Open a shared link</div>
    <p class="card-body">
      Paste a Silo collection link someone sent you. Silo shows what you already have, what's
      a different version, what it can install for you, and what you'll need to grab yourself —
      before anything is downloaded.
    </p>
    <div class="share-form">
      <input
        class="share-input"
        placeholder="Paste a gist or github.com/owner/repo link…"
        bind:value={importUrl}
      />
      <button class="btn primary" onclick={doPreview} disabled={importBusy || !importUrl.trim()}>
        {importBusy ? "Reading…" : "Preview →"}
      </button>
    </div>
    {#if importErr}<div class="err" style="margin-top:10px">{importErr}</div>{/if}

    {#if plan}
      {@const p = plan}
      <div class="plan-head">
        <strong>{p.name}</strong>
        {#if p.author}<span class="rt">by {p.author}</span>{/if}
      </div>
      {#if p.description}<p class="caveat" style="margin-top:4px">{p.description}</p>{/if}

      {#snippet bucket(title: string, rows: typeof p.willInstall, drift = false)}
        {#if rows.length > 0}
          <div class="sec">{title} ({rows.length})</div>
          {#each rows as m (m.techName)}
            <div class="row">
              <span class="mn">{m.techName}</span>
              <span class="rt">
                {#if drift}yours {m.installedVersion || "?"} → collection {m.version || "?"}
                {:else}{m.version || ""}{m.source ? ` · ${m.source}` : ""}{/if}
              </span>
            </div>
          {/each}
        {/if}
      {/snippet}

      {@render bucket("Silo can install these", p.willInstall)}
      {@render bucket("Get these yourself from ModHub", p.openPage)}
      {@render bucket("Different version — update to match", p.versionDrift, true)}
      {@render bucket("Not in the catalog — find these manually", p.unresolved)}
      {@render bucket("Already in your library", p.alreadyPresent)}

      {#if preflightDeps.length > 0 || preflightConflicts.length > 0}
        <div class="heads-up">
          <div class="sec" style="margin-top:0">Heads-up (from the mods you already have)</div>
          {#each preflightConflicts as c (c.kind + c.mods.join())}
            <div class="row">
              <span class="mn"><Icon name="problem" size={13} /> {c.name}</span>
              <span class="rt">{c.mods.join(" ↔ ")}</span>
            </div>
          {/each}
          {#each preflightDeps as d (d.mod)}
            <div class="row">
              <span class="mn">{d.mod}</span>
              <span class="rt">needs {d.missing.join(", ")} — not in the collection or your library</span>
            </div>
          {/each}
          <p class="caveat" style="margin-top:6px">
            Checked against the mods already on disk. Silo re-runs the full conflict &amp;
            dependency check after import.
          </p>
        </div>
      {/if}

      {#if !applyReport}
        <div class="apply-bar">
          <p class="caveat" style="margin:0">
            Import saves the whole set as a loadout and downloads the
            {p.willInstall.length} installable mod{p.willInstall.length === 1 ? "" : "s"}. The
            rest are listed in the loadout, ready once you fetch them.
          </p>
          <button class="btn primary" onclick={doApply} disabled={applyBusy}>
            {applyBusy ? "Importing…" : "Import collection →"}
          </button>
        </div>
        {#if applyBusy && applyProgress}
          <div class="busy">
            Installing {applyProgress.done}/{applyProgress.total}
            {applyProgress.current ? `— ${applyProgress.current}` : ""}
          </div>
        {/if}
      {:else}
        {@const a = applyReport}
        {#if a.loadoutId === null}
          <div class="err">
            Didn't save “{p.name}” as a loadout. {a.failed} failed. Try again in a moment.
          </div>
        {:else}
          <div class="ok-note">
            Saved “{p.name}” as a loadout.
            {a.installed} installed{a.failed > 0 ? `, ${a.failed} failed` : ""}. Rescanning your
            library…
          </div>
        {/if}
        {#each a.rows.filter((r) => r.status === "installed" || r.status === "failed") as r (r.techName)}
          <div class="row">
            <span class="mn">{r.techName}</span>
            <span class="rt">
              {#if r.status === "failed"}<Icon name="problem" size={13} /> {r.detail ?? "failed"}
              {:else if r.verdict === "verified"}<Icon name="verifiedBuild" size={13} /> verified
              {:else if r.verdict === "modified"}<Icon name="modified" size={13} /> modified — differs from the shared build
              {:else}installed (unverified){/if}
            </span>
          </div>
        {/each}
        {#if a.loadoutId !== null}
          <p class="caveat">
            Apply the “{p.name}” loadout from the Loadouts panel to activate it.
          </p>
        {/if}
      {/if}
    {/if}
  </div>
</div>

<style>
  .coll {
    padding: 16px 18px 22px;
  }
  .coll-head {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .coll-head .coll-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.35rem;
    color: var(--text);
  }
  .sub {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
    line-height: 1.5;
    max-width: 56ch;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 1rem;
    align-self: start;
  }
  .x:hover {
    color: var(--text);
  }
  .busy {
    color: var(--text-muted);
    font-size: 0.85rem;
    margin-bottom: 8px;
    font-variant-numeric: tabular-nums;
  }
  .err {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .ok-note {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--primary-text);
    border-radius: var(--radius-sm);
    padding: 8px 12px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .card-title {
    font-weight: 600;
    color: var(--text);
  }
  .card-body {
    margin: 6px 0 10px;
    color: var(--text-muted);
    font-size: 0.85rem;
    line-height: 1.5;
  }
  .btn {
    padding: 8px 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn.primary {
    background: var(--primary);
    color: var(--on-primary);
    border-color: transparent;
    font-weight: 600;
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .caveat {
    margin: 12px 0 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .share {
    margin-top: 18px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-raised);
  }
  .share:first-of-type {
    margin-top: 4px;
  }
  .share-form {
    display: flex;
    gap: 10px;
  }
  .share-input {
    flex: 1;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
  }
  .vis-row {
    display: flex;
    gap: 16px;
    margin: 8px 0;
    font-size: var(--fs-sm);
    color: var(--text-muted);
  }
  .vis-row label {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .share-link {
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 4px 0 4px;
  }
  .coll-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 9px 4px;
    border-top: 1px solid var(--border);
  }
  .coll-row:first-of-type {
    border-top: none;
    margin-top: 4px;
  }
  .coll-info {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .coll-title {
    font-weight: 600;
    font-size: 0.9rem;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .coll-sub {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .kind {
    font-size: var(--fs-xs);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 1px 6px;
    border-radius: 999px;
    border: 1px solid var(--border);
  }
  .kind.repo {
    color: var(--info);
    border-color: color-mix(in srgb, var(--info) 40%, var(--border));
  }
  .kind.gist {
    color: var(--text-muted);
  }
  .coll-actions {
    flex: 0 0 auto;
    display: flex;
    gap: 4px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }
  .mini {
    padding: 4px 9px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: var(--fs-xs);
    cursor: pointer;
  }
  .mini:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
  }
  .mini.danger:hover:not(:disabled) {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, var(--border));
  }
  .mini:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .heads-up {
    margin-top: 12px;
    padding: 8px 10px;
    border: 1px solid color-mix(in srgb, var(--warn, orange) 35%, var(--border));
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--warn, orange) 7%, transparent);
  }
  .apply-bar {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 12px;
  }
  .apply-bar .btn {
    flex: 0 0 auto;
  }
  .link {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--info);
    text-decoration: none;
  }
  .link:hover {
    text-decoration: underline;
  }
  .plan-head {
    margin-top: 12px;
  }
  .sec {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    margin: 14px 0 5px;
  }
  .row {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 5px 8px;
    border-radius: var(--radius-sm);
    font-size: 0.85rem;
  }
  .row:nth-child(odd) {
    background: var(--bg);
  }
  .mn {
    color: var(--text);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-sm);
  }
  .rt {
    color: var(--text-muted);
    font-size: var(--fs-sm);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
</style>
