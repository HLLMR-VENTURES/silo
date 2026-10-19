// @vitest-environment jsdom
// #228 wiring: producers own their notice keys (a rescan clears only `scan`), notice actions can
// be off with a reason, a held library write turns controls off with its reason, and an error
// toast shows a backend message as its second line.
import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(() => Promise.resolve()),
}));
import { act, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { clearActivity, dismissNotice, error, notices, syncNotice } from "../state/notify.svelte";
import { running, start, tryStart } from "../state/ops.svelte";
import ToastRegion from "../ui/ToastRegion.svelte";
import { dismissToast, toasts } from "../ui/toasts.svelte";
import type { UpdateRow } from "../types";
import NoticeArea from "./NoticeArea.svelte";
import ActivityButton from "./ActivityButton.svelte";
import LockHarness from "./__tests__/LockHarness.svelte";

afterEach(() => {
  for (const t of [...toasts]) dismissToast(t.id);
  notices.length = 0;
  running.length = 0;
  clearActivity();
  vi.restoreAllMocks();
});

describe("notice keys", () => {
  it("a scan setting and clearing `scan` leaves the other producers' notices up", async () => {
    render(NoticeArea);
    await act(() => {
      syncNotice("mods-folder", {
        level: "blocking",
        title: "Your mods folder isn't available: E:/mods.",
        dismissible: false,
      });
      syncNotice("cloud", { level: "warning", title: "Cloud-synced folder", dismissible: true });
      syncNotice("scan", { level: "warning", title: "The scan failed", dismissible: true });
    });
    expect(notices.map((n) => n.key).sort()).toEqual(["cloud", "mods-folder", "scan"]);
    // The next scan works: it clears only its own key.
    await act(() => syncNotice("scan", null));
    expect(notices.map((n) => n.key).sort()).toEqual(["cloud", "mods-folder"]);
    expect(screen.getByRole("alert")).toHaveTextContent("Your mods folder isn't available: E:/mods.");
  });

  it("dismissing runs the notice's onDismiss", async () => {
    const remembered = vi.fn();
    const user = userEvent.setup();
    render(NoticeArea);
    await act(() =>
      syncNotice("cloud", {
        level: "warning",
        title: "Cloud-synced folder",
        dismissible: true,
        onDismiss: remembered,
      }),
    );
    await user.click(screen.getByRole("button", { name: "Dismiss: Cloud-synced folder" }));
    expect(notices).toHaveLength(0);
    expect(remembered).toHaveBeenCalledTimes(1);
  });

  it("an action can be off, saying why", async () => {
    const run = vi.fn();
    render(NoticeArea);
    await act(() =>
      syncNotice("bisect-recovery", {
        level: "blocking",
        title: "A guided diagnosis was interrupted.",
        actions: [{ label: "Restore my mod set", run, disabled: true, title: "Silo is filing 3 mods." }],
        dismissible: false,
      }),
    );
    const button = screen.getByRole("button", { name: "Restore my mod set" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Silo is filing 3 mods.");
  });
});

describe("the library-write lock on a control", () => {
  const row: UpdateRow = {
    techName: "FS25_X",
    title: "Big Tractor",
    path: "C:/mods/FS25_X.zip",
    current: "1.0",
    latest: "1.1",
    hasUpdate: true,
    assetUrl: "https://example.com/x.zip",
  };

  it("turns Update off with the running write's reason, and back on when it ends", async () => {
    render(LockHarness, { row, onInstall: () => {} });
    const install = screen.getByRole("button", { name: "Update Big Tractor" });
    const all = screen.getByRole("button", { name: /Update all 1/ });
    expect(install).toBeEnabled();
    expect(all).toBeEnabled();
    let op!: ReturnType<typeof start>;
    await act(() => {
      op = start("Filing 3 mods into the library", { exclusive: "library-write" });
    });
    expect(install).toBeDisabled();
    expect(all).toBeDisabled();
    expect(screen.getByText("Silo is filing 3 mods into the library.")).toBeInTheDocument();
    // A second write is refused while the first holds the class.
    expect(tryStart("Installing Big Tractor", { exclusive: "library-write" })).toBeNull();
    await act(() => op.end());
    expect(install).toBeEnabled();
    expect(screen.queryByText("Silo is filing 3 mods into the library.")).toBeNull();
  });

  it("a background op (the archive move) leaves the panel usable", async () => {
    render(LockHarness, { row, onInstall: () => {} });
    await act(() => {
      start("Moving your library", { exclusive: "library-write", background: true });
    });
    expect(screen.getByRole("button", { name: "Update Big Tractor" })).toBeEnabled();
    // ...but a write still can't start beside it.
    expect(tryStart("Installing Big Tractor", { exclusive: "library-write" })).toBeNull();
  });

  it("a plain (read) op doesn't lock anything", async () => {
    render(LockHarness, { row, onInstall: () => {} });
    await act(() => {
      start("Checking the folder");
    });
    expect(screen.getByRole("button", { name: "Update Big Tractor" })).toBeEnabled();
  });
});

describe("error toast body", () => {
  it("shows a backend message under the generic title", async () => {
    render(ToastRegion);
    await act(() => {
      error("That loadout was deleted in another window.", { op: "Saving the loadout" });
    });
    expect(screen.getByText("Something went wrong while saving the loadout.")).toBeInTheDocument();
    expect(screen.getByText("That loadout was deleted in another window.")).toHaveClass("body");
    expect(screen.getByRole("button", { name: "Copy details" })).toBeInTheDocument();
  });

  it("keeps technical text out of the toast", async () => {
    render(ToastRegion);
    await act(() => {
      error("weird failure (os error 1234)", { op: "Saving the loadout" });
    });
    expect(screen.getByText("Something went wrong while saving the loadout.")).toBeInTheDocument();
    expect(screen.queryByText(/os error/)).toBeNull();
  });
});

describe("Activity popover focus", () => {
  it("Esc from inside the panel hands focus back to the bell, not <body>", async () => {
    const user = userEvent.setup();
    render(ActivityButton);
    const bell = screen.getByRole("button", { name: /^Activity/ });
    await user.click(bell);
    const panel = await screen.findByRole("dialog", { name: "Activity" });
    panel.focus();
    expect(panel).toHaveFocus();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog", { name: "Activity" })).toBeNull();
    expect(bell).toHaveFocus();
  });
});
