// GitHub connection state and the OAuth device flow, kept outside any component so a sign-in
// started in Settings keeps polling when the user goes to another page.
//   await refreshGithub();                 // githubAuth.status
//   void startDeviceFlow(write, gist);     // githubAuth.flow shows the code while it waits
//   cancelDeviceFlow();                    // stops polling and tells the backend; a late answer is ignored
//   await disconnectGithub();              // cancels any sign-in, then forgets the token
//   await saveGithubPat(pat);              // signs in with a pasted token; returns the login
//   $effect(() => attachGithubView());     // GitHubAuth.svelte while mounted
// One timer at a time: each flow has a generation number, and a poll result (or the start call's
// result) that comes back for an older generation is dropped, so nothing re-arms after a cancel.
// The backend keeps its own order (newest sign-in wins), and a cancel tells it which flow ended.
// Account changes reach it in the order the user asked: a sign-in first registers an attempt
// (quick, no network), and those registrations, cancels and Disconnects are sent one at a time.
// A sign-in's own command (the device start, the token save) goes out in its registration's
// turn, but nothing waits for its answer, so a Disconnect can't overtake a token save clicked
// before it, nor wait for its network check.
// When the flow finishes while no GitHubAuth view is mounted, a toast says how it went.

import {
  ghDeviceCancel,
  ghDevicePoll,
  ghDeviceStart,
  ghLogout,
  ghSetPat,
  ghSignInBegin,
  ghStatus,
  openExternal,
} from "../api";
import type { GhStatus } from "../types";
import { toast } from "../ui/toasts.svelte";
import { inlineError, reportError, success } from "./notify.svelte";
import { setGithubConnected } from "../errorCopy";

export interface DeviceFlowView {
  userCode: string;
  verificationUri: string;
}

interface GithubAuthState {
  status: GhStatus;
  /** The device flow waiting for the user, or null. */
  flow: DeviceFlowView | null;
  /** The last problem to show beside the GitHub controls, or null. */
  message: string | null;
}

/** GitHub polls no faster than this (seconds), whatever the device code says. */
export const MIN_POLL_SECONDS = 3;
/** GitHub's slow_down answer adds this many seconds to the interval. */
export const SLOW_DOWN_SECONDS = 5;

function emptyStatus(): GhStatus {
  return { clientId: null, user: null, builtin: false, canWrite: false, canGist: false };
}

/** Reactive. Change it through the functions below. */
export const githubAuth: GithubAuthState = $state({ status: emptyStatus(), flow: null, message: null });

let timer: ReturnType<typeof setTimeout> | undefined;
/** Bumped by every start and cancel; results for an older generation are ignored. */
let generation = 0;
/** GitHubAuth views mounted right now. */
let views = 0;
/** The device code of the flow on screen, so a cancel can name it to the backend. */
let activeCode: string | null = null;
/** The account change (a registration, a cancel or a Disconnect) still on its way, if any. */
let accountTail: Promise<void> | null = null;

/**
 * Send an account change after the one before it has answered, so the backend sees them in the
 * order the user asked (it can't tell that order itself). Runs at once when nothing is pending.
 * Only quick steps are waited for: registering a sign-in attempt, cancelling a flow, and
 * Disconnect. A sign-in's own command is sent inside its registration's turn and handed back
 * unanswered (wrapped, so the turn doesn't wait for it); the next change goes out right after.
 */
function inOrder<T>(op: () => Promise<T>): Promise<T> {
  const run = accountTail
    ? accountTail.then(op)
    : (() => {
        try {
          return op();
        } catch (e) {
          return Promise.reject(e);
        }
      })();
  const settled = run.then(
    () => {},
    () => {},
  );
  accountTail = settled;
  void settled.then(() => {
    if (accountTail === settled) accountTail = null;
  });
  return run;
}

/**
 * A GitHubAuth view mounted (it shows the flow and its messages itself).
 * @return call when it unmounts
 */
export function attachGithubView(): () => void {
  views++;
  let attached = true;
  return () => {
    if (!attached) return;
    attached = false;
    views--;
  };
}

/** Is a device flow waiting for the user? */
export function deviceFlowActive(): boolean {
  return githubAuth.flow !== null;
}

/** Bumped by every status read; an answer for an older read is dropped. */
let statusRead = 0;

/** Re-read the connection status from the backend. When reads overlap, the last one started wins. */
export async function refreshGithub(): Promise<void> {
  const read = ++statusRead;
  try {
    const status = await ghStatus();
    if (read !== statusRead) return; // a newer read (e.g. after Disconnect) is on its way
    githubAuth.status = status;
    setGithubConnected(!!status.user);
  } catch (e) {
    if (read !== statusRead) return;
    githubAuth.message = inlineError(e, "Checking your GitHub connection");
  }
}

function stopTimer(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = undefined;
}

/**
 * Tell the backend a flow is over, so a poll already on its way there can't store its token.
 * When it says the flow had already finished (the sign-in may have just landed), re-read the
 * status so the screen shows what's really connected.
 */
function cancelOnBackend(code: string, refreshIfOver: boolean): void {
  inOrder(() => ghDeviceCancel(code))
    .then((cancelled) => {
      if (!cancelled && refreshIfOver) return refreshGithub();
    })
    .catch(() => {});
}

/** End the flow on a problem: say it beside the controls, or as a toast when none are on screen. */
function fail(gen: number, message: string): void {
  if (gen !== generation) return;
  stopTimer();
  activeCode = null;
  githubAuth.flow = null;
  githubAuth.message = message;
  if (views === 0 && message) toast({ message, tone: "warn" });
}

function arm(gen: number, deviceCode: string, seconds: number): void {
  if (gen !== generation) return;
  stopTimer(); // never two timers
  timer = setTimeout(() => void poll(gen, deviceCode, seconds), seconds * 1000);
}

