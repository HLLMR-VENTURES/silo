<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import type { ModEntry, UpdateInfo } from "../types";
  import { setModRepo, guessRepo, checkModUpdate, downloadUpdate, openExternal } from "../api";
  import { githubRepoUrl } from "../links";
  import { ask } from "../ui";
  import { blocked, tryStart } from "../state/ops.svelte";

  let {
    mod,
    repo,
    onRepoChange,
    onInstalled,
  }: {
    mod: ModEntry;
    repo: { owner: string; repo: string } | null;
    onRepoChange: (r: { owner: string; repo: string } | null) => void;
    onInstalled: () => void;
  } = $props();

  let repoInput = $state("");
  let update = $state<UpdateInfo | null>(null);
  let checking = $state(false);
  let ghError = $state<string | null>(null);
  let suggested = $state(false);
  let installing = $state(false);

  $effect(() => {
    const m = mod;
    repoInput = repo ? `${repo.owner}/${repo.repo}` : "";
    update = null;
    ghError = null;
    suggested = false;
    // Auto-suggest a repo from the mod's modDesc when none is linked yet.
    if (!repo) {
      guessRepo(m.path, m.kind).then((g) => {
        if (g && mod.path === m.path && !repoInput) {
          repoInput = `${g.owner}/${g.repo}`;
          suggested = true;
        }
      }).catch(() => {}); // only a suggestion: no guess is fine
    }
  });

  async function linkRepo() {
    const parts = repoInput.trim().replace(/^https?:\/\/github\.com\//i, "").split("/");
    const owner = parts[0]?.trim() ?? "";
    const r = parts[1]?.trim() ?? "";
    try {
      await setModRepo(mod.techName, owner, r);
      onRepoChange(owner && r ? { owner, repo: r } : null);
    } catch (e) {
      ghError = inlineError(e, "Linking the repo");
    }
  }

  async function checkUpdate() {
    if (!repo) return;
    checking = true;
    ghError = null;
    update = null;
    try {
      update = await checkModUpdate(repo.owner, repo.repo, mod.version ?? "0");
    } catch (e) {
      ghError = inlineError(e, "Checking for an update");
    }
    checking = false;
  }

  async function installUpdate() {
    if (!update?.release.assetUrl) return;
    if (
      !(await ask({
        title: `Install ${update.release.tag}?`,
        body: "Silo downloads it and replaces the installed version. The current version is kept in your mods folder's backups, so you can put it back from there.",
        confirm: "Download and install",
      }))
    )
      return;
    // It replaces a mod in the library: a library write, one at a time.
    const op = tryStart("Installing an update", { exclusive: "library-write" });
    if (!op) {
      ghError = `${blocked("library-write").reason} Try again when it's done.`;
      return;
    }
    installing = true;
    ghError = null;
    installNote = null;
    const path = mod.path;
    let installed = false;
    try {
      const out = await downloadUpdate(path, update.release.assetUrl);
      installNote = {
        path,
        backup: out.backup ? `The previous version is kept in ${out.backup}.` : null,
        warning: out.problems.length ? out.problems.join(" ") : null,
      };
      installed = true;
    } catch (e) {
      ghError = inlineError(e, "Installing the update");
    } finally {
      op.end();
    }
    installing = false;
    // After the op ends: the rescan it triggers skips while a library write is running.
    if (installed) onInstalled();
  }

  // What the last install said. Kept across the rescan `onInstalled` triggers (which hands this
  // panel a fresh `mod`), and shown only while the same mod is open.
  let installNote = $state<{ path: string; backup: string | null; warning: string | null } | null>(
    null,
  );
  const note = $derived(installNote?.path === mod.path ? installNote : null);
</script>

<div class="d-section">
  <h4 class="d-label">GitHub repo</h4>
  <div class="gh-link">
    <input
      class="gh-input"
      aria-label="GitHub repo, as owner/repo"
      placeholder="owner/repo"
      bind:value={repoInput}
      onkeydown={(e) => e.key === "Enter" && linkRepo()}
    />
    <button class="gh-btn" onclick={linkRepo}>{repo ? "Update link" : "Link"}</button>
  </div>
  {#if suggested && !repo}<div class="gh-suggest">Found in the mod's own files. Select Link to confirm.</div>{/if}
  {#if repo}
    <div class="gh-actions">
      <button class="gh-btn" onclick={checkUpdate} disabled={checking}>
        {checking ? "Checking…" : "Check for update"}
      </button>
      <button class="gh-open" onclick={() => openExternal(githubRepoUrl(repo.owner, repo.repo))}>
        Open repo <Icon name="external" size={12} />
      </button>
    </div>
  {/if}
  {#if ghError}<div class="gh-err">{ghError}</div>{/if}
  {#if note?.warning}<div class="gh-err">{note.warning}</div>{/if}
  {#if note?.backup}<div class="gh-suggest">Updated. {note.backup}</div>{/if}
  {#if update}
    <div class="gh-result" class:has={update.hasUpdate}>
      {#if update.hasUpdate}
        <b>Update available:</b> {update.release.tag} (you have {update.current})
      {:else}
        Up to date. Latest is {update.release.tag}
      {/if}
      {#if update.release.htmlUrl}
        <button class="gh-open" onclick={() => openExternal(update!.release.htmlUrl!)}>
          View release <Icon name="external" size={12} />
        </button>
      {/if}
      {#if update.hasUpdate && update.release.assetUrl}
        <button class="gh-btn" onclick={installUpdate} disabled={installing}>
          {installing ? "Installing…" : "Download & install"}
        </button>
      {/if}
    </div>
  {/if}
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
  .gh-link {
    display: flex;
    gap: 6px;
  }
  .gh-input {
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
  .gh-btn {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 7px 12px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .gh-btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .gh-actions {
    display: flex;
    gap: 6px;
    margin-top: 8px;
  }
  .gh-open {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .gh-open:hover {
    text-decoration: underline;
  }
  .gh-err {
    margin-top: 8px;
    color: var(--danger);
    font-size: var(--fs-xs);
  }
  .gh-suggest {
    margin-top: 6px;
    font-size: var(--fs-xs);
    color: var(--info);
  }
  .gh-result {
    margin-top: 8px;
    font-size: var(--fs-xs);
    padding: 8px 10px;
    border-radius: var(--radius-sm);
    background: var(--bg);
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .gh-result.has {
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent-text);
  }
</style>
