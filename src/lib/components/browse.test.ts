// @vitest-environment jsdom
// Browse (#246): the catalog's offline and empty states, skeleton cards while loading, cards with
// spelled-out sources and capped badges, the install-progress listener dropped on unmount (even
// when it resolves after), and GitHub sign-in polling that outlives the Settings view safely.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import { tick } from "svelte";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  browseMods: vi.fn(),
  siloapiStats: vi.fn(() => Promise.resolve({ mods: 3, sources: 4 })),
  siloapiCategories: vi.fn(() => Promise.resolve([])),
  siloapiFacets: vi.fn(() => Promise.resolve({ facets: {} })),
  siloapiModDetail: vi.fn(),
  installRemoteMod: vi.fn(),
  onInstallProgress: vi.fn(),
  openExternal: vi.fn(() => Promise.resolve()),
  ghStatus: vi.fn(() =>
    Promise.resolve({ clientId: null, user: null, builtin: true, canWrite: false, canGist: false }),
  ),
  ghSetClientId: vi.fn(),
  ghSetPat: vi.fn(),
  ghLogout: vi.fn(),
  ghDeviceStart: vi.fn(),
  ghDevicePoll: vi.fn(),
  catalogImage: vi.fn(() => Promise.resolve("")),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));

import {
  browseMods,
  ghDevicePoll,
  ghDeviceStart,
  onInstallProgress,
} from "../api";
import { AppError } from "../errors";
import type { BrowseMod, ModSourceOption } from "../types";
import ModBrowser from "./ModBrowser.svelte";
import GitHubAuth from "./GitHubAuth.svelte";
import { pollArmed, resetGithubAuth, startDeviceFlow } from "../state/githubAuth.svelte";
import { dismissToast, toasts } from "../ui/toasts.svelte";

beforeAll(() => {
  // jsdom has no ResizeObserver; Svelte's bind:clientWidth/Height needs one.
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});

const gh: ModSourceOption = {
  source: "github",
  sourceUrl: "https://github.com/o/r",
  version: "1.0.6.0",
  installable: true,
  downloadUrl: "https://example.invalid/FS25_Lizard.zip",
};
const mh: ModSourceOption = {
  source: "modhub",
  sourceUrl: "https://www.farming-simulator.com/mod.php?mod_id=1",
  version: "1.0.5.0",
  installable: false,
  downloadUrl: null,
};

function mod(id: string, over: Partial<BrowseMod> = {}): BrowseMod {
  return {
    id,
    techName: `FS25_${id}`,
    title: `Mod ${id}`,
    author: "Someone",
    category: null,
    description: null,
    imageUrl: null,
    latestVersion: "1.0.0.0",
    trustScore: null,
    updatedAt: null,
    sources: [gh, mh],
    pageUrl: null,
    ...over,
  };
}

/** Each test searches something unique, so the cross-mount view cache never answers for it. */
let seq = 0;
const uniqueSearch = (word: string) => `${word}-${++seq}`;

/** Let mocked promises and the 300 ms search debounce settle. */
async function settle() {
  for (let i = 0; i < 6; i++) {
    await Promise.resolve();
    await tick();
  }
}

function props(over: Record<string, unknown> = {}) {
  return {
    installed: new Set<string>(),
    onInstalled: vi.fn(),
    showTitle: false,
    seed: uniqueSearch("q"),
    ...over,
  };
}

let unlisten: ReturnType<typeof vi.fn>;

beforeEach(() => {
  unlisten = vi.fn();
  vi.mocked(onInstallProgress).mockResolvedValue(unlisten);
  vi.mocked(browseMods).mockReset();
});

afterEach(() => {
  for (const t of [...toasts]) dismissToast(t.id);
});

describe("Browse states", () => {
  it("shows skeleton cards while the first page loads", async () => {
    vi.mocked(browseMods).mockReturnValue(new Promise(() => {}));
    const { container } = render(ModBrowser, props());
    await settle();
    expect(container.querySelector('[aria-busy="true"][aria-label="Loading the catalog"]')).not.toBeNull();
    expect(container.querySelectorAll(".sk-card").length).toBeGreaterThan(0);
  });

  it("says the catalog is unreachable, keeps the library reassurance, and retries", async () => {
    vi.mocked(browseMods).mockRejectedValueOnce(new AppError("NET_OFFLINE", "dns error"));
    render(ModBrowser, props());
    await settle();
    expect(screen.getByText("Silo can't reach its catalog right now.")).toBeInTheDocument();
    expect(screen.getByText("Your library still works.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copy details" })).toBeInTheDocument();

    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [mod("a")], total: 1 });
    await fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await settle();
    expect(browseMods).toHaveBeenCalledTimes(2);
    expect(screen.queryByText("Silo can't reach its catalog right now.")).toBeNull();
    expect(screen.getByRole("list", { name: "Catalog mods" })).toBeInTheDocument();
  });

  it("names an empty search and clears it", async () => {
    const word = uniqueSearch("lizard");
    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [], total: 0 });
    render(ModBrowser, props({ seed: word }));
    await settle();
    expect(screen.getByText(`No mods match “${word}”.`)).toBeInTheDocument();
    expect(vi.mocked(browseMods).mock.calls[0][0].query).toBe(word);

    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [mod("b")], total: 1 });
    await fireEvent.click(screen.getByRole("button", { name: "Clear search" }));
    await settle();
    expect(vi.mocked(browseMods).mock.calls[1][0].query).toBeUndefined();
    expect(screen.queryByText(`No mods match “${word}”.`)).toBeNull();
  });
});

