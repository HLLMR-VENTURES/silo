<script lang="ts">
  import Icon from "../ui/Icon.svelte";
  import { displayVersion } from "../format";
  import { inlineError } from "../state/notify.svelte";
  import type { ModEntry, VerifyResult } from "../types";
  import { verifyMod } from "../api";
  import { getFailure, getVerdict, isVerifying, setFailure, setVerdict, setVerifying } from "../provenanceCache.svelte";

  let { mod }: { mod: ModEntry } = $props();

  type Phase = "idle" | "running" | "done" | "error";
  let phase = $state<Phase>("idle");
  let result = $state<VerifyResult | null>(null);
  let err = $state("");

  // The panel follows the file shown, across drawer switches: a check still running for it
  // (started before a switch away and back), its error, its verdict from this session, or the
  // button. The error comes before the verdict: a failed Re-check mustn't show the old verdict
  // as if it had been confirmed (#358).
  $effect(() => {
    const cached = getVerdict(mod);
    const failed = getFailure(mod);
    if (isVerifying(mod)) {
      phase = "running";
      err = "";
    } else if (failed !== undefined) {
      phase = "error";
      err = failed;
    } else if (cached) {
      result = cached;
      phase = "done";
      err = "";
    } else {
      phase = "idle";
      result = null;
      err = "";
    }
  });

  const canVerify = $derived(mod.kind === "zip");

  async function run() {
    // The verdict belongs to the file that was hashed: the drawer may show another mod by the
    // time it lands (this panel is reused across mods). The effect above shows the outcome.
    const m = mod;
    if (isVerifying(m)) return;
    setFailure(m, null);
    setVerifying(m, true);
    try {
      setVerdict(m, await verifyMod(m.techName, m.version, m.path));
    } catch (e) {
      // Kept under the file that was hashed, whichever mod the drawer shows now (#358).
      setFailure(m, inlineError(e, "Checking the mod's integrity"));
    } finally {
      setVerifying(m, false);
    }
  }

  const ver = displayVersion;

  const howLabel = (how: string | null) =>
    how === "exact"
      ? "Exactly the same as the published build."
      : how === "content"
        ? "The same files as the published build, zipped again."
        : "Matches the published build.";

  // Cap each diff list so a wildly-modified mod can't blow out the drawer.
  const CAP = 12;
  function capped(list: string[]): { shown: string[]; more: number } {
    return { shown: list.slice(0, CAP), more: Math.max(0, list.length - CAP) };
  }
</script>

