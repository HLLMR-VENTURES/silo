<script lang="ts">
  // GitHub sign-in controls (Settings → Accounts). A view of the githubAuth store: the device flow
  // and its polling live there, so leaving Settings mid-sign-in doesn't stop it.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { ghSetClientId, ghSetPat, openExternal } from "../api";
  import { GITHUB_NEW_OAUTH_APP_URL, GITHUB_NEW_PAT_URL } from "../links";
  import {
    attachGithubView,
    cancelDeviceFlow,
    disconnectGithub,
    githubAuth,
    refreshGithub,
    startDeviceFlow,
  } from "../state/githubAuth.svelte";

  const status = $derived(githubAuth.status);
  const flow = $derived(githubAuth.flow);
  let clientIdInput = $state("");
  let showAdvanced = $state(false);
  let showPat = $state(false);
  let patInput = $state("");

  async function refresh() {
    await refreshGithub();
    clientIdInput = githubAuth.status.clientId ?? "";
  }
  refresh();

  // While this view is mounted it shows the flow's messages itself (no toast).
  $effect(() => attachGithubView());

  async function saveClientId() {
    try {
      await ghSetClientId(clientIdInput.trim());
      await refresh();
      githubAuth.message = null;
    } catch (e) {
      githubAuth.message = inlineError(e, "Saving the GitHub app ID");
    }
  }

  function connect(write = false, gist = false) {
    void startDeviceFlow(write, gist);
  }

  async function savePat() {
    githubAuth.message = null;
    try {
      await ghSetPat(patInput.trim());
      patInput = "";
      showPat = false;
      await refresh();
    } catch (e) {
      githubAuth.message = inlineError(e, "Saving your GitHub token");
      // A save that fails halfway signs the account out: show what's connected now.
      await refresh();
    }
  }

  async function disconnect() {
    await disconnectGithub();
    clientIdInput = githubAuth.status.clientId ?? "";
  }
</script>

