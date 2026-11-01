// The GitHub device-flow store: one timer, slow_down backs off, cancel drops an answer already on
// its way, a restart never leaves two timers, and a finish with no view mounted says so in a toast.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  ghStatus: vi.fn(),
  ghDeviceStart: vi.fn(),
  ghDevicePoll: vi.fn(),
  ghDeviceCancel: vi.fn(),
  ghLogout: vi.fn(),
  openExternal: vi.fn(() => Promise.resolve()),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));
import { ghDeviceCancel, ghDevicePoll, ghDeviceStart, ghLogout, ghStatus, openExternal } from "../api";
import type { PollResult } from "../types";
import { dismissToast, toasts } from "../ui/toasts.svelte";
import { activity, clearActivity } from "./activity.svelte";
import {
  attachGithubView,
  cancelDeviceFlow,
  disconnectGithub,
  githubAuth,
  pollArmed,
  resetGithubAuth,
  startDeviceFlow,
} from "./githubAuth.svelte";

const device = { deviceCode: "dev-1", userCode: "ABCD-1234", verificationUri: "https://github.com/login/device", interval: 5, expiresIn: 900 };
const connected = { clientId: "Iv1.x", user: "octocat", builtin: true, canWrite: false, canGist: false };
const answer = (status: PollResult["status"], error: string | null = null): PollResult => ({ status, error });

/** Let pending promise callbacks run (mocked API calls resolve on the microtask queue). */
const flush = () => vi.advanceTimersByTimeAsync(0);

beforeEach(() => {
  vi.useFakeTimers();
  resetGithubAuth();
  vi.mocked(ghDeviceStart).mockResolvedValue(device);
  vi.mocked(ghStatus).mockResolvedValue(connected);
  vi.mocked(ghDeviceCancel).mockResolvedValue(true);
});

