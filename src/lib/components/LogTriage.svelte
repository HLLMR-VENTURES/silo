<script lang="ts">
  // Diagnose → Last run: read FS25's log.txt and say, in plain terms, whether the last game
  // crashed and which mods are most likely at fault, separating real breakage from cosmetic
  // noise. Each named mod gets actions (Park, Show in library, Copy details). When the log
  // names nothing, "Find the problem mod" runs the guided bisection here.
  import Icon from "../ui/Icon.svelte";
  import Button from "../ui/Button.svelte";
  import { inlineError, success, reportError } from "../state/notify.svelte";
  import { onMount, untrack } from "svelte";
  import { copyText, logStatus, scanLog, revealInFolder, userDirPath } from "../api";
  import type { LogReport, LogModHealth } from "../types";
  import BisectFlow from "./BisectFlow.svelte";
  import {
    countsLine,
    groupLogMods,
    lastRunMeta,
    lastRunVerdict,
    logModDetails,
    logTimeLabel,
  } from "../diagnose/lastRun";
  import type { LibraryLookup } from "../diagnose/crashNotice";
  import { formatCount, plural } from "../plural";
  import { latestOnly } from "../diagnose/latest";
  import { fillTypeCapNote, groundPileCapNote } from "../diagnose/conflictCopy";
  import { describeError, NO_LOG_COPY } from "../errorCopy";

  interface Props {
    /** Current active tech names, and which of them are maps (kept on during bisection). */
    activeMods: string[];
    activeMaps: string[];
    /** Active mods Silo can't park (unzipped folder mods): kept on every round too. */
    activeFixed?: string[];
    /** Before a search starts: file loose zips so they can be parked (false stops it). */
    onBisectPrepare?: () => Promise<boolean>;
    /** Passed through to guided bisection: apply a set, and report start/end. */
    onApply: (techNames: string[]) => Promise<void>;
    onBisecting: (active: boolean) => void;
    /** An interrupted bisection is waiting to be restored; a new one can't start. */
    recoveryPending?: boolean;
    /** The library's title and active state for a log name (tech name), or null. */
    lookup?: LibraryLookup;
    /** Park a library mod (with Undo). */
    onPark?: (techName: string) => void;
    /** Open a library mod's drawer. */
    onShow?: (techName: string) => void;
    /** Guided bisection's round, for the header mini-status. */
    onBisectProgress?: (p: { round: number; about: number; launching: boolean }) => void;
    /** Guided bisection put the mods back (`parked`: the culprit it left parked). */
    onBisectFinished?: (parked: string | null) => void;
    /** Someone asked to find the problem mod (the crash notice): open the flow, then report back. */
    bisectRequested?: boolean;
    onBisectRequestTaken?: () => void;
    /** Changes when Silo read a new log elsewhere (crash detection): re-read it here. */
    refreshKey?: number;
    /** Why library writes are off right now (Park is off with this reason), or "". */
    lockReason?: string;
  }
  let {
    activeMods,
    activeMaps,
    activeFixed = [],
    onBisectPrepare,
    onApply,
    onBisecting,
    recoveryPending = false,
    lookup = () => null,
    onPark,
    onShow,
    onBisectProgress,
    onBisectFinished,
    bisectRequested = false,
    onBisectRequestTaken,
    refreshKey = 0,
    lockReason = "",
  }: Props = $props();

  let mode = $state<"report" | "bisect">("report");
  // Bisect the active non-map mods; maps stay on so the save still loads, and mods Silo can't
  // park (unzipped folders) stay on because set_active can't move them.
  const alwaysOn = $derived([...new Set([...activeMaps, ...activeFixed])]);
  const pool = $derived(activeMods.filter((m) => !alwaysOn.includes(m)));
  const canBisect = $derived(pool.length >= 2);
  const bisectOff = "Turn on at least 2 mods that aren't maps to find the problem mod.";

  let report = $state<LogReport | null>(null);
  let logTime = $state<string | null>(null);
  let loading = $state(true);
  let error = $state<string | null>(null);
  /** The game hasn't written a log yet (never played): an empty state, not an error. */
  let noLog = $state(false);

  // Reads can overlap (a new log read elsewhere while "Read the log again" runs): the newest wins.
  const reads = latestOnly<{ r: LogReport; ms: number | null }>();
  async function scan() {
    loading = true;
    error = null;
    noLog = false;
    try {
      const applied = await reads.run(
        async () => {
          const [r, st] = await Promise.all([scanLog(), logStatus().catch(() => null)]);
          return { r, ms: st?.modifiedMs ?? null };
        },
        ({ r, ms }) => {
          report = r;
          logTime = ms != null ? logTimeLabel(ms, Date.now()) : null;
          loading = false;
        },
      );
      if (!applied) return; // a newer read owns `loading`
    } catch (e) {
      // By code, or by the older backend's text ("No log.txt found…").
      if (describeError(e).code === "NO_LOG") noLog = true;
      else error = inlineError(e, "Reading the game log");
      report = null;
    }
    loading = false;
  }

  const groups = $derived(report ? groupLogMods(report) : { causes: [], worth: [], cosmetic: [] });
  const verdict = $derived(report ? lastRunVerdict(report, groups.causes.length) : null);

  let showCosmetic = $state(false);
  /** Mods whose first log line is shown in full. */
  let expanded = $state<Set<string>>(new Set());
  function toggleLine(name: string) {
    const next = new Set(expanded);
    if (next.has(name)) next.delete(name);
    else next.add(name);
    expanded = next;
  }

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

  async function copyDetails(m: LogModHealth) {
    const title = lookup(m.modName)?.title ?? m.modName;
    try {
      await copyText(logModDetails(m, title));
      success(`Copied the details for ${title}.`);
    } catch (e) {
      reportError(e, "Copying the details");
    }
  }

  function findProblemMod() {
    if (canBisect) mode = "bisect";
  }

  // The crash notice's "Find the problem mod": open the flow once, then tell App it was seen.
  $effect(() => {
    if (!bisectRequested) return;
    untrack(() => {
      if (canBisect && !recoveryPending) mode = "bisect";
      onBisectRequestTaken?.();
    });
  });

  // A new log was read elsewhere: read it here too (not mid-diagnosis, which reads its own).
  let lastKey = untrack(() => refreshKey);
  $effect(() => {
    const k = refreshKey;
    if (k === lastKey) return;
    lastKey = k;
    if (untrack(() => mode) === "report") void scan();
  });

  onMount(scan);
