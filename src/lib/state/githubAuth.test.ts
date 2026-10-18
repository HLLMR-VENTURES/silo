// The GitHub device-flow store: one timer, slow_down backs off, cancel drops an answer already on
// its way, a restart never leaves two timers, and a finish with no view mounted says so in a toast.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  ghStatus: vi.fn(),
  ghDeviceStart: vi.fn(),
  ghDevicePoll: vi.fn(),
  openExternal: vi.fn(() => Promise.resolve()),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));
import { ghDevicePoll, ghDeviceStart, ghStatus, openExternal } from "../api";
import type { PollResult } from "../types";
import { dismissToast, toasts } from "../ui/toasts.svelte";
import {
  attachGithubView,
  cancelDeviceFlow,
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
});

afterEach(() => {
  resetGithubAuth();
  for (const t of [...toasts]) dismissToast(t.id);
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("githubAuth device flow", () => {
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
    expect(githubAuth.message).toBe("Authorization was denied.");
    expect(pollArmed()).toBe(false);
    expect(toasts.map((t) => t.message)).toContain("Authorization was denied.");

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
