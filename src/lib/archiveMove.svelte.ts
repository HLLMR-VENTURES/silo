// The library archive move (#166): its reactive state, the status poll, and the drive/settle
// cycle. A move can run for hours: its invoke stays pending the whole time, so the UI is driven
// by `archive_status` (polled while running) and `archive-move-progress` events, not the return.
// App-level side effects (the error banner, mods-folder status, rescans, the adopt offer) are
// injected, so this owns only the move. Pure decisions live in `archiveMoveLogic.ts`.

import {
  archiveStatus,
  archiveMovePause,
  archiveMoveResume,
  archiveMoveCancel,
  confirmDialog,
  onMoveProgress,
  type ArchiveStatus,
  type MoveProgress,
} from "./api";
import { errorCode } from "./errors";
import {
  cancelWasLost,
  isArchiveMissing,
  isMoveCancelling,
  isMoveRunning,
  isMoveUnfinished,
  joinProblems,
  lostCancelText,
  pollShouldSettle,
  settleMayOfferAdopt,
} from "./archiveMoveLogic";

export interface ArchiveMoveDeps {
  /** Show a message in the app's error banner. */
  setError: (msg: string) => void;
  /** Re-read the mods-folder status (keeping the last one if that fails). */
  refreshModsStatus: () => Promise<void>;
  /** Rescan the library (auto-filing allowed). Clears the error banner. */
  runScan: () => Promise<void>;
  /** A move ended cleanly: offer to file in loose mods found in the archive folder. */
  offerAdopt: () => Promise<void>;
}

const isStillLoading = (e: unknown) => errorCode(e) === "STILL_LOADING";

/**
 * One window's view of the archive move. Create it during component initialisation: the
 * constructor registers the poll effect, which then lives and dies with that component.
 */
export class ArchiveMove {
  archive = $state<ArchiveStatus | null>(null);
  /** The startup read gave up — Settings says so and retries once when opened. */
  loadFailed = $state(false);
  progress = $state<MoveProgress | null>(null);
  /** Move invokes (start / resume / cancel / use default) this window is still waiting on. */
  #inFlight = $state(0);
  /** Pause was clicked; the move pauses at the next mod boundary. */
  pauseRequested = $state(false);
  /** Cancel was confirmed; shows "moving back" before the backend reports the turn-around. */
  #cancelInFlight = $state(false);

