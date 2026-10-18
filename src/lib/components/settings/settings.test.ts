// @vitest-environment jsdom
// Settings sections (#236): the shared lock rule, and a control that's off says why in visible
// text under it (not only in its title). The buttons still call App's handlers.
import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import MaintenanceSettings from "./MaintenanceSettings.svelte";
import LibrarySettings from "./LibrarySettings.svelte";
import ModsFolderSettings from "./ModsFolderSettings.svelte";
import GeneralSettings from "./GeneralSettings.svelte";
import DiagnosticsSettings from "./DiagnosticsSettings.svelte";
import { openKnownFolder } from "../../api";
import { MOVE_UNFINISHED_REASON, rereadLock, SCANNING_REASON, settingsLock } from "./lock";

vi.mock("../../api", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../api")>()),
  openKnownFolder: vi.fn(() => Promise.resolve()),
}));

describe("settingsLock", () => {
  it("is off with no reason when nothing runs", () => {
    expect(settingsLock({ lockReason: "", scanning: false, moveUnfinished: false })).toEqual({
      locked: false,
      reason: undefined,
    });
  });
  it("names the move first, then the write, then the scan", () => {
    const both = { lockReason: "Silo is reclaiming disk space.", scanning: true };
    expect(settingsLock({ ...both, moveUnfinished: true }).reason).toBe(MOVE_UNFINISHED_REASON);
    expect(settingsLock({ ...both, moveUnfinished: false }).reason).toBe("Silo is reclaiming disk space.");
    expect(settingsLock({ lockReason: "", scanning: true, moveUnfinished: false })).toEqual({
      locked: true,
      reason: SCANNING_REASON,
    });
  });
  it("Re-read every mod ignores a paused move; its title stays the write's reason", () => {
    expect(rereadLock({ lockReason: "", scanning: false }).locked).toBe(false);
    const scan = rereadLock({ lockReason: "", scanning: true });
    expect(scan).toMatchObject({ locked: true, reason: SCANNING_REASON, title: undefined });
  });
});

function maintenance(lockReason = "", organizedCount = 3) {
  const props = {
    organizedCount,
    cloudSynced: false,
    lockReason,
    scanning: false,
    moveUnfinished: false,
    onRebuild: vi.fn(),
    onOptimizeLinks: vi.fn(),
    onRestoreVanilla: vi.fn(),
  };
  render(MaintenanceSettings, props);
  return props;
}

