<script lang="ts">
  // Guided bisection: the automated "disable half, relaunch, repeat" hunt for the mod
  // (or interaction) that breaks the game when the log can't name it. Silo drives the
  // set changes + launch; the player provides the verdict each round (a crash is
  // auto-detected from the log). The player's real mod set is snapshotted first and
  // restored on finish/cancel — nothing here is destructive.
  import Icon from "../ui/Icon.svelte";
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
  }
  let { pool, alwaysOn, onApply, onBisecting, recoveryPending = false, onExit }: Props =
    $props();

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
      // Snapshot the REAL set first (crash-safe restore lives in the DB).
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
      error = String(e);
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
    }
  }

  async function launch() {
    working = "Launching FS25…";
    try {
      await launchGame();
    } catch (e) {
      error = String(e);
      phase = "error";
    } finally {
      working = null;
    }
  }

  // User is back from the game. Peek the log to pre-answer "did it crash?".
  async function backFromGame() {
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
      error = String(e);
      phase = "error";
    } finally {
      working = null;
    }
  }

  // Always restore the real set before leaving, however we leave. The set comes from the
  // snapshot saved at start, not the live props: a rescan mid-round would have turned
  // `pool` into the test half. The snapshot is cleared only after a clean restore, so if
  // this fails the restore banner can offer it again.
  async function finish() {
    if (working) return;
    if (!started) {
      // Nothing was saved or changed yet (or saving failed): just leave.
      exitUnchanged();
      return;
    }
    working = "Restoring your mods…";
    error = null;
    const r = await finishBisect(
      { get: bisectSnapshotGet, apply: onApply, clear: bisectSnapshotClear },
      alwaysOn,
      pool,
    );
    working = null;
    if (!r.ok) {
      // Stay open with the message (and any culprit still on screen); ✕ or the restore
      // button retries. Closing the panel instead leaves the snapshot for App's banner.
      if (!destroyed) error = r.error;
      return;
    }
    started = false;
    // Called even if the panel closed mid-restore: App re-reads the (now cleared) snapshot.
    onBisecting(false);
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
</script>

<div class="bf">
  <div class="bf-head">
    <h3>Guided bisection</h3>
    <button class="x" title="Cancel & restore" aria-label="Cancel & restore" onclick={finish} disabled={!!working}><Icon name="close" /></button>
  </div>

  {#if working}
    <div class="working">{working}</div>
  {/if}
  {#if error}
    <div class="err">{error}</div>
  {/if}

  {#if phase === "intro"}
    <p class="body">
      Silo will find the culprit by halving your active mods and having you relaunch a few
      times. There are <b>{total}</b> mods to test — about <b
        >{Math.ceil(Math.log2(Math.max(total, 2)))}</b
      > launches. Maps and required mods stay on the whole time, and your exact mod set is
      saved now and restored when you finish.
    </p>
    {#if recoveryPending && !error}
      <div class="err">{BISECT_RESTORE_FIRST}</div>
    {/if}
    <div class="actions">
      <button class="btn ghost" onclick={exitUnchanged} disabled={!!working}>Not now</button>
      <button
        class="btn primary"
        onclick={begin}
        disabled={!!working || recoveryPending}
        title={recoveryPending ? BISECT_RESTORE_FIRST : ""}>Start</button
      >
    </div>
  {:else if phase === "await-launch"}
    {#if step?.kind === "split"}
      <p class="body">
        <span class="round">Round {round}</span>
        {roundsLeft} launch{roundsLeft === 1 ? "" : "es"} left at most. Testing
        <b>{step.test.length}</b> mods this round ({step.rest.length} temporarily off).
      </p>
      <ol class="steps">
        <li>Launch FS25 and load the save that was breaking.</li>
        <li>Try to reproduce the problem, then quit back to desktop.</li>
        <li>Come back here and tell Silo what happened.</li>
      </ol>
    {/if}
    <div class="actions">
      <button class="btn" onclick={launch} disabled={!!working}><Icon name="play" size={14} /> Launch FS25</button>
      <button class="btn primary" onclick={backFromGame} disabled={!!working}>
        I've quit the game →
      </button>
    </div>
  {:else if phase === "verdict"}
    <p class="body">Did the problem still happen this run?</p>
    {#if crashHint === true}
      <div class="hint bad">The log shows FS25 didn't exit cleanly — looks like it crashed again.</div>
    {:else if crashHint === false}
      <div class="hint good">The log shows a clean exit this run.</div>
    {/if}
    <div class="actions">
      <button class="btn danger" onclick={() => verdict(true)} disabled={!!working}>
        Still broken
      </button>
      <button class="btn ok" onclick={() => verdict(false)} disabled={!!working}>
        It's fine now
      </button>
    </div>
  {:else if phase === "culprit"}
    <div class="result bad">
      <div class="result-title">Most likely culprit</div>
      <div class="culprit">{culprit}</div>
      <p class="body">
        The problem tracked to this mod. Disable or update it — or, if it only breaks
        alongside another mod, the pair is worth reporting to its author.
      </p>
    </div>
    <div class="actions">
      <button class="btn primary" onclick={finish} disabled={!!working}>
        Restore my mods & finish
      </button>
    </div>
  {:else if phase === "inconclusive"}
    <div class="result">
      <div class="result-title">No single mod is at fault</div>
      <p class="body">
        Narrowing cleared every half, which means the problem only appears with a
        <b>combination</b> of mods (or isn't mod-related). Bisection can't pin an interaction
        on one mod — that's an honest dead end, not a Silo bug.
      </p>
    </div>
    <div class="actions">
      <button class="btn primary" onclick={finish} disabled={!!working}>
        Restore my mods & finish
      </button>
    </div>
  {:else if phase === "error"}
    <div class="actions">
      <button class="btn primary" onclick={finish} disabled={!!working}>
        Restore my mods & close
      </button>
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
  .round {
    display: inline-block;
    background: var(--primary);
    color: var(--on-primary);
    font-size: var(--fs-xs);
    font-weight: 600;
    padding: 1px 8px;
    border-radius: 999px;
    margin-right: 6px;
    font-variant-numeric: tabular-nums;
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
    font-size: var(--fs-xs);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--text-muted);
    margin-bottom: 4px;
  }
  .culprit {
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 1.05rem;
    font-weight: 600;
    color: var(--text);
    margin-bottom: 8px;
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
