// @vitest-environment jsdom
// Find the problem mod (#358): while Start's preparation runs (its "File N mods" dialog, then the
// filing), Start and Not now stay off, so a second click can't start a second preparation.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import BisectFlow from "./BisectFlow.svelte";
import { bisectPlan, bisectSnapshotGet, bisectSnapshotSave } from "../api";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  bisectPlan: vi.fn(),
  bisectSnapshotSave: vi.fn(() => Promise.resolve()),
  bisectSnapshotGet: vi.fn(),
  bisectSnapshotClear: vi.fn(() => Promise.resolve()),
  logStatus: vi.fn(() => Promise.resolve({ path: "log.txt", modifiedMs: 1, gameRunning: false, lastSeenMs: null })),
}));

beforeEach(() => {
  vi.mocked(bisectSnapshotGet).mockReset().mockResolvedValue(null);
  vi.mocked(bisectSnapshotSave).mockReset().mockResolvedValue(undefined);
  vi.mocked(bisectPlan).mockReset().mockResolvedValue({ kind: "culprit", modName: "FS25_B" });
});

describe("Find the problem mod: Start while preparing", () => {
  it("stays off until the preparation is done, and a second click does nothing", async () => {
    let ready!: (ok: boolean) => void;
    const onPrepare = vi.fn(() => new Promise<boolean>((r) => (ready = r)));
    const user = userEvent.setup();
    render(BisectFlow, { pool: ["FS25_A", "FS25_B"], alwaysOn: [], onApply: vi.fn(() => Promise.resolve()), onBisecting: vi.fn(), onExit: vi.fn(), onPrepare });
    const start = screen.getByRole("button", { name: "Start" });
    await user.click(start);
    await waitFor(() => expect(onPrepare).toHaveBeenCalledOnce());
    expect(start).toBeDisabled();
    expect(screen.getByRole("button", { name: "Not now" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Close" })).toBeDisabled();
    await user.click(start);
    expect(onPrepare).toHaveBeenCalledOnce();
    // Declined (or refused): Start comes back, and nothing was saved.
    ready(false);
    await waitFor(() => expect(start).toBeEnabled());
    expect(bisectSnapshotSave).not.toHaveBeenCalled();
  });
});
