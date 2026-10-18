<script lang="ts">
  // Crash / log triage: read FS25's log.txt and tell the user, in plain terms, whether
  // the last run crashed and which mods are actually at fault — separating real breakage
  // from cosmetic noise. The thing no other FS25 manager does.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { onMount } from "svelte";
  import { scanLog, revealInFolder, userDirPath } from "../api";
  import type { LogReport, LogModHealth } from "../types";
  import BisectFlow from "./BisectFlow.svelte";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    /** Shows a close button (the page has none). */
    onClose?: () => void;
    /** Current active tech names, and which of them are maps (kept on during bisection). */
    activeMods: string[];
    activeMaps: string[];
    /** Passed through to guided bisection: apply a set, and report start/end. */
    onApply: (techNames: string[]) => Promise<void>;
    onBisecting: (active: boolean) => void;
    /** An interrupted bisection is waiting to be restored; a new one can't start. */
    recoveryPending?: boolean;
  }
  let {
    onClose,
    activeMods,
    activeMaps,
    onApply,
    onBisecting,
    recoveryPending = false,
  }: Props = $props();

  let mode = $state<"report" | "bisect">("report");
  // Bisect the active non-map mods; maps stay on so the save still loads.
  const pool = $derived(activeMods.filter((m) => !activeMaps.includes(m)));
  const canBisect = $derived(pool.length >= 2);

  let report = $state<LogReport | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);

  async function scan() {
    loading = true;
    error = null;
    try {
      report = await scanLog();
    } catch (e) {
      error = inlineError(e, "Reading the game log");
      report = null;
    } finally {
      loading = false;
    }
  }

  // A mod is a "culprit" if it has real errors; "noisy" if only non-cosmetic warnings;
  // the rest is cosmetic-only and gets tucked away.
  const culprits = $derived((report?.mods ?? []).filter((m) => m.errors > 0));
  const noisy = $derived(
    (report?.mods ?? []).filter((m) => m.errors === 0 && m.warnings - m.benign > 0),
  );
  const cosmeticOnly = $derived(
    (report?.mods ?? []).filter((m) => m.errors === 0 && m.warnings - m.benign === 0),
  );

  let showCosmetic = $state(false);

  // Kept apart from `error` so a failed "Open log.txt" doesn't replace the report.
  let revealError = $state<string | null>(null);

  async function reveal() {
    revealError = null;
    try {
      const dir = await userDirPath();
      if (dir) await revealInFolder(`${dir}/log.txt`);
    } catch (e) {
      revealError = inlineError(e, "Opening the log folder");
    }
  }

  onMount(scan);
</script>