  /** A move is copying right now — Pause applies, Launch is blocked. */
  readonly running = $derived(isMoveRunning(this.archive, this.#inFlight));
  /** A move is running or paused — library writes are refused until it finishes. */
  readonly unfinished = $derived(isMoveUnfinished(this.archive, this.running));
  readonly cancelling = $derived(
    isMoveCancelling(this.archive, this.#cancelInFlight, this.running, this.progress),
  );
  /** A custom archive whose folder is missing (drive unplugged): nothing can be moved out of
   *  it, so Change… / Use default re-point Silo instead. */
  readonly missing = $derived(isArchiveMissing(this.archive));

  // Lost-cancel detection: Cancel was clicked on a running move, but it may finish first.
  #cancelRequested = false;
  /** Where the move started when Cancel was clicked (the archive path during a move). */
  #cancelHome: string | null = null;
  #cancelHomeDefault = false;
  /** The cancel invoke's own error, held until we know whether the cancel was lost. */
  #cancelProblem: string | null = null;
  #sawTurnAround = false;
  #lastProgressCancelling = false;
  #settling = false;
  /** A settle was asked for while one ran: the running one loops once more with this. */
  #resettle: { problem: string | null } | null = null;
  /** Orders archive_status responses so a stale one can't overwrite newer state. */
  #seq = 0;

  #deps: ArchiveMoveDeps;

  constructor(deps: ArchiveMoveDeps) {
    this.#deps = deps;
    // While a move runs, poll so Pause → paused and the turn-around show even if an event is missed.
    $effect(() => {
      if (!this.running) return;
      const t = setInterval(() => void this.#poll(), 2000);
      return () => clearInterval(t);
    });
  }

  /** Follow `archive-move-progress` events. Resolves to the unlisten function. */
  listen(): Promise<() => void> {
    return onMoveProgress((p) => {
      this.progress = p;
      this.#lastProgressCancelling = p.cancelling;
    });
  }

  /** Re-read the archive status; keeps the last one on error ("still loading" is expected
   *  until the layout resolves at startup). False when it failed or a newer read superseded it. */
  async refresh(): Promise<boolean> {
    const seq = ++this.#seq;
    try {
      const st = await archiveStatus();
      if (seq !== this.#seq) return false;
      this.archive = st;
      this.loadFailed = false;
      if (st.moving?.direction === "back") this.#sawTurnAround = true;
      if (st.moving && !st.running) this.pauseRequested = false;
      return true;
    } catch (e) {
      if (!isStillLoading(e)) console.warn("[Silo] archive_status:", e);
      return false;
    }
  }

  /** Startup: retry while the backend is still resolving the layout. */
  async load(): Promise<void> {
    for (let i = 0; i < 60; i++) {
      const seq = ++this.#seq;
      try {
        const st = await archiveStatus();
        if (seq === this.#seq) this.archive = st;
        this.loadFailed = false;
        return;
      } catch (e) {
        if (!isStillLoading(e)) {
          console.warn("[Silo] archive_status at startup:", e);
          break;
        }
        await new Promise((r) => setTimeout(r, 500));
      }
    }
    if (!this.archive) {
      console.warn("[Silo] gave up reading the library archive status");
      this.loadFailed = true;
    }
  }

  async #poll(): Promise<void> {
    const wasRunning = !!this.archive?.running;
    if (!(await this.refresh())) return;
    // A move this window isn't waiting on (e.g. after a webview reload) stopped: settle it here.
    if (pollShouldSettle(wasRunning, this.archive, this.#inFlight)) await this.#settle(null);
  }

  /** After a move stops (finished, paused, or back home): refresh everything that depends on
   *  where the library lives, explain a lost cancel, and offer to file in loose mods. A settle
   *  asked for while one runs is folded into it (one more pass), never dropped. */
  async #settle(problem: string | null): Promise<void> {
    if (this.#settling) {
      if (problem) this.#deps.setError(problem);
      this.#resettle = { problem: joinProblems(this.#resettle?.problem, problem) };
      return;
    }
    this.#settling = true;
    try {
      // Messages accumulate across passes: each pass's runScan clears the error banner, so
      // every pass re-shows everything said so far.
      let said: { problem: string | null; lost: string | null } = { problem, lost: null };
      for (;;) {
        said = await this.#settleOnce(said.problem, said.lost);
        if (!this.#resettle) break;
        said = { problem: joinProblems(said.problem, this.#resettle.problem), lost: said.lost };
        this.#resettle = null;
      }
    } finally {
      this.#settling = false;
    }
  }

  /** One settle pass. Returns the messages it showed, for the next pass to carry. */
  async #settleOnce(
    problem: string | null,
    lost: string | null,
  ): Promise<{ problem: string | null; lost: string | null }> {
    this.progress = null;
    this.pauseRequested = false;
    this.#cancelInFlight = false;
    await this.refresh();
    await this.#deps.refreshModsStatus();
    if (this.#cancelRequested && !this.archive?.moving && !this.archive?.running) {
      const lostCancel = cancelWasLost({
        lastProgressCancelling: this.#lastProgressCancelling,
        sawTurnAround: this.#sawTurnAround,
        cancelHome: this.#cancelHome,
        archivePath: this.archive?.path,
      });
      if (lostCancel) {
        // Lost: the cancel's own "nothing to cancel" error is noise — drop it.
        lost = this.#lostCancelMessage();
      } else {
        problem = joinProblems(problem, this.#cancelProblem);
      }
      this.#cancelRequested = false;
      this.#cancelHome = null;
      this.#cancelProblem = null;
      this.#sawTurnAround = false;
    }
    await this.#deps.runScan();
    // runScan clears the error banner — show what the move said afterwards.
    const shown = joinProblems(problem, lost);
    if (shown) this.#deps.setError(shown);
    // One adopt offer at a time: `offerAdopt` holds off while a preview is open (a second
    // pass mustn't replace it).
    if (settleMayOfferAdopt(this.archive, problem)) await this.#deps.offerAdopt();
    return { problem, lost };
  }

  #lostCancelMessage(): string {
    return lostCancelText(this.archive?.path, this.#cancelHomeDefault);
  }

  /** Run a move-ish invoke (pending until the move finishes or pauses), then settle. */
  async drive(call: () => Promise<unknown>, isCancel = false): Promise<void> {
    this.#inFlight++;
    let problem: string | null = null;
    try {
      await call();
    } catch (e) {
      // A cancel of a running move can race its finish; hold that error until the settle
      // knows whether the cancel was lost.
      if (isCancel && this.#cancelRequested) this.#cancelProblem = String(e);
      else problem = String(e);
      // A rejected cancel didn't turn anything around.
      if (isCancel) this.#cancelInFlight = false;
    } finally {
      this.#inFlight--;
    }
    if (this.#inFlight > 0) {
      if (problem) this.#deps.setError(problem);
      return;
    }
    const fresh = await this.refresh();
    if (fresh && this.archive?.running) {
      // Not this window's move to settle (e.g. after a webview reload) — the poll will.
      if (problem) this.#deps.setError(problem);
      return;
    }
    await this.#settle(problem);
  }

  /** Fresh move: forget the previous one's progress and cancel bookkeeping. */
  startTracking(): void {
    this.progress = null;
    this.#lastProgressCancelling = false;
    this.pauseRequested = false;
    this.#cancelInFlight = false;
    this.#cancelRequested = false;
    this.#cancelHome = null;
    this.#cancelProblem = null;
    this.#sawTurnAround = false;
  }

  async pause(): Promise<void> {
    this.pauseRequested = true;
    try {
      await archiveMovePause();
    } catch (e) {
      this.pauseRequested = false;
      this.#deps.setError(String(e));
    }
    void this.refresh();
  }

  async resume(): Promise<void> {
    this.startTracking();
    await this.drive(() => archiveMoveResume());
  }

  async cancel(): Promise<void> {
    // Captured before the dialog: the move may finish while it's open.
    const home = this.archive?.path ?? null;
    const homeDefault = !!this.archive?.isDefault;
    if (
      !(await confirmDialog("Cancel the move? Mods already moved go back to where they were.", {
        okLabel: "Cancel move",
        cancelLabel: "Keep going",
      }))
    )
      return;
    if (!this.unfinished) {
      // It finished while the dialog was open — nothing left to cancel.
      if (this.archive?.path && this.archive.path !== home) {
        this.#cancelHomeDefault = homeDefault;
        this.#deps.setError(this.#lostCancelMessage());
      }
      return;
    }
    // Only a running move can finish before it sees the cancel; a paused one is run home here.
    if (this.running) {
      this.#cancelRequested = true;
      this.#cancelHome = home;
      this.#cancelHomeDefault = homeDefault;
    }
    this.#cancelInFlight = true;
    // The backend's cancel replaces a pending pause — don't leave "Pausing…" up for the trip home.
    this.pauseRequested = false;
    // A running move is asked to turn around (resolves at once; the original invoke settles
    // when it lands); a paused one stays pending until it's back home.
    await this.drive(() => archiveMoveCancel(), true);
    void this.refresh();
  }
}