afterEach(() => {
  resetGithubAuth();
  for (const t of [...toasts]) dismissToast(t.id);
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("githubAuth device flow", () => {
  it("an error answer shows a plain line; GitHub's reason goes to Copy details", async () => {
    clearActivity();
    const detach = attachGithubView();
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("error", "device_flow_disabled"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.message).toBe("GitHub sign-in didn't finish. Try connecting again.");
    expect(activity.some((a) => a.level === "error" && (a.detail ?? "").includes("device_flow_disabled"))).toBe(true);
    detach();
  });

  it("shows the code, opens the page and polls on the device code's interval", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("pending"));
    await startDeviceFlow();
    expect(githubAuth.flow).toEqual({ userCode: "ABCD-1234", verificationUri: device.verificationUri });
    expect(openExternal).toHaveBeenCalledWith(device.verificationUri);
    expect(pollArmed()).toBe(true);
    await vi.advanceTimersByTimeAsync(4_999);
    expect(ghDevicePoll).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(1);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(5_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(2);
    expect(vi.getTimerCount()).toBe(1);
  });

  it("never polls faster than every 3 seconds, and backs off 5 more on slow_down", async () => {
    vi.mocked(ghDeviceStart).mockResolvedValue({ ...device, interval: 1 });
    vi.mocked(ghDevicePoll).mockResolvedValueOnce(answer("slow_down")).mockResolvedValue(answer("pending"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(3_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(7_999);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(1);
    expect(ghDevicePoll).toHaveBeenCalledTimes(2);
  });

  it("on success refreshes the status and stops; with no view mounted, a toast names the account", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("ok"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.flow).toBeNull();
    expect(githubAuth.status.user).toBe("octocat");
    expect(pollArmed()).toBe(false);
    expect(toasts.map((t) => t.message)).toContain("Connected to GitHub as octocat");
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
  });

  it("no toast when the GitHubAuth view is on screen", async () => {
    const detach = attachGithubView();
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("ok"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.status.user).toBe("octocat");
    expect(toasts).toHaveLength(0);
    detach();
  });

  it("keeps polling after the view unmounts (navigation doesn't stop the sign-in)", async () => {
    const detach = attachGithubView();
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("pending"));
    await startDeviceFlow();
    detach();
    detach(); // idempotent
    await vi.advanceTimersByTimeAsync(10_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(2);
    expect(githubAuth.flow).not.toBeNull();
  });

  it("cancel stops the timer and ignores a poll already in flight", async () => {
    let resolvePoll!: (r: PollResult) => void;
    vi.mocked(ghDevicePoll).mockImplementation(() => new Promise((r) => (resolvePoll = r)));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000); // the poll is out now
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    cancelDeviceFlow();
    expect(githubAuth.flow).toBeNull();
    resolvePoll(answer("pending"));
    await flush();
    expect(pollArmed()).toBe(false); // the late answer didn't re-arm
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
  });

  it("cancel tells the backend which flow ended, so a poll already there can't store", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("pending"));
    await startDeviceFlow();
    cancelDeviceFlow();
    await flush();
    expect(vi.mocked(ghDeviceCancel).mock.calls).toEqual([["dev-1"]]);
    expect(ghStatus).not.toHaveBeenCalled(); // the backend cancelled it: nothing changed
    // Nothing on screen: a second cancel has no flow to name.
    cancelDeviceFlow();
    await flush();
    expect(ghDeviceCancel).toHaveBeenCalledTimes(1);
  });

  it("a cancel that arrives after the sign-in landed re-reads the status", async () => {
    // The poll stored the token on the backend just before the cancel got there.
    let resolvePoll!: (r: PollResult) => void;
    vi.mocked(ghDevicePoll).mockImplementation(() => new Promise((r) => (resolvePoll = r)));
    vi.mocked(ghDeviceCancel).mockResolvedValue(false);
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    cancelDeviceFlow();
    resolvePoll(answer("ok"));
    await flush();
    expect(ghStatus).toHaveBeenCalledTimes(1);
    expect(githubAuth.status.user).toBe("octocat");
    expect(toasts).toHaveLength(0);
  });

  it("a start that answers after a cancel is cancelled on the backend too", async () => {
    let resolveStart!: (d: typeof device) => void;
    vi.mocked(ghDeviceStart).mockImplementation(() => new Promise((r) => (resolveStart = r)));
    const p = startDeviceFlow();
    cancelDeviceFlow();
    expect(ghDeviceCancel).not.toHaveBeenCalled(); // no code yet
    resolveStart(device);
    await p;
    await flush();
    expect(vi.mocked(ghDeviceCancel).mock.calls).toEqual([["dev-1"]]);
    expect(ghStatus).not.toHaveBeenCalled();
  });

  it("a flow the backend ended (a newer sign-in) stops quietly", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("cancelled"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.flow).toBeNull();
    expect(githubAuth.message).toBeNull();
    expect(pollArmed()).toBe(false);
    expect(toasts).toHaveLength(0);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
  });

  it("a sign-in that fails while saving re-reads the status (the backend may have signed out)", async () => {
    const detach = attachGithubView();
    vi.mocked(ghDevicePoll).mockRejectedValue("database is locked");
    vi.mocked(ghStatus).mockResolvedValue({ ...connected, user: null });
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.flow).toBeNull();
    expect(githubAuth.message).toContain("database is locked");
    expect(ghStatus).toHaveBeenCalledTimes(1);
    expect(githubAuth.status.user).toBeNull();
    detach();
  });

  it("a late success after cancel changes nothing", async () => {
    let resolvePoll!: (r: PollResult) => void;
    vi.mocked(ghDevicePoll).mockImplementation(() => new Promise((r) => (resolvePoll = r)));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    cancelDeviceFlow();
    resolvePoll(answer("ok"));
    await flush();
    expect(ghStatus).not.toHaveBeenCalled();
    expect(toasts).toHaveLength(0);
  });

  it("starting again replaces the running flow: one timer, polling the new code", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("pending"));
    await startDeviceFlow();
    vi.mocked(ghDeviceStart).mockResolvedValue({ ...device, deviceCode: "dev-2", userCode: "WXYZ-9876" });
    await startDeviceFlow(true, false);
    expect(vi.getTimerCount()).toBe(1);
    expect(githubAuth.flow?.userCode).toBe("WXYZ-9876");
    await vi.advanceTimersByTimeAsync(5_000);
    expect(vi.mocked(ghDevicePoll).mock.calls).toEqual([["dev-2"]]);
  });

  it("a start cancelled before GitHub answered never shows a code or arms", async () => {
    let resolveStart!: (d: typeof device) => void;
    vi.mocked(ghDeviceStart).mockImplementation(() => new Promise((r) => (resolveStart = r)));
    const p = startDeviceFlow();
    cancelDeviceFlow();
    resolveStart(device);
    await p;
    expect(githubAuth.flow).toBeNull();
    expect(pollArmed()).toBe(false);
  });

  it("denied, expired and errors end the flow with a message (a toast when no view is mounted)", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("denied"));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.flow).toBeNull();
    expect(githubAuth.message).toBe("You didn't approve Silo on GitHub. Try connecting again when you're ready.");
    expect(pollArmed()).toBe(false);
    expect(toasts.map((t) => t.message)).toContain("You didn't approve Silo on GitHub. Try connecting again when you're ready.");

    const detach = attachGithubView();
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("expired"));
    await startDeviceFlow();
    expect(githubAuth.message).toBeNull();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(githubAuth.message).toBe("The code expired. Try again.");
    expect(toasts).toHaveLength(1); // only the first one, shown while no view was mounted
    detach();
  });
});