describe("Browse cards", () => {
  it("are a keyed virtual list with spelled-out sources and a primary button per state", async () => {
    vi.mocked(browseMods).mockResolvedValueOnce({
      mods: [
        mod("a"),
        mod("b", { sources: [mh] }),
        mod("c", { verified: true, verifiedAuthor: { login: "lizard" } }),
      ],
      total: 3,
    });
    const { container } = render(ModBrowser, props({ installed: new Set(["FS25_b"]) }));
    await settle();
    const list = screen.getByRole("list", { name: "Catalog mods" });
    const cards = list.querySelectorAll('[role="listitem"]');
    expect([...cards].map((c) => c.getAttribute("data-key"))).toEqual(["a", "b", "c"]);
    expect(cards[1]).toHaveAttribute("aria-label", "Mod b by Someone, in your library");

    expect(container.textContent).toContain("GitHub 1.0.6.0");
    expect(container.textContent).toContain("ModHub 1.0.5.0");
    expect(container.textContent).not.toMatch(/\b(GH|MH)\b/);

    expect(screen.getByRole("button", { name: "Install: Mod a" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Show in library: Mod b" })).toBeInTheDocument();
    expect(screen.getByText("In your library")).toBeInTheDocument();
    // Verified author only on the claimed card; Verified build is its own badge.
    expect(screen.getAllByText("Verified author")).toHaveLength(1);
    expect(screen.getAllByText("Verified build")).toHaveLength(1);
  });

  it("offers Open on ModHub for an index-only mod", async () => {
    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [mod("m", { sources: [mh] })], total: 1 });
    render(ModBrowser, props());
    await settle();
    expect(screen.getByRole("button", { name: "Open on ModHub: Mod m" })).toBeInTheDocument();
  });
});

describe("listener cleanup", () => {
  it("removes the install-progress listener on unmount", async () => {
    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [], total: 0 });
    const { unmount } = render(ModBrowser, props());
    await settle();
    expect(onInstallProgress).toHaveBeenCalled();
    expect(unlisten).not.toHaveBeenCalled();
    unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });

  it("drops a listener that only arrives after Browse unmounted", async () => {
    let resolve!: (fn: () => void) => void;
    vi.mocked(onInstallProgress).mockReturnValueOnce(new Promise((r) => (resolve = r)));
    vi.mocked(browseMods).mockResolvedValueOnce({ mods: [], total: 0 });
    const { unmount } = render(ModBrowser, props());
    unmount();
    expect(unlisten).not.toHaveBeenCalled();
    resolve(unlisten);
    await settle();
    expect(unlisten).toHaveBeenCalledTimes(1);
  });
});

describe("GitHub sign-in after the view unmounts", () => {
  beforeEach(() => {
    resetGithubAuth();
    vi.useFakeTimers();
    vi.mocked(ghDeviceStart).mockResolvedValue({
      deviceCode: "dev",
      userCode: "ABCD-1234",
      verificationUri: "https://github.com/login/device",
      interval: 5,
      expiresIn: 900,
    });
  });
  afterEach(() => {
    resetGithubAuth();
    vi.useRealTimers();
  });

  it("keeps one poll going, then reports by toast and stops once GitHub answers", async () => {
    const { unmount } = render(GitHubAuth);
    await vi.advanceTimersByTimeAsync(0);
    await startDeviceFlow();
    expect(pollArmed()).toBe(true);
    unmount();

    vi.mocked(ghDevicePoll).mockResolvedValueOnce({ status: "denied", error: null });
    await vi.advanceTimersByTimeAsync(5000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
    // No view is mounted, so the outcome is a toast, and no timer is left behind.
    expect(toasts.some((t) => t.message === "Authorization was denied.")).toBe(true);
    expect(pollArmed()).toBe(false);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(ghDevicePoll).toHaveBeenCalledTimes(1);
  });
});
