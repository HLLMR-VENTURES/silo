// @vitest-environment jsdom
// The drawer after a save or a check that failed (#358): nothing unsaved or unconfirmed is shown
// as current, and nothing the user typed is thrown away.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ProvenanceCheck from "../ProvenanceCheck.svelte";
import CurationHarness from "./__tests__/CurationHarness.svelte";
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

  it("a Re-check that fails while the drawer shows another mod is still reported on the way back", async () => {
    const m = entry(5);
    setVerdict(m, { status: "unverified", how: null, added: [], removed: [], changed: [], matchedVersion: null, note: null });
    let fail!: (e: unknown) => void;
    vi.mocked(verifyMod).mockImplementationOnce(() => new Promise((_, r) => (fail = r)));
    const user = userEvent.setup();
    const { rerender } = render(ProvenanceCheck, { mod: m });
    await user.click(screen.getByRole("button", { name: "Re-check" }));
    await rerender({ mod: entry(6) });
    await settle();
    fail("The catalog didn't answer.");
    await settle();
    expect(screen.queryByText(/catalog didn't answer/)).toBeNull(); // not on the other mod
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
  it("puts the stored row back (a rating), even after two failures in a row", async () => {
    vi.mocked(setCuration).mockRejectedValueOnce("database is locked").mockRejectedValueOnce("database is locked");
    const h = render(CurationHarness, { techName: "A", stored: row("A", { rating: 1 }) });
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "3 stars" }));
    await user.click(screen.getByRole("button", { name: "4 stars" }));
    await settle();
    expect(setCuration).toHaveBeenCalledTimes(2);
    expect((h.component as unknown as { shown(): CurationRow }).shown().rating).toBe(1);
  });

  it("a note that didn't save stays in the field, and leaving it tries again", async () => {
    let fail!: (e: unknown) => void;
    vi.mocked(setCuration).mockImplementationOnce(() => new Promise((_, r) => (fail = r)));
    const h = render(CurationHarness, { techName: "B", stored: row("B", { note: "old" }) });
    const notes = screen.getByLabelText("Notes") as HTMLTextAreaElement;
    await fireEvent.input(notes, { target: { value: "new note" } });
    await fireEvent.blur(notes);
    await settle();
    fail("database is locked");
    await settle();
    expect((h.component as unknown as { shown(): CurationRow }).shown().note).toBe("old");
    expect(notes.value).toBe("new note");
    await fireEvent.blur(notes);
    expect(setCuration).toHaveBeenCalledTimes(2);
    expect(setCuration).toHaveBeenLastCalledWith(expect.objectContaining({ techName: "B", note: "new note" }));
  });

  it("text typed while a note save is out survives that save failing", async () => {
    let fail!: (e: unknown) => void;
    vi.mocked(setCuration).mockImplementationOnce(() => new Promise((_, r) => (fail = r)));
    render(CurationHarness, { techName: "C", stored: row("C", { note: "old" }) });
    const notes = screen.getByLabelText("Notes") as HTMLTextAreaElement;
    await fireEvent.input(notes, { target: { value: "first draft" } });
    await fireEvent.blur(notes);
    await settle();
    await fireEvent.input(notes, { target: { value: "second draft" } });
    fail("database is locked");
    await settle();
    expect(notes.value).toBe("second draft");
  });

  it("a flag set elsewhere while typing doesn't wipe the note being typed", async () => {
    const h = render(CurationHarness, { techName: "D", stored: row("D", { note: "old" }) });
    const notes = screen.getByLabelText("Notes") as HTMLTextAreaElement;
    await fireEvent.input(notes, { target: { value: "typing…" } });
    (h.component as unknown as { replace(r: CurationRow): void }).replace(row("D", { note: "old", hidden: true }));
    await settle();
    expect(notes.value).toBe("typing…");
    await fireEvent.blur(notes);
    expect(setCuration).toHaveBeenLastCalledWith(expect.objectContaining({ techName: "D", note: "typing…", hidden: true }));
  });
});
