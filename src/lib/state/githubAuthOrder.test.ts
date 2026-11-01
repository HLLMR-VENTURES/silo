// GitHub account changes, seen at `invoke`: sign-in registrations, cancels and Disconnect are
// sent in click order, each after the one before it answered, and Disconnect never waits for a
// sign-in's network work (#403).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-opener", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/plugin-opener")>()),
  openUrl: vi.fn(() => Promise.resolve()),
}));
import { invoke } from "@tauri-apps/api/core";
import {
  cancelDeviceFlow,
  disconnectGithub,
  githubAuth,
  resetGithubAuth,
  startDeviceFlow,
} from "./githubAuth.svelte";

const device = {
  deviceCode: "dev-1",
  userCode: "ABCD-1234",
  verificationUri: "https://github.com/login/device",
  interval: 5,
  expiresIn: 900,
};
const disconnected = {
  clientId: "Iv1.x",
  user: null,
  builtin: true,
  canWrite: false,
  canGist: false,
};

/** Let pending promise callbacks run. */
const flush = () => vi.advanceTimersByTimeAsync(0);

/** Commands in the order `invoke` was called, and a way to hold a command's answer. */
let sent: string[] = [];
let held: Map<string, Promise<unknown>> = new Map();

/** The next call of `cmd` answers only when the returned function is called. */
function hold(cmd: string): () => void {
  let release!: () => void;
  held.set(cmd, new Promise<void>((r) => (release = r)));
  return release;
}

beforeEach(() => {
  vi.useFakeTimers();
  resetGithubAuth();
  sent = [];
  held = new Map();
  let attempts = 0;
  vi.mocked(invoke).mockImplementation(async (cmd: string, args?: unknown) => {
    const a = (args ?? {}) as { attempt?: number; deviceCode?: string };
    sent.push(
      cmd === "gh_device_cancel"
        ? `${cmd}:${a.deviceCode}`
        : a.attempt
          ? `${cmd}#${a.attempt}`
          : cmd,
    );
    const wait = held.get(cmd);
    held.delete(cmd);
    switch (cmd) {
      case "gh_sign_in_begin": {
        const n = ++attempts;
        if (wait) await wait;
        return n;
      }
      case "gh_device_start":
        if (wait) await wait;
        return { ...device, deviceCode: `dev-${a.attempt}` };
      case "gh_device_cancel":
        if (wait) await wait;
        return true;
      case "gh_device_poll":
        return { status: "pending", error: null };
      case "gh_status":
        return disconnected;
      default:
        if (wait) await wait;
        return undefined;
    }
  });
});

afterEach(() => {
  resetGithubAuth();
  vi.clearAllMocks();
  vi.useRealTimers();
});

describe("GitHub account changes at invoke", () => {
  it("Connect then Disconnect, with the registration slower: sent in click order", async () => {
    const release = hold("gh_sign_in_begin");
    const connecting = startDeviceFlow();
    const disconnecting = disconnectGithub();
    await flush();
    expect(sent).toEqual(["gh_sign_in_begin"]);
    release();
    await disconnecting;
    await connecting;
    expect(sent).toEqual(["gh_sign_in_begin", "gh_logout", "gh_status"]);
  });

  it("Cancel then Connect: the cancel is answered before the new registration goes out", async () => {
    await startDeviceFlow();
    const release = hold("gh_device_cancel");
    cancelDeviceFlow();
    const connecting = startDeviceFlow();
    await flush();
    expect(sent).toEqual([
      "gh_sign_in_begin",
      "gh_device_start#1",
      "gh_device_cancel:dev-1",
    ]);
    release();
    await connecting;
    expect(sent.slice(3)).toEqual(["gh_sign_in_begin", "gh_device_start#2"]);
    expect(githubAuth.flow).not.toBeNull();
  });

  it("Disconnect goes out while GitHub is still answering a sign-in's start", async () => {
    hold("gh_device_start"); // never released
    void startDeviceFlow();
    await flush();
    await disconnectGithub();
    expect(sent).toEqual([
      "gh_sign_in_begin",
      "gh_device_start#1",
      "gh_logout",
      "gh_status",
    ]);
  });
});
