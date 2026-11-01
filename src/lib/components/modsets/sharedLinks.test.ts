// @vitest-environment jsdom
// Your shared links (#355): a listing that failed never reads as "nothing shared yet", and a
// list that's only partly readable says so.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  collectionsList: vi.fn(),
  collectionUpdate: vi.fn(),
  ghStatus: vi.fn(),
  logFrontendError: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../ui")>()),
  ask: vi.fn(() => Promise.resolve(true)),
  toast: vi.fn(),
}));

import { collectionsList, collectionUpdate, ghStatus } from "../../api";
import SharedLinks from "./SharedLinks.svelte";

const link = {
  kind: "gist" as const,
  reference: "abc",
  name: "Harvest 2026",
  modCount: 3,
  createdAt: null,
  pageUrl: "https://silo.hllmr.com/c/?g=abc",
  sourceUrl: "https://gist.github.com/abc",
  canDelete: true,
};

beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(ghStatus).mockResolvedValue({ clientId: null, user: "me", builtin: true, canWrite: true, canGist: true });
});

describe("Your shared links", () => {
  it("a failed listing shows the error, not an empty list", async () => {
    vi.mocked(collectionsList).mockRejectedValue(new Error("GitHub rejected the token. Reconnect your account."));
    render(SharedLinks, { active: [] });
    const err = await screen.findByRole("status");
    expect(err.className).toContain("err");
    expect(err.textContent).not.toBe("");
    expect(screen.queryByText(/Nothing shared yet/)).toBeNull();
  });

  it("a partial list shows what it found and says what's missing", async () => {
    vi.mocked(collectionsList).mockResolvedValue({
      items: [link],
      warning: "Silo couldn't list your public GitHub repositories, so links shared that way aren't shown.",
    });
    render(SharedLinks, { active: [] });
    expect(await screen.findByText("Harvest 2026")).toBeInTheDocument();
    expect(screen.getByText(/couldn't list your public GitHub repositories/)).toBeInTheDocument();
  });

  it("a partial list that found nothing doesn't claim nothing was shared", async () => {
    vi.mocked(collectionsList).mockResolvedValue({ items: [], warning: "Silo couldn't list your unlisted links on GitHub." });
    render(SharedLinks, { active: [] });
    expect(await screen.findByText(/couldn't list your unlisted links/)).toBeInTheDocument();
    expect(screen.queryByText(/Nothing shared yet/)).toBeNull();
  });

  it("updating a link names the mods it couldn't fingerprint, and keeps a README problem on the page", async () => {
    vi.mocked(collectionsList).mockResolvedValue({ items: [link], warning: null });
    vi.mocked(collectionUpdate).mockResolvedValue({
      url: link.pageUrl,
      count: 2,
      omitted: [],
      unpinned: ["FS25_A"],
      warning: "The link works, but Silo couldn't add the page that describes the set on GitHub.",
    });
    render(SharedLinks, { active: [{ techName: "FS25_A", path: "/m/FS25_A.zip", kind: "zip", version: "1" }] });
    await fireEvent.click(await screen.findByRole("button", { name: "Update link: Harvest 2026" }));
    expect(await screen.findByText(/Silo couldn't read 1 mod \(FS25_A\)/)).toBeInTheDocument();
    expect(screen.getByText(/couldn't add the page that describes the set/)).toBeInTheDocument();
  });
});