<div class="lt">
  <div class="lt-head">
    <div>
      <h3 class="lt-title">Crash &amp; log triage</h3>
      {#if report}
        <p class="sub">
          FS25 {report.engineVersion ?? "?"} · {report.modCount.toLocaleString()} mods in the log
        </p>
      {/if}
    </div>
    <div class="lt-head-actions">
      {#if mode === "report"}
        <button class="btn" onclick={scan} disabled={loading}>
          {#if loading}Reading…{:else}<Icon name="rescan" size={14} /> Re-read log{/if}
        </button>
      {/if}
      {#if onClose}
        <button class="x" use:tooltip={"Close"} aria-label="Close" onclick={onClose}><Icon name="close" /></button>
      {/if}
    </div>
  </div>

  {#if mode === "bisect"}
    <BisectFlow
      {pool}
      alwaysOn={activeMaps}
      {onApply}
      {onBisecting}
      {recoveryPending}
      onExit={() => (mode = "report")}
    />
  {:else if loading && !report}
    <div class="empty">Reading log.txt…</div>
  {:else if error}
    <div class="verdict bad">{error}</div>
  {:else if report}
    {@const r = report}
    <!-- Headline verdict -->
    {#if r.crashed}
      <div class="verdict bad">
        <strong>The last run ended abnormally.</strong>
        The log stops mid-session with no clean-exit marker — that's what a crash looks like.
        {#if culprits.length > 0}
          The mod{culprits.length === 1 ? "" : "s"} below logged errors during that run, most errors first.
        {:else}
          No mod logged an error during that run. The cause is likely an interaction the
          log can't name. Guided bisection will find it.
        {/if}
      </div>
      {#if culprits.length === 0}
        <button
          class="bisect-cta"
          disabled={!canBisect}
          onclick={() => (mode = "bisect")}
          title={canBisect ? "" : "Need at least 2 active non-map mods to bisect"}
        >
          <Icon name="diagnose" size={14} /> Run guided bisection ({pool.length} mods)
        </button>
      {/if}
    {:else if culprits.length > 0}
      <div class="verdict warn">
        <strong>Last run exited cleanly</strong>, but {culprits.length} mod{culprits.length === 1
          ? ""
          : "s"} logged errors worth a look.
      </div>
    {:else}
      <div class="verdict good">
        <strong>Healthy.</strong> Last run exited cleanly and no mod logged a real error.
        {#if noisy.length + cosmeticOnly.length > 0}
          A few log cosmetic warnings only.
        {/if}
      </div>
    {/if}

    {#if culprits.length > 0}
      <div class="sec">Likely culprits</div>
      {#each culprits as m (m.modName)}
        {@render row(m, "bad")}
      {/each}
    {/if}

    {#if noisy.length > 0}
      <div class="sec">Worth a look</div>
      {#each noisy as m (m.modName)}
        {@render row(m, "warn")}
      {/each}
    {/if}

    {#if cosmeticOnly.length > 0}
      <button class="disclosure" onclick={() => (showCosmetic = !showCosmetic)}>
        <Icon name={showCosmetic ? "chevronDown" : "chevronRight"} size={14} />
        {cosmeticOnly.length} mod{cosmeticOnly.length === 1 ? "" : "s"} with cosmetic warnings
        only — safe to ignore
      </button>
      {#if showCosmetic}
        <div class="cosmetic">
          {#each cosmeticOnly as m (m.modName)}
            {@render row(m, "muted")}
          {/each}
        </div>
      {/if}
    {/if}

    {#if r.unattributed > 0}
      <p class="foot">
        {r.unattributed} finding{r.unattributed === 1 ? "" : "s"} couldn't be tied to a specific
        mod (engine or base-game). <button class="link" onclick={reveal}>Open log.txt</button>
      </p>
      {#if revealError}
        <div class="verdict bad">{revealError}</div>
      {/if}
    {/if}

    {#if r.mods.length === 0 && r.unattributed === 0}
      <div class="empty">Nothing in the log to report.</div>
    {/if}

    {#if !r.crashed && canBisect}
      <p class="foot">
        Something misbehaving that the log doesn't explain?
        <button class="link" onclick={() => (mode = "bisect")}>Run guided bisection →</button>
      </p>
    {/if}
  {/if}
</div>

{#snippet row(m: LogModHealth, tone: "bad" | "warn" | "muted")}
  <div class="modrow {tone}">
    <div class="modrow-main">
      <span class="modname">{m.modName}</span>
      <span class="counts">
        {#if m.errors > 0}<span class="pill err">{m.errors} error{m.errors === 1 ? "" : "s"}</span>{/if}
        {#if m.warnings - m.benign > 0}
          <span class="pill warnp">{m.warnings - m.benign} warning{m.warnings - m.benign === 1 ? "" : "s"}</span>
        {/if}
        {#if m.benign > 0}<span class="pill benign">{m.benign} cosmetic</span>{/if}
      </span>
    </div>
    <div class="sample" title="log line {m.sampleLine}">{m.sample}</div>
  </div>
{/snippet}

<style>
  .lt {
    padding: 16px 18px 22px;
  }
  .lt-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .lt-head .lt-title {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.35rem;
    color: var(--text);
  }
  .sub {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
    font-variant-numeric: tabular-nums;
  }
  .lt-head-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .btn {
    padding: 7px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--surface-raised);
    color: var(--text);
    font: inherit;
    font-size: 0.85rem;
    cursor: pointer;
  }
  .btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 1rem;
    padding: 4px 6px;
    border-radius: var(--radius-sm);
  }
  .x:hover {
    background: var(--bg);
    color: var(--text);
  }
  .empty {
    color: var(--text-muted);
    text-align: center;
    padding: 40px 0;
  }
  .verdict {
    border-radius: var(--radius-sm);
    padding: 11px 14px;
    font-size: 0.9rem;
    line-height: 1.5;
    margin-bottom: 14px;
    border: 1px solid var(--border);
  }
  .verdict.good {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--primary) 35%, transparent);
  }
  .verdict.warn {
    background: color-mix(in srgb, var(--warn-text) 14%, transparent);
    border-color: color-mix(in srgb, var(--warn-text) 40%, transparent);
  }
  .verdict.bad {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  }
  .sec {
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin: 14px 0 6px;
  }
  .modrow {
    border: 1px solid var(--border);
    border-left: 3px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 8px 11px;
    margin-bottom: 6px;
    background: var(--surface-raised);
  }
  .modrow.bad {
    border-left-color: var(--danger);
  }
  .modrow.warn {
    border-left-color: var(--warn-text);
  }
  .modrow.muted {
    opacity: 0.7;
  }
  .modrow-main {
    display: flex;
    align-items: center;
    gap: 10px;
    justify-content: space-between;
  }
  .modname {
    font-weight: 600;
    color: var(--text);
    font-size: 0.9rem;
  }
  .counts {
    display: flex;
    gap: 5px;
    flex-shrink: 0;
  }
  .pill {
    font-size: var(--fs-xs);
    padding: 1px 7px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .pill.err {
    background: color-mix(in srgb, var(--danger) 18%, transparent);
    color: var(--danger);
    font-weight: 600;
  }
  .pill.warnp {
    background: color-mix(in srgb, var(--warn-text) 18%, transparent);
    color: var(--warn-text);
  }
  .pill.benign {
    background: var(--bg);
    color: var(--text-muted);
  }
  .sample {
    margin-top: 5px;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .disclosure {
    display: block;
    width: 100%;
    text-align: left;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--fs-sm);
    padding: 8px 2px;
    cursor: pointer;
    margin-top: 6px;
  }
  .disclosure:hover {
    color: var(--text);
  }
  .cosmetic {
    padding-left: 4px;
  }
  .foot {
    margin-top: 14px;
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
  .link {
    border: none;
    background: transparent;
    color: var(--info);
    font: inherit;
    cursor: pointer;
    padding: 0;
    text-decoration: underline;
  }
  .bisect-cta {
    display: block;
    width: 100%;
    margin: 0 0 14px;
    padding: 10px;
    border: 1px solid var(--primary);
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--primary-text);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .bisect-cta:hover:not(:disabled) {
    background: var(--primary);
    color: var(--on-primary);
  }
  .bisect-cta:disabled {
    opacity: 0.5;
    cursor: default;
    border-color: var(--border);
    color: var(--text-muted);
    background: var(--bg);
  }
</style>
