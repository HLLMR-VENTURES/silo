// @vitest-environment jsdom
// The drawer's own async actions (Verify integrity, Check for update) when the drawer moves to
// another mod before the answer arrives: the answer belongs to the mod it was asked for.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ProvenanceCheck from "../ProvenanceCheck.svelte";
import ModRepoLink from "../ModRepoLink.svelte";
import ModCuration from "../ModCuration.svelte";
import { getVerdict } from "../../provenanceCache.svelte";
import { checkModUpdate, setCuration, verifyMod } from "../../api";
import { ask } from "../../ui";
import type { ModEntry, UpdateInfo, VerifyResult } from "../../types";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  verifyMod: vi.fn(),
  checkModUpdate: vi.fn(),
  guessRepo: vi.fn(() => Promise.resolve(null)),
  setCuration: vi.fn(() => Promise.resolve()),
  setTags: vi.fn(() => Promise.resolve()),
}));
vi.mock("../../ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../ui")>()),
  ask: vi.fn(() => Promise.resolve(false)),
}));

function entry(i: number): ModEntry {
  return {
    techName: `FS25_Race${i}`,
    path: `C:/mods/FS25_Race${i}.zip`,
    kind: "zip",
    size: 4096,
    mtimeMs: 5000 + i,
    title: `Race ${i}`,
    author: "Someone",
    version: "1.0.0.0",
    descVersion: 1,
    iconFilename: null,
    isMap: false,
    mapTitle: null,
    category: "Tools",
    subcategory: null,
    organized: false,
    active: false,
    dependencies: [],
    scriptCount: 0,
    registrationCount: 0,
    uniqueType: null,
  } as unknown as ModEntry;
}

async function settle() {
  for (let i = 0; i < 4; i++) {
    await tick();
    await Promise.resolve();
  }
}

beforeEach(() => {
  vi.mocked(verifyMod).mockReset();
  vi.mocked(checkModUpdate).mockReset();
});