<div class="prov">
  <div class="prov-head">
    <h3 class="prov-title">Integrity</h3>
    {#if canVerify && phase === "idle"}
      <button class="prov-btn" onclick={run}>Verify integrity</button>
    {:else if phase === "running"}
      <span class="prov-run">Checking against the published build…</span>
    {:else if phase === "done" && (result?.status === "modified" || result?.status === "unverified")}
      <button class="prov-btn ghost" onclick={run}>Re-check</button>
    {/if}
  </div>

  {#if !canVerify}
    <p class="prov-note">Only zipped mods can be checked. This one is a folder.</p>
  {:else if phase === "idle"}
    <p class="prov-note">
      Check this mod against its published build in Silo's catalog.
    </p>
  {:else if phase === "error"}
    <p class="prov-err">{err}</p>
    <button class="prov-btn ghost" onclick={run}>Try again</button>
  {:else if phase === "done" && result}
    {@const r = result}
    {#if r.status === "verified"}
      <div class="verdict ok">
        <span class="dot"><Icon name="verifiedBuild" /></span>
        <div>
          <div class="v-label">Verified build</div>
          <div class="v-sub">
            {howLabel(r.how)}{#if r.matchedVersion}&nbsp;({ver(r.matchedVersion)}){/if}
          </div>
        </div>
      </div>
    {:else if r.status === "modified"}
      <div class="verdict warn">
        <span class="dot"><Icon name="modified" /></span>
        <div>
          <div class="v-label">Modified from the published build</div>
          <div class="v-sub">
            {r.changed.length} changed · {r.added.length} added · {r.removed.length} removed{#if r.matchedVersion}
              &nbsp;(compared with {ver(r.matchedVersion)}){/if}
          </div>
        </div>
      </div>
      {#each [{ k: "changed", label: "Changed", list: r.changed }, { k: "added", label: "Added", list: r.added }, { k: "removed", label: "Removed", list: r.removed }] as g (g.k)}
        {#if g.list.length}
          {@const c = capped(g.list)}
          <details class="diff">
            <summary>{g.label} <span class="cnt">{g.list.length}</span></summary>
            <ul>
              {#each c.shown as f (f)}<li class="mono">{f}</li>{/each}
              {#if c.more}<li class="more">+{c.more} more</li>{/if}
            </ul>
          </details>
        {/if}
      {/each}
    {:else}
      <div class="verdict muted">
        <span class="dot">?</span>
        <div>
          <div class="v-label">Unverified</div>
          <div class="v-sub">{r.note ?? "Silo's catalog has nothing to compare it with yet."}</div>
        </div>
      </div>
    {/if}
    <p class="prov-fine">
      This checks where the mod came from and that it's unchanged. It isn't a virus scan and
      can't tell whether a mod is safe.
    </p>
  {/if}
</div>

<style>
  .prov {
    border-top: 1px solid var(--border);
    padding: 12px 0;
  }
  .prov-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    margin-bottom: 6px;
  }
  .prov-title {
    margin: 0;
    font-weight: 700;
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
  }
  .prov-btn {
    border: 1px solid var(--primary);
    background: var(--primary);
    color: var(--on-primary);
    font: inherit;
    font-size: var(--fs-xs);
    font-weight: 600;
    min-height: 24px;
    padding: 4px 11px;
    border-radius: var(--radius-sm);
    cursor: pointer;
  }
  .prov-btn.ghost {
    background: var(--surface);
    color: var(--text);
    border-color: var(--border);
    font-weight: 500;
  }
  .prov-btn:hover {
    filter: brightness(1.05);
  }
  .prov-run {
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .prov-note,
  .prov-fine,
  .prov-err {
    font-size: var(--fs-xs);
    line-height: 1.45;
    color: var(--text-muted);
    margin: 0;
  }
  .prov-err {
    color: var(--danger);
  }
  .prov-fine {
    margin-top: 10px;
    font-size: var(--fs-xs);
    opacity: 0.85;
  }
  .verdict {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    padding: 9px 11px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--border);
  }
  .verdict .dot {
    font-size: 0.95rem;
    line-height: 1.3;
    flex: 0 0 auto;
  }
  .verdict.ok {
    background: color-mix(in srgb, var(--primary) 10%, transparent);
    border-color: color-mix(in srgb, var(--primary) 35%, var(--border));
  }
  .verdict.ok .dot {
    color: var(--primary-text);
  }
  .verdict.warn {
    background: color-mix(in srgb, var(--warn-text) 12%, transparent);
    border-color: color-mix(in srgb, var(--warn-text) 40%, var(--border));
  }
  .verdict.warn .dot {
    color: var(--warn-text);
  }
  .verdict.muted .dot {
    color: var(--text-muted);
  }
  .v-label {
    font-weight: 600;
    font-size: 0.85rem;
    color: var(--text);
  }
  .v-sub {
    font-size: var(--fs-xs);
    color: var(--text-muted);
    line-height: 1.4;
    margin-top: 1px;
  }
  .diff {
    margin-top: 6px;
    font-size: var(--fs-xs);
  }
  .diff summary {
    cursor: pointer;
    color: var(--text);
    padding: 3px 0;
  }
  .diff .cnt {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .diff ul {
    list-style: none;
    margin: 2px 0 4px;
    padding: 0 0 0 6px;
    max-height: 160px;
    overflow-y: auto;
  }
  .diff li {
    padding: 1px 0;
    color: var(--text-muted);
  }
  .diff li.mono {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    word-break: break-all;
  }
  .diff li.more {
    font-style: italic;
    font-size: var(--fs-xs);
  }
</style>
