<script lang="ts">
  import {
    ghStatus,
    ghSetClientId,
    ghDeviceStart,
    ghDevicePoll,
    ghSetPat,
    ghLogout,
    openExternal,
  } from "../api";
  import type { GhStatus } from "../types";

  let status = $state<GhStatus>({
    clientId: null,
    user: null,
    builtin: false,
    canWrite: false,
    canGist: false,
  });
  let clientIdInput = $state("");
  let flow = $state<{ userCode: string; verificationUri: string } | null>(null);
  let message = $state<string | null>(null);
  let showAdvanced = $state(false);
  let showPat = $state(false);
  let patInput = $state("");
  let poller: ReturnType<typeof setTimeout> | undefined;

  async function refresh() {
    try {
      status = await ghStatus();
      clientIdInput = status.clientId ?? "";
    } catch (e) {
      message = String(e);
    }
  }
  refresh();

  async function saveClientId() {
    try {
      await ghSetClientId(clientIdInput.trim());
      await refresh();
      message = null;
    } catch (e) {
      message = String(e);
    }
  }

  // Callers request the UNION of held + newly-wanted capabilities, so enabling one
  // (e.g. gist) never drops another the token already carries.
  async function connect(write = false, gist = false) {
    message = null;
    try {
      const dc = await ghDeviceStart(write, gist);
      flow = { userCode: dc.userCode, verificationUri: dc.verificationUri };
      openExternal(dc.verificationUri).catch(() => {});
      let interval = Math.max(dc.interval, 3);
      const tick = async () => {
        try {
          const r = await ghDevicePoll(dc.deviceCode);
          if (r.status === "ok") {
            flow = null;
            await refresh();
            return;
          }
          if (r.status === "slow_down") interval += 5;
          if (r.status === "expired" || r.status === "denied" || r.status === "error") {
            flow = null;
            message =
              r.status === "denied"
                ? "Authorization was denied."
                : r.status === "expired"
                  ? "The code expired — try again."
                  : `Auth error: ${r.error ?? "unknown"}`;
            return;
          }
          poller = setTimeout(tick, interval * 1000);
        } catch (e) {
          flow = null;
          message = String(e);
        }
      };
      poller = setTimeout(tick, interval * 1000);
    } catch (e) {
      message = String(e);
    }
  }

  async function savePat() {
    message = null;
    try {
      await ghSetPat(patInput.trim());
      patInput = "";
      showPat = false;
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }

  async function disconnect() {
    try {
      await ghLogout();
      await refresh();
    } catch (e) {
      message = String(e);
    }
  }

  $effect(() => () => clearTimeout(poller));
</script>

<div class="gha">
  {#snippet flowDisplay()}
    <div class="gha-flow">
      <div class="gha-hint">
        A browser opened to <b>github.com/login/device</b>. Enter this code there:
      </div>
      <div class="gha-code tnum">{flow?.userCode}</div>
      <div class="gha-hint">Waiting for you to authorize… keep this open.</div>
    </div>
  {/snippet}

  {#if status.user}
    <div class="gha-row">
      <div>
        <div class="gha-connected">✓ Connected as {status.user}</div>
        <div class="gha-hint">
          {#if status.canWrite}
            Star &amp; Watch actions enabled · 5,000 req/hr.
          {:else}
            Read-only (update checks · 5,000 req/hr).
          {/if}
          {#if status.canGist}
            · Collection sharing on.
          {/if}
        </div>
      </div>
      <button class="gha-btn" onclick={disconnect}>Disconnect</button>
    </div>
    {#if flow}
      <!-- Escalating scope (e.g. read-only → actions, or enabling collection sharing)
           while already connected: the code MUST show here, since the connected branch
           owns the render when status.user is set. -->
      {@render flowDisplay()}
    {:else}
      {#if !status.canWrite}
        <div class="gha-row">
          <div class="gha-hint">
            Enable ⭐ Star / 👁 Watch on mod pages — grants the <code>public_repo</code>
            scope so actions land on your GitHub account.
          </div>
          <button class="gha-btn primary" onclick={() => connect(true, status.canGist)}>
            Enable actions
          </button>
        </div>
      {/if}
      {#if !status.canGist}
        <div class="gha-row">
          <div class="gha-hint">
            Enable sharing Collections — grants the <code>gist</code> scope so a shared
            mod set is saved to your own GitHub as a secret (unlisted) gist.
          </div>
          <button class="gha-btn primary" onclick={() => connect(status.canWrite, true)}>
            Enable collection sharing
          </button>
        </div>
      {/if}
    {/if}
  {:else if flow}
    {@render flowDisplay()}
  {:else if status.clientId}
    <div class="gha-row">
      <div class="gha-hint">
        Connect your GitHub account for faster update checks (5,000 requests per hour). This
        grants <code>read:user</code> only. Star / Watch (<code>public_repo</code>) and sharing
        Collections (<code>gist</code>) are separate, optional steps.
      </div>
      <button class="gha-btn primary" onclick={() => connect()}>Connect GitHub</button>
    </div>
    {#if !status.builtin}
      <button class="gha-link" onclick={() => (showAdvanced = !showAdvanced)}>
        {showAdvanced ? "Hide" : "Change OAuth App…"}
      </button>
      {#if showAdvanced}
        <div class="gha-row">
          <input class="gha-input" placeholder="OAuth App Client ID" bind:value={clientIdInput} />
          <button class="gha-btn" onclick={saveClientId}>Save</button>
        </div>
      {/if}
    {/if}
  {:else}
    <div class="gha-setup">
      <div class="gha-hint">
        GitHub sign-in isn't configured in this build. (One-time: register a GitHub OAuth App with
        Device Flow enabled and add its Client ID.)
        <button class="gha-link" onclick={() => openExternal("https://github.com/settings/applications/new")}>
          Register an OAuth App ↗
        </button>
      </div>
      <div class="gha-row">
        <input class="gha-input" placeholder="OAuth App Client ID (Iv1.…)" bind:value={clientIdInput} />
        <button class="gha-btn" onclick={saveClientId}>Save</button>
      </div>
    </div>
  {/if}

  {#if !status.user && !flow}
    <button class="gha-link" onclick={() => (showPat = !showPat)}>
      {showPat ? "Hide token option" : "Use a Personal Access Token instead…"}
    </button>
    {#if showPat}
      <div class="gha-hint">
        A fine-grained PAT scoped to only <b>Starring</b> (read/write) is the most
        minimal-permission path — narrower than the OAuth scope.
        <button
          class="gha-link"
          onclick={() => openExternal("https://github.com/settings/personal-access-tokens/new")}
        >
          Create one ↗
        </button>
      </div>
      <div class="gha-row">
        <input
          class="gha-input"
          type="password"
          placeholder="github_pat_… or ghp_…"
          bind:value={patInput}
        />
        <button class="gha-btn" onclick={savePat} disabled={!patInput.trim()}>Save token</button>
      </div>
    {/if}
  {/if}
  {#if message}<div class="gha-msg">{message}</div>{/if}
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
