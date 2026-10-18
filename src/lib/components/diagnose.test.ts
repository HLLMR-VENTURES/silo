// @vitest-environment jsdom
// Diagnose (#260): Conflicts (labels, hidden notes, Park one), Problems (actions per row), Last
// run (actions on a named mod, the crash notice's Find the problem mod) and the bisection finish
// that puts the mods back and parks the culprit.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import ConflictsPanel from "./panels/ConflictsPanel.svelte";
import ProblemsPanel from "./panels/ProblemsPanel.svelte";
import LogTriage from "./LogTriage.svelte";
import BisectFlow from "./BisectFlow.svelte";
import {
  bisectPlan,
  bisectSnapshotClear,
  bisectSnapshotGet,
  bisectSnapshotSave,
  copyText,
  logStatus,
  scanLog,
} from "../api";
import type { Conflict, LogReport, ModEntry } from "../types";

vi.mock("../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../api")>()),
  scanLog: vi.fn(),
  logStatus: vi.fn(),
  copyText: vi.fn(() => Promise.resolve()),
  bisectPlan: vi.fn(),
  bisectNarrow: vi.fn(),
  bisectSnapshotSave: vi.fn(() => Promise.resolve()),
  bisectSnapshotGet: vi.fn(),
  bisectSnapshotClear: vi.fn(() => Promise.resolve()),
  launchGame: vi.fn(() => Promise.resolve()),
  revealInFolder: vi.fn(() => Promise.resolve()),
  userDirPath: vi.fn(() => Promise.resolve("C:/fs25")),
}));

function conflict(over: Partial<Conflict>): Conflict {
  return {
    id: "x",
    severity: "critical",
    kind: "uniqueType",
    name: "seedSlot",
    explanation: "These mods use the same exclusive slot (\"seedSlot\"). The game loads only one mod per slot, so the others won't take effect.",
    mods: ["Seeder A", "Seeder B"],
    techNames: ["FS25_SeederA", "FS25_SeederB"],
    ...over,
  };
}

describe("Conflicts", () => {
  const list: Conflict[] = [
    conflict({ id: "u" }),
    conflict({ id: "s", severity: "warning", kind: "script", name: "util.lua" }),
    conflict({
      id: "m",
      severity: "info",
      kind: "map",
      name: "4 maps active",
      explanation: "Only the map your savegame uses loads.",
      mods: ["Map 1", "Map 2", "Map 3", "Map 4"],
      techNames: ["M1", "M2", "M3", "M4"],
    }),
  ];

  it("plain headline and labels; notes hidden until Show notes", async () => {
    const user = userEvent.setup();
    render(ConflictsPanel, { conflicts: list, activeCount: 40 });
    const head = screen.getByRole("heading", { level: 3 });
    expect(head).toHaveTextContent("1 problem · 1 to review · 1 note hidden");
    expect(screen.getByText("Problem")).toBeInTheDocument();
    expect(screen.getByText("Review")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Two mods claim the same exclusive slot" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Two mods ship a script with the same file name" })).toBeInTheDocument();
    // Raw wire words never show.
    expect(screen.queryByText("critical")).toBeNull();
    expect(screen.queryByText("More than one map is active")).toBeNull();

    await user.click(screen.getByRole("switch", { name: "Show notes" }));
    expect(head).toHaveTextContent("1 note");
    expect(head).not.toHaveTextContent("hidden");
    expect(screen.getByText("Note")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "More than one map is active" })).toBeInTheDocument();
    expect(screen.getByText("Only the map your savegame uses loads.")).toBeInTheDocument();
  });

  it("mods open the drawer; Park one… parks the chosen mod; the maps note has no Park", async () => {
    const user = userEvent.setup();
    const onOpen = vi.fn();
    const onPark = vi.fn();
    render(ConflictsPanel, { conflicts: list, activeCount: 40, onOpen, onPark, showNotes: true });
    await user.click(screen.getAllByRole("button", { name: "Seeder B" })[0]);
    expect(onOpen).toHaveBeenCalledWith("FS25_SeederB");
    // Two clashes (exclusive slot, script) get a menu; the maps note doesn't.
    const menus = screen.getAllByRole("button", { name: "Park one…" });
    expect(menus).toHaveLength(2);
    await user.click(menus[0]);
    await user.click(await screen.findByRole("menuitem", { name: "Park Seeder A" }));
    expect(onPark).toHaveBeenCalledWith("FS25_SeederA");
  });

  it("only notes: says nothing needs doing", () => {
    render(ConflictsPanel, { conflicts: [list[2]], activeCount: 4 });
    expect(screen.getByText(/No problems and nothing to review/)).toBeInTheDocument();
  });
});