describe("disconnecting GitHub", () => {
  const disconnected = { ...connected, user: null };

  it("cancels a sign-in still waiting, so a later approval can't reconnect the account", async () => {
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("pending"));
    vi.mocked(ghLogout).mockResolvedValue();
    await startDeviceFlow(true);
    expect(githubAuth.flow).not.toBeNull();
    vi.mocked(ghStatus).mockResolvedValue(disconnected);

    await disconnectGithub();
    expect(githubAuth.flow).toBeNull();
    expect(pollArmed()).toBe(false);
    expect(ghLogout).toHaveBeenCalledTimes(1);
    expect(githubAuth.status.user).toBeNull();

    // The user approves on github.com afterwards: nothing polls, so nothing stores a token.
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("ok"));
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ghDevicePoll).not.toHaveBeenCalled();
    expect(githubAuth.status.user).toBeNull();
  });

  it("a poll already on its way when Disconnect is clicked is ignored", async () => {
    let answerPoll!: (r: PollResult) => void;
    vi.mocked(ghDevicePoll).mockReturnValue(new Promise((r) => (answerPoll = r)));
    vi.mocked(ghLogout).mockResolvedValue();
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    vi.mocked(ghStatus).mockResolvedValue(disconnected);
    await disconnectGithub();
    vi.mocked(ghStatus).mockClear();
    answerPoll(answer("ok"));
    await flush();
    expect(ghStatus).not.toHaveBeenCalled();
    expect(githubAuth.status.user).toBeNull();
  });

  it("a status read that started before Disconnect can't bring the account back, nor toast", async () => {
    // A poll succeeds and starts reading the status; that read is slow. Disconnect meanwhile.
    vi.mocked(ghDevicePoll).mockResolvedValue(answer("ok"));
    vi.mocked(ghLogout).mockResolvedValue();
    let answerStatus!: (s: typeof connected) => void;
    vi.mocked(ghStatus).mockReturnValueOnce(new Promise((r) => (answerStatus = r)));
    await startDeviceFlow();
    await vi.advanceTimersByTimeAsync(5_000);
    expect(ghStatus).toHaveBeenCalledTimes(1);
    vi.mocked(ghStatus).mockResolvedValue(disconnected);
    await disconnectGithub();
    expect(githubAuth.status.user).toBeNull();
    answerStatus(connected); // the old read answers last
    await flush();
    expect(githubAuth.status.user).toBeNull();
    expect(toasts.some((t) => /Connected to GitHub/.test(t.message))).toBe(false);
  });

  it("shows the backend's reason when the credential store kept the token", async () => {
    const reason =
      "Silo couldn't remove the token from your system's credential store. Silo won't use it, and removes it once the store allows (for example after you unlock it). You can also delete the com.hllmr.silo entry there yourself.";
    vi.mocked(ghLogout).mockRejectedValue(reason);
    vi.mocked(ghStatus).mockResolvedValue(disconnected);
    await disconnectGithub();
    expect(githubAuth.message).toContain("couldn't remove the token from your system's credential store");
    expect(githubAuth.status.user).toBeNull();
  });
});
