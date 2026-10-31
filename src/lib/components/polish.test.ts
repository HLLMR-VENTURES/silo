// @vitest-environment jsdom
// 1.0 polish (copy-polish.md, logic list): what a new player and a failed copy see.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";

const api = vi.hoisted(() => ({
  scanLog: vi.fn(),
  logStatus: vi.fn(),
  scanBindings: vi.fn(),
  copyText: vi.fn(),
  saveTextFile: vi.fn(),
}));
vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  ...api,
}));

import LogTriage from "./LogTriage.svelte";
import BindingsView from "./BindingsView.svelte";
import ErrorState from "../ui/ErrorState.svelte";

const noLog = { code: "NO_LOG", message: "No game log yet. Play Farming Simulator 25 once, then come back here." };

describe("no game log yet (L5)", () => {
  it("Last run shows a calm empty state, not an error", async () => {
    api.scanLog.mockRejectedValue(noLog);
    api.logStatus.mockResolvedValue({ path: null, modifiedMs: null, gameRunning: false, lastSeenMs: null });
    render(LogTriage, { activeMods: [], activeMaps: [], onApply: vi.fn(), onBisecting: vi.fn() });
    expect(await screen.findByText("No game log yet. Play Farming Simulator 25 once, then come back here.")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.queryByText(/Something went wrong/)).not.toBeInTheDocument();
  });

  it("the controls map shows an empty state too", async () => {
    api.scanBindings.mockRejectedValue({ code: "NO_LOG", message: "No control settings yet." });
    render(BindingsView, {});
    expect(await screen.findByText("No control settings yet. Play Farming Simulator 25 once, then come back here.")).toBeInTheDocument();
    expect(screen.queryByText(/Something went wrong/)).not.toBeInTheDocument();
  });

  it("also from the older backend's plain text", async () => {
    api.scanBindings.mockRejectedValue("No inputBinding.xml found. Launch FS25 at least once first.");
    render(BindingsView, {});
    expect(await screen.findByText("No control settings yet. Play Farming Simulator 25 once, then come back here.")).toBeInTheDocument();
  });
});

describe("the controls map (L21)", () => {
  it("writes actions in sentence case", async () => {
    api.scanBindings.mockResolvedValue({
      totalActions: 1,
      totalBindings: 1,
      devices: [
        { device: "KB_MOUSE_DEFAULT", bindings: [{ action: "AXIS_MOVE_SIDE_VEHICLE", input: "KEY_a" }], shared: [] },
      ],
    });
    render(BindingsView, {});
    expect(await screen.findByText("Axis move side vehicle")).toBeInTheDocument();
  });
});

describe("the log line button (L19)", () => {
  it("its name starts with the text it shows", async () => {
    api.scanLog.mockResolvedValue({
      engineVersion: "1",
      modCount: 1,
      cleanExit: false,
      crashed: true,
      mods: [{ modName: "FS25_X", errors: 3, warnings: 0, benign: 0, sample: "Error: boom", sampleLine: 12 }],
      unattributed: 0,
      totalFindings: 3, groundPileCap: null, groundPileDisabled: [], fillTypeCap: null, droppedFillTypes: [],
    });
    api.logStatus.mockResolvedValue({ path: "log.txt", modifiedMs: 1, gameRunning: false, lastSeenMs: null });
    render(LogTriage, { activeMods: ["FS25_X"], activeMaps: [], onApply: vi.fn(), onBisecting: vi.fn() });
    const btn = await screen.findByRole("button", { name: /log line for FS25_X/ });
    expect(btn.getAttribute("aria-label")?.startsWith("line 12")).toBe(true);
    expect(btn).toHaveAttribute("aria-expanded", "false");
  });
});

describe("Copy details when the clipboard fails (L13)", () => {
  it("says the details were saved to a file, not copied", async () => {
    api.copyText.mockRejectedValue(new Error("no clipboard"));
    api.saveTextFile.mockResolvedValue(true);
    render(ErrorState, { title: "Couldn't read it", details: "Code: X" });
    await userEvent.setup().click(screen.getByRole("button", { name: "Copy details" }));
    expect(await screen.findByRole("button", { name: "Saved to a file" })).toBeInTheDocument();
  });
});