describe("MaintenanceSettings", () => {
  it("uses the new names and calls the same handlers", async () => {
    const user = userEvent.setup();
    const p = maintenance();
    await user.click(screen.getByRole("button", { name: "Re-read every mod" }));
    await user.click(screen.getByRole("button", { name: "Reclaim disk space" }));
    await user.click(screen.getByRole("button", { name: "Undo organizing" }));
    expect(p.onRebuild).toHaveBeenCalledOnce();
    expect(p.onOptimizeLinks).toHaveBeenCalledOnce();
    expect(p.onRestoreVanilla).toHaveBeenCalledOnce();
    expect(screen.getByRole("button", { name: "Undo organizing" })).toHaveClass("btn-danger");
  });

  it("a disabled control shows its reason as text under it", () => {
    const p = maintenance("Silo is reclaiming disk space.");
    for (const name of ["Re-read every mod", "Reclaim disk space", "Undo organizing"]) {
      const b = screen.getByRole("button", { name });
      expect(b).toBeDisabled();
      expect(b).toHaveAttribute("title", "Silo is reclaiming disk space.");
    }
    expect(screen.getAllByText("Silo is reclaiming disk space.")).toHaveLength(3);
    expect(p.onRestoreVanilla).not.toHaveBeenCalled();
  });

  it("with nothing organized, Reclaim and Undo aren't offered", () => {
    maintenance("", 0);
    expect(screen.queryByRole("button", { name: "Reclaim disk space" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Undo organizing" })).toBeNull();
    expect(screen.getByRole("button", { name: "Re-read every mod" })).toBeEnabled();
  });
});

describe("LibrarySettings", () => {
  it("mid-move, Organize and the archive's Change… are off and say why", () => {
    render(LibrarySettings, {
      autoFileNew: false,
      organizedCount: 2,
      unorganizedCount: 4,
      lockReason: "",
      scanning: false,
      archive: { path: "D:/silo-archive", isDefault: false, available: true, moving: null, running: true },
      moveUnfinished: true,
      archiveLoadFailed: false,
      onSetAutoFile: vi.fn(),
      onOrganize: vi.fn(),
      onChangeArchive: vi.fn(),
      onArchiveDefault: vi.fn(),
    });
    expect(screen.getByRole("button", { name: "Organize 4" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Change…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Use default" })).toBeDisabled();
    expect(screen.getAllByText(MOVE_UNFINISHED_REASON).length).toBeGreaterThanOrEqual(2);
    expect(screen.getByRole("switch", { name: "Auto-file new mods" })).toHaveAttribute("aria-checked", "false");
  });
});

describe("ModsFolderSettings", () => {
  it("shows the folder, its source and the game install; Change… calls back", async () => {
    const user = userEvent.setup();
    const onChangeModsFolder = vi.fn();
    render(ModsFolderSettings, {
      roots: ["C:/mods"],
      modsStatus: {
        path: "C:/mods",
        source: "default",
        gameReads: "C:/mods",
        mismatch: false,
        available: true,
        modsFolderMissing: false,
        known: [],
        canWriteGameSetting: true,
        gameSettingProblem: null,
        unresolvedOverride: null,
        layoutProblem: null,
        layoutProblemCause: null,
        layoutConflict: null,
      },
      gameInfo: null,
      lockReason: "",
      scanning: false,
      moveUnfinished: false,
      onChangeModsFolder,
      onUseGameFolder: vi.fn(),
      onSwitchModsFolder: vi.fn(),
    });
    expect(screen.getByText("C:/mods")).toBeInTheDocument();
    expect(screen.getByText("FS25 default")).toBeInTheDocument();
    expect(screen.getByText(/install not found/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Change…" }));
    expect(onChangeModsFolder).toHaveBeenCalledOnce();
  });
});

describe("GeneralSettings", () => {
  it("offers the theme as System / Light / Dark", async () => {
    const user = userEvent.setup();
    const onSetTheme = vi.fn();
    render(GeneralSettings, {
      theme: "system",
      uiScale: 1,
      userDir: null,
      onSetTheme,
      onSetUiScale: vi.fn(),
      onOpenGameGraphics: vi.fn(),
    });
    const group = screen.getByRole("radiogroup", { name: "Theme" });
    expect([...group.querySelectorAll("[role=radio]")].map((r) => r.textContent?.trim())).toEqual([
      "System",
      "Light",
      "Dark",
    ]);
    await user.click(screen.getByRole("radio", { name: "Dark" }));
    expect(onSetTheme).toHaveBeenCalledWith("dark");
    expect(screen.getByLabelText("UI size")).toHaveValue("1");
  });
});

describe("DiagnosticsSettings", () => {
  it("Open log folder asks Rust for the logs folder; a failure shows under the card", async () => {
    const user = userEvent.setup();
    const onCopyDiagnostics = vi.fn();
    render(DiagnosticsSettings, { onCopyDiagnostics, onExportReport: vi.fn() });
    await user.click(screen.getByRole("button", { name: "Open log folder" }));
    expect(openKnownFolder).toHaveBeenCalledWith("logs");

    vi.mocked(openKnownFolder).mockRejectedValueOnce("That folder isn't there right now.");
    await user.click(screen.getByRole("button", { name: "Open log folder" }));
    expect(await screen.findByText(/That folder isn't there right now\./)).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Copy diagnostics" }));
    expect(onCopyDiagnostics).toHaveBeenCalledOnce();
  });
});
