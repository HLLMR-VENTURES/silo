<script lang="ts">
  // Guided bisection: the automated "disable half, relaunch, repeat" hunt for the mod
  // (or interaction) that breaks the game when the log can't name it. Silo drives the
  // set changes + launch; the player provides the verdict each round (a crash is
  // auto-detected from the log). The player's real mod set is snapshotted first and
  // restored on finish/cancel — nothing here is destructive.
  import Icon from "../ui/Icon.svelte";
  import { inlineError } from "../state/notify.svelte";
  import { onDestroy } from "svelte";
  import {
    bisectPlan,
    bisectNarrow,
    bisectSnapshotSave,
    bisectSnapshotGet,
    bisectSnapshotClear,
    launchGame,
    scanLog,
  } from "../api";
  import { BISECT_RESTORE_FIRST, canStartBisect, finishBisect } from "../bisect";
  import type { BisectStep } from "../types";
  import { tooltip } from "../ui/tooltip";

  interface Props {
    /** Suspects to bisect (active mods minus always-on). */
    pool: string[];
    /** Mods kept active every round (maps/required deps) so the save still loads. */
    alwaysOn: string[];
    /** Make exactly these mods active. Shows any problems to the user and rejects if the
     *  set couldn't be applied cleanly. */
    onApply: (techNames: string[]) => Promise<void>;
    /** Told when a bisection starts (snapshot saved) and when it ends. */
    onBisecting: (active: boolean) => void;
    /** An interrupted run's snapshot is waiting in App's restore banner; don't start. */
    recoveryPending?: boolean;
    onExit: () => void;
    /** Told each round ("Round 2 of about 8") for the header's mini-status; `launching` is true
     *  while a test round waits to be played (until "I've quit the game"). */
    onProgress?: (p: { round: number; about: number; launching: boolean }) => void;
    /** Told after the mods were put back: `parked` is the culprit left parked, if any. */
    onFinished?: (parked: string | null) => void;
    /** A mod's title for display (tech name when unknown). */
    titleOf?: (techName: string) => string;
    /** Before anything is saved: get the pool ready to be parked (file loose zips into the
     *  library; set_active only parks filed mods). False stops the start with nothing changed. */
    onPrepare?: () => Promise<boolean>;
    /** Active mods Silo can't park (unzipped folder mods), kept on every round. */
    stayOn?: number;
  }
  let {
    pool,
    alwaysOn,
    onApply,
    onBisecting,
    recoveryPending = false,
    onExit,
    onProgress,
    onFinished,
    titleOf = (t: string) => t,
    onPrepare,
    stayOn = 0,
  }: Props = $props();

  type Phase =
    | "intro"
    | "await-launch" // set applied; waiting for the user to launch, play, quit
    | "verdict" // back from the game; asking still-broken?
    | "culprit"
    | "inconclusive"
    | "error";

  let phase = $state<Phase>("intro");
  let working = $state<string | null>(null); // in-flight action label
  let error = $state<string | null>(null);

  let current = $state<string[]>([]); // working pool, set once bisection begins
  let step = $state<BisectStep | null>(null);
  let round = $state(0);
  let crashHint = $state<boolean | null>(null); // from the log after a round
  let culprit = $state<string | null>(null);

  let started = $state(false); // true once the snapshot is saved; until then nothing changed
  // Set when the panel closes under us. Every await re-checks it so an unmounted flow never
  // applies a set or reports itself as running.
  let destroyed = false;

  const total = $derived(pool.length);

  // Unmounted before the run really started: make sure App doesn't think we're running.
  function abandon() {
    if (!started) onBisecting(false);
  }

  async function begin() {
    if (recoveryPending) {
      error = BISECT_RESTORE_FIRST;
      return;
    }
    working = "Saving your current mod set…";
    error = null;
    try {
      // Never overwrite an interrupted run's snapshot: it's the only record of the real set.
      const prior = await bisectSnapshotGet();
      if (destroyed) return abandon();
      if (!canStartBisect(prior)) {
        error = BISECT_RESTORE_FIRST;
        onBisecting(false); // App re-reads the snapshot and shows the restore banner
        return;
      }
      if (onPrepare) {
        working = null; // its dialog speaks for itself
        const ready = await onPrepare();
        if (destroyed) return abandon();
        if (!ready) return; // declined or refused (said so): nothing was saved
        working = "Saving your current mod set…";
      }
      // Snapshot the REAL set first (crash-safe restore lives in the DB). Read after the
      // preparation, so the props are the library as it is now.
      await bisectSnapshotSave([...alwaysOn, ...pool]);
      if (destroyed) {
        // Nothing was changed yet, so the fresh snapshot isn't needed.
        await bisectSnapshotClear().catch(() => {});
        return abandon();
      }
      started = true;
      onBisecting(true); // no await since the check above, so we're still mounted
      await advance([...pool]);
    } catch (e) {
      if (destroyed) return abandon();
      error = inlineError(e, "Starting the diagnosis");
      phase = "error";
    } finally {
      working = null;
    }
  }

  // Plan the next split for `next` and, if it's a split, project it and wait for a run.
  async function advance(next: string[]) {
    current = next;
    const s = await bisectPlan(next);
    if (destroyed) return;
    step = s;
    if (s.kind === "culprit") {
      culprit = s.modName;
      phase = "culprit";
    } else if (s.kind === "inconclusive") {
      phase = "inconclusive";
    } else {
      working = `Applying ${s.test.length} of ${next.length} mods…`;
      await onApply([...alwaysOn, ...s.test]);
      if (destroyed) return;
      working = null;
      round += 1;
      crashHint = null;
      phase = "await-launch";
      onProgress?.({ round, about: round - 1 + s.roundsLeft, launching: true });
    }
  }

  async function launch() {
    working = "Launching FS25…";
    try {
      await launchGame();
    } catch (e) {
      error = inlineError(e, "Launching the game");
      phase = "error";
    } finally {
      working = null;
    }
  }

  // User is back from the game. Peek the log to pre-answer "did it crash?".
  async function backFromGame() {
    onProgress?.({ round, about, launching: false });
    working = "Reading the log…";
    try {
      const r = await scanLog();
      crashHint = r.crashed;
    } catch {
      crashHint = null; // best-effort hint only
    } finally {
      working = null;
      phase = "verdict";
    }
  }

  async function verdict(stillBroken: boolean) {
    if (step?.kind !== "split") return;
    working = "Narrowing…";
    try {
      const next = await bisectNarrow(step.test, step.rest, stillBroken);
      if (destroyed) return;
      await advance(next);
    } catch (e) {
      error = inlineError(e, "Narrowing down the suspects");
      phase = "error";
    } finally {
      working = null;
    }
  }

  // Always restore the real set before leaving, however we leave. The set comes from the
  // snapshot saved at start, not the live props: a rescan mid-round would have turned
  // `pool` into the test half. The snapshot is cleared only after a clean restore, so if
  // this fails the restore banner can offer it again.
  // `park`: leave this mod (the culprit) out of the restored set.
  async function finish(park: string | null = null) {
    if (working) return;
    if (!started) {
      // Nothing was saved or changed yet (or saving failed): just leave.
      exitUnchanged();
      return;
    }
    working = park ? `Putting your mods back without ${titleOf(park)}…` : "Putting your mods back…";
    error = null;
    const r = await finishBisect(
      { get: bisectSnapshotGet, apply: onApply, clear: bisectSnapshotClear },
      alwaysOn,
      pool,
      park,
    );
    working = null;
    if (!r.ok) {
      // Stay open with the message (and any culprit still on screen); ✕ or the restore
      // button retries. Closing the panel instead leaves the snapshot for App's banner.
      if (!destroyed) error = inlineError(r.error, "Restoring your mods");
      return;
    }
    started = false;
    // Called even if the panel closed mid-restore: App re-reads the (now cleared) snapshot.
    onBisecting(false);
    onFinished?.(park);
    if (!destroyed) onExit();
  }

  function exitUnchanged() {
    if (working) return;
    onBisecting(false);
    onExit();
  }

  // Closing the panel mid-run unmounts this flow without finishing. The snapshot stays
  // (App then offers to restore it); just make sure App stops treating us as running.
  onDestroy(() => {
    destroyed = true;
    if (started) onBisecting(false);
  });

  const roundsLeft = $derived(step?.kind === "split" ? step.roundsLeft : 0);
  /** "Round 2 of about 8": rounds so far plus the most still needed. */
  const about = $derived(round - 1 + roundsLeft);
  const culpritTitle = $derived(culprit ? titleOf(culprit) : "");
  const cancelLabel = $derived(started ? "Stop and put my mods back" : "Close");