</script>

<div class="lt">
  {#if mode === "bisect"}
    <BisectFlow
      {pool}
      {alwaysOn}
      onPrepare={onBisectPrepare}
      stayOn={activeFixed.filter((m) => activeMods.includes(m) && !activeMaps.includes(m)).length}
      {onApply}
      {onBisecting}
      {recoveryPending}
      onProgress={onBisectProgress}
      onFinished={onBisectFinished}
      titleOf={(t) => lookup(t)?.title ?? t}
      onExit={() => {
        mode = "report";
        void scan();
      }}
    />
  {:else}
    <div class="lt-head">
      <div>
        <h3 class="lt-title">Last run</h3>
        {#if report}
          <p class="sub">{lastRunMeta(report, logTime)}</p>
        {/if}
      </div>
      <Button size="sm" icon="rescan" onclick={scan} disabled={loading}>{loading ? "Reading…" : "Read the log again"}</Button>
    </div>

    <div role="status" aria-live="polite" class="live">
      {#if loading && !report}<div class="empty">Reading log.txt…</div>{/if}
    </div>
    {#if noLog}
      <div class="empty">{NO_LOG_COPY}</div>
    {:else if error}
      <div class="verdict bad" role="alert">{error}</div>
    {:else if report && verdict}
      {@const r = report}
      <div class="verdict {verdict.tone}">
        <Icon name={verdict.tone === "good" ? "check" : "problem"} size={16} />
        <p><strong>{verdict.title}</strong> {verdict.body}</p>
      </div>
      {#if r.groundPileCap}
        {@const note = groundPileCapNote(r.groundPileCap, r.groundPileDisabled)}
        <div class="verdict bad" role="note">
          <Icon name="problem" size={16} />
          <p><strong>{note.title}</strong> {note.body}</p>
        </div>
      {/if}
      {#if r.fillTypeCap && r.droppedFillTypes.length > 0}
        {@const note = fillTypeCapNote(r.fillTypeCap, r.droppedFillTypes)}
        <div class="verdict bad" role="note">
          <Icon name="problem" size={16} />
          <p><strong>{note.title}</strong> {note.body}</p>
        </div>
      {/if}
      {#if r.crashed && groups.causes.length === 0}
        <div class="cta">
          <Button variant="primary" icon="diagnose" onclick={findProblemMod} disabled={!canBisect}>Find the problem mod</Button>
          <p class="cta-why">
            {#if canBisect}
              Silo tests half your mods at a time, about {formatCount(Math.ceil(Math.log2(Math.max(pool.length, 2))))} {plural(Math.ceil(Math.log2(Math.max(pool.length, 2))), { one: "launch", other: "launches" })}.
            {:else}
              {bisectOff}
            {/if}
          </p>
        </div>
      {/if}

      {#if lockReason && (groups.causes.length > 0 || groups.worth.length > 0)}
        <p class="why">{lockReason}</p>
      {/if}
      {#if groups.causes.length > 0}
        <h4 class="sec">{r.crashed ? "Most likely causes" : "Mods with errors"}</h4>
        <ul class="rows">
          {#each groups.causes as m (m.modName)}
            {@render row(m, "bad")}
          {/each}
        </ul>
      {/if}

      {#if groups.worth.length > 0}
        <h4 class="sec">Worth a look</h4>
        <ul class="rows">
          {#each groups.worth as m (m.modName)}
            {@render row(m, "warn")}
          {/each}
        </ul>
      {/if}

      {#if groups.cosmetic.length > 0}
        <button class="disclosure" aria-expanded={showCosmetic} onclick={() => (showCosmetic = !showCosmetic)}>
          <Icon name={showCosmetic ? "chevronDown" : "chevronRight"} size={14} />
          {formatCount(groups.cosmetic.length)}
          {plural(groups.cosmetic.length, { one: "mod", other: "mods" })} with cosmetic warnings only, safe to ignore
        </button>
        {#if showCosmetic}
          <ul class="rows cosmetic">
            {#each groups.cosmetic as m (m.modName)}
              {@render row(m, "muted")}
            {/each}
          </ul>
        {/if}
      {/if}

      {#if r.unattributed > 0}
        <p class="foot">
          {formatCount(r.unattributed)}
          {plural(r.unattributed, { one: "finding isn't", other: "findings aren't" })} tied to a mod (engine or base game).
          <Button variant="link" size="sm" onclick={reveal}>Open log.txt</Button>
        </p>
        {#if revealError}
          <div class="verdict bad" role="alert">{revealError}</div>
        {/if}
      {/if}

      {#if r.mods.length === 0 && r.unattributed === 0}
        <div class="empty">Nothing in the log to report.</div>
      {/if}

      {#if !(r.crashed && groups.causes.length === 0)}
        <p class="foot">
          Not named here?
          <Button variant="link" size="sm" onclick={findProblemMod} disabled={!canBisect}>Find the problem mod</Button>
          {#if !canBisect}<span class="why">{bisectOff}</span>{/if}
        </p>
      {/if}
    {/if}
  {/if}
</div>

{#snippet row(m: LogModHealth, tone: "bad" | "warn" | "muted")}
  {@const lib = lookup(m.modName)}
  {@const title = lib?.title ?? m.modName}
  {@const open = expanded.has(m.modName)}
  <li class="modrow {tone}">
    <div class="modrow-main">
      <div class="names">
        <span class="modname">{title}</span>
        {#if title !== m.modName}<span class="techname">{m.modName}</span>{/if}
      </div>
      <span class="counts tnum">{countsLine(m)}</span>
    </div>
    <button
      class="sample"
      class:open
      aria-expanded={open}
      aria-label={`line ${m.sampleLine}, the log line for ${title}`}
      onclick={() => toggleLine(m.modName)}
    >
      <span class="line-no tnum">line {m.sampleLine}</span>
      <span class="line-text">{m.sample}</span>
    </button>
    {#if tone !== "muted"}
      <div class="row-actions">
        {#if lib?.active && onPark}
          <Button size="sm" disabled={!!lockReason} onclick={() => onPark(m.modName)}>Park</Button>
        {/if}
        {#if lib && onShow}
          <Button size="sm" variant="ghost" onclick={() => onShow(m.modName)}>Show in library</Button>
        {/if}
        <Button size="sm" variant="ghost" icon="copy" onclick={() => copyDetails(m)}>Copy details</Button>
        {#if !lib}<span class="why">Not in your library</span>{/if}
      </div>
    {/if}
  </li>
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
  .empty {
    color: var(--text-muted);
    text-align: center;
    padding: 40px 0;
  }
  .verdict {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    border-radius: var(--radius-sm);
    padding: 11px 14px;
    font-size: 0.9rem;
    line-height: 1.5;
    margin-bottom: 14px;
    border: 1px solid var(--border);
  }
  .verdict p {
    margin: 0;
  }
  .verdict :global(svg) {
    flex: none;
    margin-top: 2px;
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
  .cta {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
    margin: 0 0 14px;
  }
  .cta-why,
  .why {
    margin: 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
  .sec {
    font-size: var(--fs-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin: 14px 0 6px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
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
    opacity: 0.8;
  }
  .modrow-main {
    display: flex;
    align-items: baseline;
    gap: 10px;
    justify-content: space-between;
  }
  .names {
    display: flex;
    align-items: baseline;
    gap: 8px;
    min-width: 0;
    flex-wrap: wrap;
  }
  .modname {
    font-weight: 600;
    color: var(--text);
    font-size: 0.9rem;
  }
  .techname {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .counts {
    flex-shrink: 0;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  .sample {
    display: flex;
    gap: 8px;
    width: 100%;
    margin-top: 5px;
    padding: 4px 6px;
    border: none;
    border-radius: var(--radius-sm);
    background: var(--bg);
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: var(--fs-xs);
    color: var(--text-muted);
    text-align: left;
    cursor: pointer;
    min-height: 1.75rem;
  }
  .line-no {
    flex: none;
  }
  .line-text {
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .sample.open .line-text {
    white-space: pre-wrap;
    word-break: break-word;
  }
  .row-actions {
    display: flex;
    gap: 6px;
    align-items: center;
    flex-wrap: wrap;
    margin-top: 6px;
  }
  .disclosure {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    min-height: 2rem;
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
  .foot {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    margin-top: 14px;
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
</style>
