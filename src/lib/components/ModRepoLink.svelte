<script lang="ts">
  import type { ModEntry, UpdateInfo } from "../types";
  import { setModRepo, guessRepo, checkModUpdate, downloadUpdate, openExternal, confirmDialog } from "../api";

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
      });
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
      ghError = String(e);
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
      ghError = String(e);
    }
    checking = false;
  }

  async function installUpdate() {
    if (!update?.release.assetUrl) return;
    if (!(await confirmDialog(`Download and install ${update.release.tag}? The current version is kept in your mods folder's backups.`)))
      return;
    installing = true;
    ghError = null;
    installNote = null;
    const path = mod.path;
    try {
      const out = await downloadUpdate(path, update.release.assetUrl);
      installNote = {
        path,
        backup: out.backup ? `The previous version is kept in ${out.backup}.` : null,
        warning: out.problems.length ? out.problems.join(" ") : null,
      };
      onInstalled();
    } catch (e) {
      ghError = String(e);
    }
    installing = false;
  }

  // What the last install said. Kept across the rescan `onInstalled` triggers (which hands this
  // panel a fresh `mod`), and shown only while the same mod is open.
  let installNote = $state<{ path: string; backup: string | null; warning: string | null } | null>(
    null,
  );
  const note = $derived(installNote?.path === mod.path ? installNote : null);
</script>

<div class="d-section">
  <div class="d-label">GitHub updates</div>
  <div class="gh-link">
    <input
      class="gh-input"
      placeholder="owner/repo"
      bind:value={repoInput}
      onkeydown={(e) => e.key === "Enter" && linkRepo()}
    />
    <button class="gh-btn" onclick={linkRepo}>{repo ? "Update link" : "Link"}</button>
  </div>
  {#if suggested && !repo}<div class="gh-suggest">Suggested from the mod's page — click Link to confirm.</div>{/if}
  {#if repo}
    <div class="gh-actions">
      <button class="gh-btn" onclick={checkUpdate} disabled={checking}>
        {checking ? "Checking…" : "Check for update"}
      </button>
      <button class="gh-open" onclick={() => openExternal(`https://github.com/${repo.owner}/${repo.repo}`)}>
        Open repo ↗
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
        Up to date — latest is {update.release.tag}
      {/if}
      {#if update.release.htmlUrl}
        <button class="gh-open" onclick={() => openExternal(update!.release.htmlUrl!)}>
          View release ↗
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
    padding: 12px 0;
    border-top: 1px solid var(--border);
  }
  .d-label {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    font-weight: 700;
    margin-bottom: 8px;
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
    font-size: 12.5px;
    font-family: inherit;
  }
  .gh-btn {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 7px 12px;
    border-radius: var(--radius-sm);
    font-size: 12.5px;
    font-weight: 600;
  }
  .gh-btn:hover:not(:disabled) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary);
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
    font-size: 12px;
    font-weight: 600;
  }
  .gh-open:hover {
    text-decoration: underline;
  }
  .gh-err {
    margin-top: 8px;
    color: var(--danger);
    font-size: 12px;
  }
  .gh-suggest {
    margin-top: 6px;
    font-size: 11.5px;
    color: var(--info);
  }
  .gh-result {
    margin-top: 8px;
    font-size: 12.5px;
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
    color: var(--gold-700);
  }
</style>