async function poll(gen: number, deviceCode: string, seconds: number): Promise<void> {
  timer = undefined;
  if (gen !== generation) return;
  let r;
  try {
    r = await ghDevicePoll(deviceCode);
  } catch (e) {
    // Saving the sign-in can fail halfway, and then the backend signs out: show what's true,
    // even for a flow that was cancelled or replaced meanwhile.
    void refreshGithub();
    if (gen === generation) fail(gen, inlineError(e, "Waiting for GitHub to confirm"));
    return;
  }
  // Cancelled (or restarted) while the poll was out: drop its answer. If it connected anyway
  // (the backend stored it before the cancel or the newer sign-in reached it), show that.
  if (gen !== generation) {
    if (r.status === "ok") void refreshGithub();
    return;
  }
  switch (r.status) {
    case "ok": {
      const done = ++generation; // done: nothing of this flow may run again
      activeCode = null;
      githubAuth.flow = null;
      githubAuth.message = null;
      await refreshGithub();
      if (done !== generation) return; // disconnected (or restarted) meanwhile: no "Connected" toast
      if (views === 0) {
        const who = githubAuth.status.user;
        success(who ? `Connected to GitHub as ${who}` : "Connected to GitHub");
      }
      return;
    }
    case "denied":
      return fail(gen, "You didn't approve Silo on GitHub. Try connecting again when you're ready.");
    case "expired":
      return fail(gen, "The code expired. Try again.");
    case "error":
      // The friendly line is the headline; GitHub's own reason goes to the activity list, where
      // Copy details has it.
      reportError(`GitHub sign-in error: ${r.error ?? "unknown"}`, "Connecting GitHub", { quiet: true });
      return fail(gen, "GitHub sign-in didn't finish. Try connecting again.");
    case "cancelled":
      // A newer sign-in (e.g. a pasted token) ended this flow; that one reports for itself.
      stopTimer();
      activeCode = null;
      githubAuth.flow = null;
      return;
    case "slow_down":
      return arm(gen, deviceCode, seconds + SLOW_DOWN_SECONDS);
    default:
      return arm(gen, deviceCode, seconds);
  }
}

/**
 * Start GitHub's device flow: show the code, open the verification page and poll until the user
 * answers. A flow already running is replaced. Callers request the UNION of held and newly wanted
 * capabilities, so enabling one (e.g. gist) never drops another the token already carries.
 * @param write ask for the public_repo scope (Star / Watch)
 * @param gist ask for the gist scope (sharing Collections)
 */
export async function startDeviceFlow(write = false, gist = false): Promise<void> {
  const gen = ++generation;
  stopTimer();
  activeCode = null; // the backend drops the old flow when this one starts
  githubAuth.flow = null;
  githubAuth.message = null;
  try {
    // Waiting behind another account change and cancelled meanwhile: don't start at all. The
    // start goes out in this sign-in's turn; GitHub's answer is awaited outside it.
    const sent = await inOrder(async () => {
      if (gen !== generation) return null;
      const attempt = await ghSignInBegin();
      if (gen !== generation) return null;
      return { start: ghDeviceStart(write, gist, attempt) };
    });
    if (sent === null) return;
    const dc = await sent.start;
    if (gen !== generation) {
      // Cancelled or restarted meanwhile: make sure the backend forgets this flow too.
      cancelOnBackend(dc.deviceCode, false);
      return;
    }
    activeCode = dc.deviceCode;
    githubAuth.flow = { userCode: dc.userCode, verificationUri: dc.verificationUri };
    openExternal(dc.verificationUri).catch(() => {});
    arm(gen, dc.deviceCode, Math.max(dc.interval, MIN_POLL_SECONDS));
  } catch (e) {
    fail(gen, inlineError(e, "Starting GitHub sign-in"));
    // A flow this one replaced may have connected meanwhile: show what's connected now.
    if (gen === generation) void refreshGithub();
  }
}

/**
 * Stop waiting for the user. A poll already on its way is ignored when it answers, and the
 * backend is told, so that poll can't store its token either.
 */
export function cancelDeviceFlow(): void {
  generation++;
  stopTimer();
  githubAuth.flow = null;
  const code = activeCode;
  activeCode = null;
  if (code !== null) cancelOnBackend(code, true);
}

/**
 * Disconnect the GitHub account. A sign-in still waiting is cancelled first, so a code approved
 * on github.com afterwards can't reconnect the account. A failure (e.g. the credential store kept
 * the token) shows beside the controls, and the status is re-read either way.
 */
export async function disconnectGithub(): Promise<void> {
  cancelDeviceFlow();
  githubAuth.message = null;
  try {
    await inOrder(() => ghLogout());
  } catch (e) {
    githubAuth.message = inlineError(e, "Disconnecting GitHub");
  }
  await refreshGithub();
}

/**
 * Sign in with a pasted personal access token, in order with the other account changes.
 * @return the GitHub login it belongs to
 */
export async function saveGithubPat(pat: string): Promise<string> {
  // The save goes out in this sign-in's turn; GitHub's check of the token is awaited outside it.
  const sent = await inOrder(async () => ({
    save: ghSetPat(pat, await ghSignInBegin()),
  }));
  return sent.save;
}

/** Test helper: back to the initial state with no timer and no views. */
export function resetGithubAuth(): void {
  activeCode = null; // tests: no backend call
  accountTail = null;
  cancelDeviceFlow();
  views = 0;
  githubAuth.status = emptyStatus();
  setGithubConnected(false);
  githubAuth.message = null;
}

/** Test helper: is a poll timer armed? */
export function pollArmed(): boolean {
  return timer !== undefined;
}
