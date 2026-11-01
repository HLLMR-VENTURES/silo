// @vitest-environment jsdom
// The drawer after a save or a check that failed (#358): nothing unsaved or unconfirmed is shown
// as current, and nothing the user typed is thrown away.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ProvenanceCheck from "../ProvenanceCheck.svelte";
import ModCuration from "../ModCuration.svelte";
import { setVerdict } from "../../provenanceCache.svelte";
import { setCuration, verifyMod } from "../../api";
import type { CurationRow, ModEntry, VerifyResult } from "../../types";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  verifyMod: vi.fn(),
  setCuration: vi.fn(() => Promise.resolve()),
  setTags: vi.fn(() => Promise.resolve()),
}));

function entry(i: number): ModEntry {
  return {
    techName: `FS25_Fail${i}`,
    path: `C:/mods/FS25_Fail${i}.zip`,
    kind: "zip",
    size: 4096,
    mtimeMs: 9000 + i,
    title: `Fail ${i}`,
    version: "1.0.0.0",
  } as unknown as ModEntry;
}

const row = (techName: string, over: Partial<CurationRow> = {}): CurationRow => ({
  techName,
  favorite: false,
  hidden: false,
  broken: false,
  rating: 0,
  note: null,
  skipUpdates: false,
  ...over,
});

async function settle() {
  for (let i = 0; i < 4; i++) {
    await tick();
    await Promise.resolve();
  }
}

beforeEach(() => {
  vi.mocked(verifyMod).mockReset();
  vi.mocked(setCuration).mockReset().mockResolvedValue(undefined);
});

describe("a failed integrity Re-check", () => {
  it("shows the failure, not the old verdict as if it were confirmed", async () => {
    const m = entry(1);
    setVerdict(m, { status: "modified", how: null, added: [], removed: [], changed: ["a.lua"], matchedVersion: null, note: null });
    vi.mocked(verifyMod).mockRejectedValueOnce("The catalog didn't answer.");
    const user = userEvent.setup();
    const { rerender } = render(ProvenanceCheck, { mod: m });
    await user.click(screen.getByRole("button", { name: "Re-check" }));
    await settle();
    expect(screen.getByText(/catalog didn't answer/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Try again" })).toBeInTheDocument();
    // Still there after the drawer shows another mod and comes back.
    await rerender({ mod: entry(2) });
    await settle();
    await rerender({ mod: m });
    await settle();
    expect(screen.getByText(/catalog didn't answer/)).toBeInTheDocument();
  });

  it("a Try again that works shows the new verdict", async () => {
    const m = entry(3);
    setVerdict(m, { status: "unverified", how: null, added: [], removed: [], changed: [], matchedVersion: null, note: null });
    vi.mocked(verifyMod)
      .mockRejectedValueOnce("The catalog didn't answer.")
      .mockResolvedValueOnce({ status: "verified", how: "exact", matchedVersion: "1.0.0.0" } as unknown as VerifyResult);
    const user = userEvent.setup();
    render(ProvenanceCheck, { mod: m });
    await user.click(screen.getByRole("button", { name: "Re-check" }));
    await settle();
    await user.click(screen.getByRole("button", { name: "Try again" }));
    await settle();
    expect(screen.queryByText(/catalog didn't answer/)).toBeNull();
    expect(screen.getByText(/Exactly the same as the published build/)).toBeInTheDocument();
  });
});

describe("a failed curation save in the drawer", () => {
  const base = { tags: [], onTagsChange: vi.fn(), onFilterTag: vi.fn() };

  it("hands the row back to be put back (a rating)", async () => {
    vi.mocked(setCuration).mockRejectedValueOnce("database is locked");
    const onCurationChange = vi.fn();
    const onCurationFailed = vi.fn();
    const before = row("A");
    render(ModCuration, { ...base, onCurationChange, onCurationFailed, techName: "A", curation: before });
    await userEvent.setup().click(screen.getByRole("button", { name: "3 stars" }));
    await settle();
    const sent = onCurationChange.mock.calls[0][0] as CurationRow;
    expect(sent.rating).toBe(3);
    expect(onCurationFailed).toHaveBeenCalledWith(sent, before);
  });

  it("a note that didn't save stays in the field, and leaving it tries again", async () => {
    let fail!: (e: unknown) => void;
    vi.mocked(setCuration).mockImplementationOnce(() => new Promise((_, r) => (fail = r)));
    const before = row("B", { note: "old" });
    const onCurationChange = vi.fn();
    const onCurationFailed = vi.fn();
    const props = { ...base, onCurationChange, onCurationFailed, techName: "B" };
    const { rerender } = render(ModCuration, { ...props, curation: before });
    const notes = screen.getByLabelText("Notes") as HTMLTextAreaElement;
    await fireEvent.input(notes, { target: { value: "new note" } });
    await fireEvent.blur(notes);
    // App shows the new note at once…
    const sent = onCurationChange.mock.calls[0][0] as CurationRow;
    await rerender({ ...props, curation: sent });
    await settle();
    // …then the save fails and App puts the saved row back.
    fail("database is locked");
    await settle();
    expect(onCurationFailed).toHaveBeenCalledWith(sent, before);
    await rerender({ ...props, curation: before });
    await settle();
    expect((screen.getByLabelText("Notes") as HTMLTextAreaElement).value).toBe("new note");
    await fireEvent.blur(screen.getByLabelText("Notes"));
    expect(setCuration).toHaveBeenCalledTimes(2);
    expect(setCuration).toHaveBeenLastCalledWith(expect.objectContaining({ techName: "B", note: "new note" }));
  });
});