function entry(techName: string, over: Partial<ModEntry> = {}): ModEntry {
  return { techName, title: techName.replace("FS25_", ""), path: `C:/lib/${techName}.zip`, ...over } as ModEntry;
}

describe("Problems", () => {
  it("each group has its actions", async () => {
    const user = userEvent.setup();
    const onFindInBrowse = vi.fn();
    const onShowFile = vi.fn();
    const onPark = vi.fn();
    const onResolveForeign = vi.fn(() => Promise.resolve(true));
    render(ProblemsPanel, {
      health: {
        missingDeps: [{ mod: entry("FS25_Trailer"), missing: ["FS25_BaseLib"] }],
        corrupt: [entry("FS25_Broken", { error: "invalid zip" })],
        ignored: [entry("1FS25_Digit")],
        foreign: [
          { techName: "FS25_Plow", fileName: "FS25_Plow.zip", kind: "zip", flatVersion: "1.1.0.0", managedVersion: "1.0.0.0" },
        ],
      },
      isActive: (t: string) => t === "FS25_Broken",
      onFindInBrowse,
      onShowFile,
      onPark,
      onResolveForeign,
    });
    expect(screen.getByRole("heading", { level: 3 })).toHaveTextContent("4 mods need attention");
    expect(screen.getByRole("heading", { name: /Missing a required mod/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Can't be read/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /Ignored by the game: the name starts with a digit/ })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /A different build is in your mods folder/ })).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Find it in Browse" }));
    expect(onFindInBrowse).toHaveBeenCalledWith("FS25_BaseLib");
    await user.click(screen.getAllByRole("button", { name: "Show file" })[0]);
    expect(onShowFile).toHaveBeenCalledWith("C:/lib/FS25_Broken.zip");
    await user.click(screen.getByRole("button", { name: "Park" }));
    expect(onPark).toHaveBeenCalledWith("FS25_Broken");
    await user.click(screen.getByRole("button", { name: "Use this version" }));
    expect(onResolveForeign).toHaveBeenCalledWith("FS25_Plow.zip", "adopt");
    await user.click(screen.getByRole("button", { name: "Keep Silo's" }));
    expect(onResolveForeign).toHaveBeenCalledWith("FS25_Plow.zip", "restore");
    // "Foreign files" is gone from the words.
    expect(screen.queryByText(/foreign/i)).toBeNull();
  });
});

function report(crashed: boolean, mods: LogReport["mods"]): LogReport {
  return { engineVersion: "1.4.0.0", modCount: 3, cleanExit: !crashed, crashed, mods, unattributed: 2, totalFindings: 9 };
}

const lookup = (t: string) =>
  t === "FS25_Mulcher" ? { title: "Field Mulcher Extended", active: true } : null;

describe("Last run", () => {
  beforeEach(() => {
    vi.mocked(logStatus).mockResolvedValue({ path: "log.txt", modifiedMs: Date.now(), gameRunning: false, lastSeenMs: null });
  });

  it("a named mod has Park, Show in library and Copy details", async () => {
    const user = userEvent.setup();
    vi.mocked(scanLog).mockResolvedValue(
      report(true, [
        { modName: "FS25_Mulcher", errors: 14, warnings: 3, benign: 0, sample: "Error: nil value", sampleLine: 812 },
      ]),
    );
    const onPark = vi.fn();
    const onShow = vi.fn();
    render(LogTriage, { activeMods: ["FS25_Mulcher", "FS25_X"], activeMaps: [], onApply: vi.fn(), onBisecting: vi.fn(), lookup, onPark, onShow });
    expect(await screen.findByText("Your last game ended unexpectedly.")).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Most likely causes" })).toBeInTheDocument();
    expect(screen.getByText("Field Mulcher Extended")).toBeInTheDocument();
    expect(screen.getByText("14 errors · 3 warnings")).toBeInTheDocument();
    expect(screen.getByText(/2 findings aren't tied to a mod/)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Park" }));
    expect(onPark).toHaveBeenCalledWith("FS25_Mulcher");
    await user.click(screen.getByRole("button", { name: "Show in library" }));
    expect(onShow).toHaveBeenCalledWith("FS25_Mulcher");
    await user.click(screen.getByRole("button", { name: "Copy details" }));
    expect(copyText).toHaveBeenCalledWith(expect.stringContaining("Field Mulcher Extended (FS25_Mulcher)"));
    // The first log line expands.
    const line = screen.getByRole("button", { name: /log line for Field Mulcher Extended/ });
    expect(line).toHaveAttribute("aria-expanded", "false");
    await user.click(line);
    expect(line).toHaveAttribute("aria-expanded", "true");
  });

  it("the crash notice's Find the problem mod opens the flow, once", async () => {
    vi.mocked(scanLog).mockResolvedValue(report(true, []));
    const onBisectRequestTaken = vi.fn();
    render(LogTriage, {
      activeMods: ["A", "B", "C"],
      activeMaps: [],
      onApply: vi.fn(),
      onBisecting: vi.fn(),
      bisectRequested: true,
      onBisectRequestTaken,
    });
    expect(await screen.findByRole("heading", { name: "Find the problem mod" })).toBeInTheDocument();
    expect(onBisectRequestTaken).toHaveBeenCalledTimes(1);
  });
});

describe("Find the problem mod: the finish", () => {
  it("Put my mods back and park X restores the saved set without the culprit", async () => {
    const user = userEvent.setup();
    const saved = ["MAP", "FS25_A", "FS25_Mulcher", "FS25_C"];
    let snapshot: string[] | null = null;
    vi.mocked(bisectSnapshotGet).mockImplementation(async () => snapshot);
    vi.mocked(bisectSnapshotSave).mockImplementation(async (s: string[]) => void (snapshot = s));
    vi.mocked(bisectSnapshotClear).mockImplementation(async () => void (snapshot = null));
    vi.mocked(bisectPlan).mockResolvedValue({ kind: "culprit", modName: "FS25_Mulcher" });
    const onApply = vi.fn(() => Promise.resolve());
    const onBisecting = vi.fn();
    const onFinished = vi.fn();
    render(BisectFlow, {
      pool: ["FS25_A", "FS25_Mulcher", "FS25_C"],
      alwaysOn: ["MAP"],
      onApply,
      onBisecting,
      onExit: vi.fn(),
      onFinished,
      titleOf: (t: string) => (t === "FS25_Mulcher" ? "Field Mulcher Extended" : t),
    });
    await user.click(screen.getByRole("button", { name: "Start" }));
    expect(await screen.findByText(/Most likely:/)).toHaveTextContent("Most likely: Field Mulcher Extended");
    expect(snapshot).toEqual(saved);
    await user.click(screen.getByRole("button", { name: "Put my mods back and park Field Mulcher Extended" }));
    await waitFor(() => expect(onFinished).toHaveBeenCalledWith("FS25_Mulcher"));
    expect(onApply).toHaveBeenLastCalledWith(["MAP", "FS25_A", "FS25_C"]);
    expect(snapshot).toBeNull();
    expect(onBisecting).toHaveBeenLastCalledWith(false);
  });

  it("Put my mods back restores everything", async () => {
    const user = userEvent.setup();
    let snapshot: string[] | null = null;
    vi.mocked(bisectSnapshotGet).mockImplementation(async () => snapshot);
    vi.mocked(bisectSnapshotSave).mockImplementation(async (s: string[]) => void (snapshot = s));
    vi.mocked(bisectSnapshotClear).mockImplementation(async () => void (snapshot = null));
    vi.mocked(bisectPlan).mockResolvedValue({ kind: "culprit", modName: "FS25_C" });
    const onApply = vi.fn(() => Promise.resolve());
    const onFinished = vi.fn();
    render(BisectFlow, { pool: ["FS25_A", "FS25_C"], alwaysOn: [], onApply, onBisecting: vi.fn(), onExit: vi.fn(), onFinished });
    await user.click(screen.getByRole("button", { name: "Start" }));
    await user.click(await screen.findByRole("button", { name: "Put my mods back" }));
    await waitFor(() => expect(onFinished).toHaveBeenCalledWith(null));
    expect(onApply).toHaveBeenLastCalledWith(["FS25_A", "FS25_C"]);
  });
});