<div class="gha">
  {#snippet flowDisplay()}
    <div class="gha-flow">
      <div class="gha-hint">
        A browser opened to <b>github.com/login/device</b>. Enter this code there:
      </div>
      <div class="gha-code tnum">{flow?.userCode}</div>
      <div class="gha-hint">
        Waiting for you to authorize… You can keep using Silo meanwhile.
      </div>
      <button class="gha-link" onclick={cancelDeviceFlow}>Cancel sign-in</button>
    </div>
  {/snippet}

  {#if status.user}
    <div class="gha-row">
      <div>
        <div class="gha-connected"><Icon name="ok" size={14} /> Connected as {status.user}</div>
        <div class="gha-hint">
          {#if status.canWrite}
            You can star and watch mods on GitHub. Update checks run faster.
          {:else}
            Used for faster update checks.
          {/if}
          {#if status.canGist}
            · Sharing links on.
          {/if}
        </div>
      </div>
      <button class="gha-btn" onclick={disconnect}>Disconnect</button>
    </div>
    {#if flow}
      <!-- Escalating scope (e.g. read-only → actions, or enabling link sharing)
           while already connected: the code MUST show here, since the connected branch
           owns the render when status.user is set. -->
      {@render flowDisplay()}
    {:else}
      {#if !status.canWrite}
        <div class="gha-row">
          <div class="gha-hint">
            Star and watch mods' GitHub pages from Silo. GitHub asks you to allow access to your
            public repositories.
          </div>
          <button class="gha-btn primary" onclick={() => connect(true, status.canGist)}>
            Allow starring and watching
          </button>
        </div>
      {/if}
      {#if !status.canGist}
        <div class="gha-row">
          <div class="gha-hint">
            Share mod sets as links. GitHub asks you to let Silo save each shared list as an
            unlisted link on your GitHub.
          </div>
          <button class="gha-btn primary" onclick={() => connect(status.canWrite, true)}>
            Allow sharing links
          </button>
        </div>
      {/if}
    {/if}
  {:else if flow}
    {@render flowDisplay()}
  {:else if status.clientId}
    <div class="gha-row">
      <div class="gha-hint">
        Connect your GitHub account for faster update checks. Silo only reads your public
        profile. Starring mods and sharing mod sets as links are separate, optional steps.
      </div>
      <button class="gha-btn primary" onclick={() => connect()}>Connect GitHub</button>
    </div>
    {#if !status.builtin}
      <button class="gha-link" aria-expanded={showAdvanced} onclick={() => (showAdvanced = !showAdvanced)}>
        {showAdvanced ? "Hide" : "Change OAuth App…"}
      </button>
      {#if showAdvanced}
        <div class="gha-row">
          <input class="gha-input" aria-label="OAuth App Client ID" placeholder="OAuth App Client ID" bind:value={clientIdInput} />
          <button class="gha-btn" onclick={saveClientId}>Save</button>
        </div>
      {/if}
    {/if}
  {:else}
    <div class="gha-setup">
      <div class="gha-hint">
        GitHub sign-in isn't configured in this build. (One-time: register a GitHub OAuth App with
        Device Flow enabled and add its Client ID.)
        <button class="gha-link" onclick={() => openExternal(GITHUB_NEW_OAUTH_APP_URL)}>
          Register an OAuth App <Icon name="external" size={12} />
        </button>
      </div>
      <div class="gha-row">
        <input class="gha-input" aria-label="OAuth App Client ID" placeholder="OAuth App Client ID (Iv1.…)" bind:value={clientIdInput} />
        <button class="gha-btn" onclick={saveClientId}>Save</button>
      </div>
    </div>
  {/if}

  {#if !status.user && !flow}
    <button class="gha-link" aria-expanded={showPat} onclick={() => (showPat = !showPat)}>
      {showPat ? "Hide token option" : "Use a personal access token instead…"}
    </button>
    {#if showPat}
      <div class="gha-hint">
        A fine-grained token with only <b>Starring</b> (read and write) access is the narrowest
        option. Sharing links need a token that can create gists.
        <button
          class="gha-link"
          onclick={() => openExternal(GITHUB_NEW_PAT_URL)}
        >
          Create one <Icon name="external" size={12} />
        </button>
      </div>
      <div class="gha-row">
        <input
          class="gha-input"
          type="password"
          aria-label="Personal access token"
          placeholder="github_pat_… or ghp_…"
          bind:value={patInput}
        />
        <button class="gha-btn" onclick={savePat} disabled={!patInput.trim()}>Save token</button>
      </div>
    {/if}
  {/if}
  {#if githubAuth.message}<div class="gha-msg">{githubAuth.message}</div>{/if}
</div>

<style>
  .gha {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .gha-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }
  .gha-hint {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.5;
  }
  .gha-connected {
    font-size: var(--fs-sm);
    font-weight: 600;
    color: var(--primary-text);
  }
  .gha-btn {
    flex: 0 0 auto;
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text);
    padding: 7px 14px;
    border-radius: var(--radius-sm);
    font-size: var(--fs-xs);
    font-weight: 600;
  }
  .gha-btn.primary {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }
  .gha-btn:hover:not(.primary) {
    border-color: color-mix(in srgb, var(--primary) 45%, var(--border));
    color: var(--primary-text);
  }
  .gha-link {
    border: none;
    background: transparent;
    color: var(--info);
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 0;
  }
  .gha-link:hover {
    text-decoration: underline;
  }
  .gha-input {
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
  .gha-setup {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .gha-flow {
    display: flex;
    flex-direction: column;
    gap: 8px;
    align-items: center;
    padding: 6px 0;
  }
  .gha-code {
    font-size: var(--fs-2xl);
    font-weight: 700;
    letter-spacing: 4px;
    color: var(--accent-text);
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 8px 16px;
  }
  .gha-msg {
    font-size: var(--fs-xs);
    color: var(--danger);
  }
</style>
