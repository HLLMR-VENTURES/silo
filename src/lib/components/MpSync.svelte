<script lang="ts">
  // Multiplayer mod-set sync: export a manifest of your active set to share, or verify
  // your set against a friend's — turning FS's "mod mismatch" join error into a fix-list.
  // (Sharing curated mod sets as a link lives in Collections, a separate panel.)
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { mpExport, mpVerify } from "../api";
  import type { MpModRef, MpVerifyReport } from "../types";
  import { start } from "../state/ops.svelte";

  interface Props {
    /** The active set as (techName, path, kind, version) refs. */
    active: MpModRef[];
    onClose: () => void;
  }
  let { active, onClose }: Props = $props();

  /** What's running (also shown in the activity list), or null. */
  let working = $state<string | null>(null);

  /** Run one hashing job as a tracked op; `working` names it while it runs. */
  async function track(label: string, fn: () => Promise<void>) {
    working = label;
    const op = start(label);
    try {
      await fn();
    } finally {
      op.end();
      working = null;
    }
  }
  let note = $state<string | null>(null);
  let error = $state<string | null>(null);
  let report = $state<MpVerifyReport | null>(null);

  const zipCount = $derived(active.filter((m) => m.kind === "zip").length);
  const dirCount = $derived(active.length - zipCount);

  async function doExport() {
    error = null;
    note = null;
    report = null;
    await track("Hashing and saving your mod set", async () => {
      try {
        const n = await mpExport(active);
        if (n != null) note = `Saved a manifest of ${n} mods. Share the .silomp file with your group.`;
      } catch (e) {
        error = inlineError(e, "Saving the multiplayer manifest");
      }
    });
  }

  async function doVerify() {
    error = null;
    note = null;
    report = null;
    await track("Hashing your mod set and comparing", async () => {
      try {
        report = await mpVerify(active);
      } catch (e) {
        error = inlineError(e, "Comparing mod sets");
      }
    });
  }
</script>

<div class="mp">
  <div class="mp-head">
    <div>
      <h2>Multiplayer mod-set sync</h2>
      <p class="sub">
        Everyone in an FS25 session needs the same mods, same versions, same files —
        that's the #1 "can't join" cause. Match sets here instead of guessing.
      </p>
    </div>
    <button class="x" title="Close" aria-label="Close" onclick={onClose}><Icon name="close" /></button>
  </div>

  {#if working}<div class="busy">{working}…</div>{/if}
  {#if error}<div class="err">{error}</div>{/if}
  {#if note}<div class="ok-note">{note}</div>{/if}

  <div class="cards">
    <div class="card">
      <div class="card-title">Host — share your set</div>
      <p class="card-body">
        Exports a manifest of your {active.length} active mods (name, version, content
        hash). Send the file to whoever's joining.
      </p>
      <button class="btn primary" onclick={doExport} disabled={!!working || active.length === 0}>
        Export active set →
      </button>
    </div>
    <div class="card">
      <div class="card-title">Joiner — check you match</div>
      <p class="card-body">
        Open the host's manifest; Silo compares it to your active set and tells you
        exactly what to add, update, or turn off.
      </p>
      <button class="btn" onclick={doVerify} disabled={!!working || active.length === 0}>
        Verify against a manifest…
      </button>
    </div>
  </div>

  {#if dirCount > 0}
    <p class="caveat">
      {dirCount} active mod{dirCount === 1 ? " is" : "s are"} unpacked folder{dirCount === 1 ? "" : "s"}
      (dev mods) — those can't be byte-verified and are marked unmatched in a manifest.
    </p>
  {/if}

  <p class="caveat">
    Want to share a curated set of mods (not just match a session)? Use
    <b>Collections</b> in the toolbar.
  </p>

  {#if report}
    {@const r = report}
    {#if r.ok}
      <div class="verdict good"><strong>Your set matches.</strong> You're clear to join.</div>
    {:else}
      <div class="verdict bad">
        <strong>Your set doesn't match yet.</strong> Fix the items below, then re-verify.
      </div>

      {#if r.missing.length > 0}
        <div class="sec">Get these ({r.missing.length}) — the host has them, you don't</div>
        {#each r.missing as m (m.techName)}
          <div class="row"><span class="mn">{m.techName}</span><span class="rt">{m.version ?? ""}</span></div>
        {/each}
      {/if}
      {#if r.versionMismatch.length > 0}
        <div class="sec">Wrong version ({r.versionMismatch.length}) — match the host's</div>
        {#each r.versionMismatch as m (m.techName)}
          <div class="row"><span class="mn">{m.techName}</span><span class="rt">yours {m.got || "?"} → need {m.expected || "?"}</span></div>
        {/each}
      {/if}
      {#if r.hashMismatch.length > 0}
        <div class="sec">Same version, different file ({r.hashMismatch.length}) — re-download to be safe</div>
        {#each r.hashMismatch as m (m.techName)}
          <div class="row"><span class="mn">{m.techName}</span><span class="rt">file differs from host</span></div>
        {/each}
      {/if}
      {#if r.extra.length > 0}
        <div class="sec">Turn these off ({r.extra.length}) — the host doesn't have them</div>
        {#each r.extra as name (name)}
          <div class="row"><span class="mn">{name}</span></div>
        {/each}
      {/if}
    {/if}
  {/if}
</div>

<style>
  .mp {
    padding: 16px 18px 22px;
  }
  .mp-head {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .mp-head h2 {
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
    max-width: 52ch;
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
  .cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 12px 14px;
    background: var(--surface-raised);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .card-title {
    font-weight: 600;
    color: var(--text);
  }
  .card-body {
    margin: 0;
    color: var(--text-muted);
    font-size: 0.85rem;
    line-height: 1.5;
    flex: 1;
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
  .verdict {
    margin-top: 14px;
    padding: 10px 13px;
    border-radius: var(--radius-sm);
    font-size: 0.9rem;
    border: 1px solid var(--border);
  }
  .verdict.good {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 35%, transparent);
  }
  .verdict.bad {
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
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