describe("Verify integrity across a drawer switch", () => {
  it("files the verdict under the mod that was hashed, not the one shown when it lands", async () => {
    let answer!: (r: VerifyResult) => void;
    vi.mocked(verifyMod).mockImplementationOnce(() => new Promise((r) => (answer = r)));
    const a = entry(1);
    const b = entry(2);
    const user = userEvent.setup();
    const { rerender } = render(ProvenanceCheck, { mod: a });
    await user.click(screen.getByRole("button", { name: /Verify integrity/i }));
    await rerender({ mod: b });
    await settle();
    answer({ status: "verified", how: "exact", matchedVersion: "1.0.0.0" } as unknown as VerifyResult);
    await settle();
    expect(getVerdict(b)).toBeUndefined();
    expect(getVerdict(a)?.status).toBe("verified");
    expect(screen.queryByText(/Verified/)).toBeNull();
  });

  it("doesn't show the first mod's error on the second", async () => {
    let fail!: (e: unknown) => void;
    vi.mocked(verifyMod).mockImplementationOnce(() => new Promise((_, r) => (fail = r)));
    const user = userEvent.setup();
    const { rerender } = render(ProvenanceCheck, { mod: entry(3) });
    await user.click(screen.getByRole("button", { name: /Verify integrity/i }));
    await rerender({ mod: entry(4) });
    await settle();
    fail("The catalog didn't answer.");
    await settle();
    expect(screen.getByRole("button", { name: /Verify integrity/i })).toBeInTheDocument();
    expect(screen.queryByText(/catalog didn't answer/)).toBeNull();
  });
});

describe("Check for update across a drawer switch", () => {
  it("doesn't offer the first mod's release on the second", async () => {
    let answer!: (u: UpdateInfo) => void;
    vi.mocked(checkModUpdate).mockImplementationOnce(() => new Promise((r) => (answer = r)));
    const user = userEvent.setup();
    const base = { onRepoChange: vi.fn(), onInstalled: vi.fn() };
    const { rerender } = render(ModRepoLink, { ...base, mod: entry(5), repo: { owner: "o", repo: "r" } });
    await user.click(screen.getByRole("button", { name: "Check for update" }));
    await rerender({ ...base, mod: entry(6), repo: null });
    await settle();
    answer({
      hasUpdate: true,
      current: "1.0.0.0",
      release: { tag: "v2.0.0", htmlUrl: null, assetUrl: "https://example.com/a.zip" },
    } as unknown as UpdateInfo);
    await settle();
    expect(screen.queryByText(/Update available/)).toBeNull();
    expect(screen.queryByRole("button", { name: /Download and install/ })).toBeNull();
  });
});

describe("Notes when the drawer moves on without a blur", () => {
  const row = (techName: string, note: string | null = null) => ({
    techName,
    favorite: false,
    hidden: false,
    broken: false,
    rating: 0,
    note,
  });
  const base = { tags: [], onTagsChange: vi.fn(), onFilterTag: vi.fn() };

  it("saves the draft under its own mod when the drawer switches mods", async () => {
    vi.mocked(setCuration).mockClear();
    const onCurationChange = vi.fn();
    const { rerender } = render(ModCuration, { ...base, onCurationChange, techName: "A", curation: row("A") });
    const notes = screen.getByLabelText("Notes") as HTMLTextAreaElement;
    await fireEvent.input(notes, { target: { value: "needs the 1.2 patch" } });
    await rerender({ ...base, onCurationChange, techName: "B", curation: row("B", "b's note") });
    await settle();
    expect(setCuration).toHaveBeenCalledWith(expect.objectContaining({ techName: "A", note: "needs the 1.2 patch" }));
    expect(onCurationChange).toHaveBeenCalledWith(expect.objectContaining({ techName: "A" }));
    expect((screen.getByLabelText("Notes") as HTMLTextAreaElement).value).toBe("b's note");
  });

  it("saves the draft when the drawer closes", async () => {
    vi.mocked(setCuration).mockClear();
    const { unmount } = render(ModCuration, { ...base, onCurationChange: vi.fn(), techName: "C", curation: row("C") });
    await fireEvent.input(screen.getByLabelText("Notes"), { target: { value: "typed, then Esc" } });
    unmount();
    expect(setCuration).toHaveBeenCalledWith(expect.objectContaining({ techName: "C", note: "typed, then Esc" }));
  });
});

describe("review follow-ups", () => {
  const release = (tag: string): UpdateInfo =>
    ({
      hasUpdate: true,
      current: "1.0.0.0",
      release: { tag, htmlUrl: null, assetUrl: "https://example.com/a.zip" },
    }) as unknown as UpdateInfo;

  it("a dropped answer for the first mod doesn't stop the second mod's check", async () => {
    let answerA!: (u: UpdateInfo) => void;
    vi.mocked(checkModUpdate)
      .mockImplementationOnce(() => new Promise((r) => (answerA = r)))
      .mockImplementationOnce(() => new Promise(() => {}));
    const user = userEvent.setup();
    const base = { onRepoChange: vi.fn(), onInstalled: vi.fn(), repo: { owner: "o", repo: "r" } };
    const { rerender } = render(ModRepoLink, { ...base, mod: entry(7) });
    await user.click(screen.getByRole("button", { name: "Check for update" }));
    await rerender({ ...base, mod: entry(8) });
    await settle();
    await user.click(screen.getByRole("button", { name: "Check for update" }));
    answerA(release("v2.0.0"));
    await settle();
    expect(screen.getByRole("button", { name: "Checking…" })).toBeDisabled();
  });

  it("the install dialog names the release without its V. prefix", async () => {
    vi.mocked(checkModUpdate).mockResolvedValueOnce(release("V.2.0.0"));
    const user = userEvent.setup();
    render(ModRepoLink, { onRepoChange: vi.fn(), onInstalled: vi.fn(), repo: { owner: "o", repo: "r" }, mod: entry(9) });
    await user.click(screen.getByRole("button", { name: "Check for update" }));
    await settle();
    await user.click(screen.getByRole("button", { name: /Download and install/ }));
    expect(vi.mocked(ask)).toHaveBeenLastCalledWith(expect.objectContaining({ title: "Install 2.0.0?" }));
  });

  it("a check still running on a mod stays running across a switch away and back", async () => {
    vi.mocked(verifyMod).mockImplementationOnce(() => new Promise(() => {}));
    const a = entry(10);
    const user = userEvent.setup();
    const { rerender } = render(ProvenanceCheck, { mod: a });
    await user.click(screen.getByRole("button", { name: /Verify integrity/i }));
    await rerender({ mod: entry(11) });
    await settle();
    expect(screen.getByRole("button", { name: /Verify integrity/i })).toBeInTheDocument();
    await rerender({ mod: a });
    await settle();
    expect(screen.queryByRole("button", { name: /Verify integrity/i })).toBeNull();
    expect(screen.getByText(/Checking against the published build/)).toBeInTheDocument();
    expect(verifyMod).toHaveBeenCalledTimes(1);
  });
});
