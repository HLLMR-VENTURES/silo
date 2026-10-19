// GitHub connection state and the OAuth device flow, kept outside any component so a sign-in
// started in Settings keeps polling when the user goes to another page.
//   await refreshGithub();                 // githubAuth.status
//   void startDeviceFlow(write, gist);     // githubAuth.flow shows the code while it waits
//   cancelDeviceFlow();                    // stops polling; an answer already on its way is ignored
//   $effect(() => attachGithubView());     // GitHubAuth.svelte while mounted
// One timer at a time: each flow has a generation number, and a poll result (or the start call's
// result) that comes back for an older generation is dropped, so nothing re-arms after a cancel.
// When the flow finishes while no GitHubAuth view is mounted, a toast says how it went.

import { ghDevicePoll, ghDeviceStart, ghStatus, openExternal } from "../api";
import type { GhStatus } from "../types";
import { toast } from "../ui/toasts.svelte";
import { inlineError, reportError, success } from "./notify.svelte";

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

/** Re-read the connection status from the backend. */
export async function refreshGithub(): Promise<void> {
  try {
    githubAuth.status = await ghStatus();
  } catch (e) {
    githubAuth.message = inlineError(e, "Checking your GitHub connection");
  }
}

function stopTimer(): void {
  if (timer !== undefined) clearTimeout(timer);
  timer = undefined;
}

/** End the flow on a problem: say it beside the controls, or as a toast when none are on screen. */
function fail(gen: number, message: string): void {
  if (gen !== generation) return;
  stopTimer();
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
    if (gen === generation) fail(gen, inlineError(e, "Waiting for GitHub to confirm"));
    return;
  }
  // Cancelled (or restarted) while the poll was out: drop its answer.
  if (gen !== generation) return;
  switch (r.status) {
    case "ok": {
      generation++; // done: nothing of this flow may run again
      githubAuth.flow = null;
      githubAuth.message = null;
      await refreshGithub();
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
  githubAuth.flow = null;
  githubAuth.message = null;
  try {
    const dc = await ghDeviceStart(write, gist);
    if (gen !== generation) return; // cancelled or restarted meanwhile
    githubAuth.flow = { userCode: dc.userCode, verificationUri: dc.verificationUri };
    openExternal(dc.verificationUri).catch(() => {});
    arm(gen, dc.deviceCode, Math.max(dc.interval, MIN_POLL_SECONDS));
  } catch (e) {
    fail(gen, inlineError(e, "Starting GitHub sign-in"));
  }
}

/** Stop waiting for the user. A poll already on its way is ignored when it answers. */
export function cancelDeviceFlow(): void {
  generation++;
  stopTimer();
  githubAuth.flow = null;
}

/** Test helper: back to the initial state with no timer and no views. */
export function resetGithubAuth(): void {
  cancelDeviceFlow();
  views = 0;
  githubAuth.status = emptyStatus();
  githubAuth.message = null;
}

/** Test helper: is a poll timer armed? */
export function pollArmed(): boolean {
  return timer !== undefined;
}