</script>

<div class="bf">
  <div class="bf-head">
    <div>
      <h3>Find the problem mod</h3>
      {#if phase === "await-launch" || phase === "verdict"}
        <p class="stepper tnum">Round {round} of about {about}</p>
      {/if}
    </div>
    <button class="x" use:tooltip={cancelLabel} aria-label={cancelLabel} onclick={() => finish()} disabled={!!working}><Icon name="close" /></button>
  </div>

  <div role="status" aria-live="polite">
    {#if working}<div class="working">{working}</div>{/if}
  </div>
  {#if error}
    <div class="err" role="alert">{error}</div>
  {/if}

  {#if phase === "intro"}
    <p class="body">
      Silo finds the problem mod by parking half of your active mods, having you play, and
      halving again. There are <b>{total}</b> mods to test, about <b
        >{Math.ceil(Math.log2(Math.max(total, 2)))}</b
      > launches. Maps and required mods stay on the whole time. Silo saves your exact mods now and
      puts them back when you finish.
    </p>
    {#if stayOn > 0}
      <p class="body muted">
        {stayOn === 1 ? "1 unzipped folder mod stays" : `${stayOn} unzipped folder mods stay`} on every round:
        Silo can only park mod zips.
      </p>
    {/if}
    {#if recoveryPending && !error}
      <div class="err">{BISECT_RESTORE_FIRST}</div>
    {/if}
    <div class="actions">
      <button class="btn ghost" onclick={exitUnchanged} disabled={!!working}>Not now</button>
      <button class="btn primary" onclick={begin} disabled={!!working || recoveryPending}>Start</button>
    </div>
  {:else if phase === "await-launch"}
    {#if step?.kind === "split"}
      <p class="body">
        Testing <b>{step.test.length}</b> {step.test.length === 1 ? "mod" : "mods"} this round ({step.rest.length}
        parked for now).
      </p>
      <ol class="steps">
        <li>Launch the game and load the save that was crashing.</li>
        <li>Try to make it crash, then quit to the desktop.</li>
        <li>Come back and tell Silo what happened.</li>
      </ol>
    {/if}
    <div class="actions">
      <button class="btn" onclick={launch} disabled={!!working}><Icon name="play" size={14} /> Launch test round</button>
      <button class="btn primary" onclick={backFromGame} disabled={!!working}>I've quit the game</button>
    </div>
  {:else if phase === "verdict"}
    <p class="body">Did the problem happen this round?</p>
    {#if crashHint === true}
      <div class="hint bad">The log shows the game didn't close cleanly. It looks like it crashed again.</div>
    {:else if crashHint === false}
      <div class="hint good">The log shows the game closed normally this round.</div>
    {/if}
    <div class="actions">
      <button class="btn danger" onclick={() => verdict(true)} disabled={!!working}>It still happened</button>
      <button class="btn ok" onclick={() => verdict(false)} disabled={!!working}>It's fine now</button>
    </div>
  {:else if phase === "culprit" && culprit}
    <div class="result bad">
      <div class="result-title">Most likely: <span class="culprit">{culpritTitle}</span></div>
      <p class="body">The crash followed this mod in every round.</p>
      <p class="body muted">If it only breaks together with another mod, the pair is worth reporting to its author.</p>
    </div>
    <div class="actions">
      <button class="btn" onclick={() => finish()} disabled={!!working}>Put my mods back</button>
      <button class="btn primary" onclick={() => finish(culprit)} disabled={!!working}>
        Put my mods back and park {culpritTitle}
      </button>
    </div>
  {:else if phase === "inconclusive"}
    <div class="result">
      <div class="result-title">No single mod is at fault.</div>
      <p class="body">
        The problem only shows up when certain mods are on together, or it isn't caused by a mod.
      </p>
    </div>
    <div class="actions">
      <button class="btn primary" onclick={() => finish()} disabled={!!working}>Put my mods back</button>
    </div>
  {:else if phase === "error"}
    <div class="actions">
      <button class="btn primary" onclick={() => finish()} disabled={!!working}>Put my mods back</button>
    </div>
  {/if}
</div>

<style>
  .bf {
    padding: 4px 2px;
  }
  .bf-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 10px;
  }
  .bf-head h3 {
    margin: 0;
    font-family: var(--font-display);
    font-size: 1.1rem;
    color: var(--text);
  }
  .x {
    border: none;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 1rem;
  }
  .x:hover {
    color: var(--text);
  }
  .working {
    color: var(--text-muted);
    font-size: var(--fs-sm);
    margin-bottom: 8px;
  }
  .err {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
    border: 1px solid color-mix(in srgb, var(--danger) 35%, transparent);
    border-radius: var(--radius-sm);
    padding: 8px 11px;
    margin-bottom: 10px;
    font-size: 0.85rem;
  }
  .body {
    color: var(--text);
    font-size: 0.9rem;
    line-height: 1.55;
    margin: 0 0 12px;
  }
  .steps {
    margin: 0 0 14px;
    padding-left: 20px;
    color: var(--text);
    font-size: 0.88rem;
    line-height: 1.6;
  }
  .hint {
    font-size: var(--fs-sm);
    padding: 7px 11px;
    border-radius: var(--radius-sm);
    margin-bottom: 12px;
  }
  .hint.bad {
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    color: var(--danger);
  }
  .hint.good {
    background: color-mix(in srgb, var(--primary) 12%, transparent);
    color: var(--primary-text);
  }
  .result {
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 12px 14px;
    margin-bottom: 14px;
  }
  .result.bad {
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
    background: color-mix(in srgb, var(--danger) 8%, transparent);
  }
  .result-title {
    font-family: var(--font-display);
    font-size: 1.05rem;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 6px;
  }
  .culprit {
    color: var(--text);
  }
  .stepper {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
  .body.muted {
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }
  .actions {
    display: flex;
    gap: 8px;
    justify-content: flex-end;
    flex-wrap: wrap;
  }
  .btn {
    padding: 8px 14px;
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
  .btn.primary {
    background: var(--primary);
    color: var(--on-primary);
    border-color: transparent;
    font-weight: 600;
  }
  .btn.danger {
    background: color-mix(in srgb, var(--danger) 16%, transparent);
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
    font-weight: 600;
  }
  .btn.ok {
    background: color-mix(in srgb, var(--primary) 16%, transparent);
    color: var(--primary-text);
    border-color: color-mix(in srgb, var(--primary) 35%, transparent);
    font-weight: 600;
  }
</style>
