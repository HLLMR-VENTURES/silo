// @vitest-environment jsdom
// The message model's views: NoticeArea, ActivityButton + ActivityPanel, ViewBoundary, the
// global error handlers, and the toast region's cap / count / Copy details / sticky errors.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(() => Promise.resolve()),
}));
import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { act, fireEvent, render, screen, within } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { AppError } from "../errors";
import { activity, clearActivity, error, info, notices, setNotice } from "../state/notify.svelte";
import { running, start } from "../state/ops.svelte";
import { installGlobalErrorHandlers } from "../state/globalErrors";
import ToastRegion from "../ui/ToastRegion.svelte";
import { dismissToast, toasts } from "../ui/toasts.svelte";
import ActivityButton from "./ActivityButton.svelte";
import NoticeArea from "./NoticeArea.svelte";
import BoundaryHarness from "./__tests__/BoundaryHarness.svelte";
import { failing } from "./__tests__/thrower";

let focusAnchor: HTMLButtonElement;

beforeEach(() => {
  focusAnchor = document.createElement("button");
  document.body.append(focusAnchor);
  focusAnchor.focus();
  vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  for (const t of [...toasts]) dismissToast(t.id);
  notices.length = 0;
  running.length = 0;
  clearActivity();
  focusAnchor.remove();
  vi.mocked(rawInvoke).mockClear();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

describe("NoticeArea", () => {
  it("renders nothing without notices", () => {
    const { container } = render(NoticeArea);
    expect(container.querySelector(".notice-area")).toBeNull();
  });

  it("shows the top notice with body, progress and actions (primary first)", async () => {
    const user = userEvent.setup();
    const later = vi.fn();
    const close = vi.fn();
    render(NoticeArea);
    await act(() =>
      setNotice({
        key: "move",
        level: "warning",
        title: "Moving your library",
        body: "Keep Silo open until it finishes.",
        progress: { done: 3, total: 12, label: "3 of 12 mods" },
        actions: [
          { label: "Later", run: later },
          { label: "Close the game", run: close, primary: true },
        ],
        dismissible: false,
      }),
    );
    const strip = screen.getByRole("status");
    expect(strip).toHaveTextContent("Moving your library");
    expect(strip).toHaveTextContent("Keep Silo open until it finishes.");
    expect(within(strip).getByRole("progressbar", { name: "3 of 12 mods" })).toHaveAttribute("aria-valuenow", "3");
    const buttons = within(strip).getAllByRole("button");
    expect(buttons.map((b) => b.textContent?.trim())).toEqual(["Close the game", "Later"]);
    expect(within(strip).queryByRole("button", { name: /Dismiss/ })).toBeNull();
    await user.click(buttons[0]);
    await vi.waitFor(() => expect(close).toHaveBeenCalledTimes(1));
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("blocking notices are alerts; dismissing one logs it", async () => {
    const user = userEvent.setup();
    render(NoticeArea);
    await act(() =>
      setNotice({ key: "game", level: "blocking", title: "Farming Simulator 25 is running", dismissible: true }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("Farming Simulator 25 is running");
    await user.click(screen.getByRole("button", { name: "Dismiss: Farming Simulator 25 is running" }));
    expect(screen.queryByRole("alert")).toBeNull();
    expect(activity[0]).toMatchObject({ kind: "notice", message: "Farming Simulator 25 is running" });
  });

  it("'+N more' expands the rest in place", async () => {
    const user = userEvent.setup();
    render(NoticeArea);
    await act(() => {
      setNotice({ key: "a", level: "info", title: "Update available", dismissible: true });
      setNotice({ key: "b", level: "warning", title: "Archive drive missing", dismissible: false });
      setNotice({ key: "c", level: "info", title: "Scan finished", dismissible: true });
    });
    const more = screen.getByRole("button", { name: "+2 more" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Archive drive missing")).toBeVisible();
    expect(screen.getAllByRole("status")).toHaveLength(1);
    await user.click(more);
    expect(more).toHaveAttribute("aria-expanded", "true");
    expect(more).toHaveTextContent("Show less");
    const shown = screen.getAllByRole("status").map((s) => s.querySelector(".title")?.textContent);
    expect(shown).toEqual(["Archive drive missing", "Scan finished", "Update available"]);
  });

  it("a pinned notice (a ready Silo update) is never folded under '+N more'", async () => {
    render(NoticeArea);
    await act(() => {
      setNotice({ key: "w", level: "warning", title: "Archive drive missing", dismissible: false });
      setNotice({ key: "self-update", level: "info", title: "Silo 0.12.0 is ready", pinned: true, dismissible: true });
      setNotice({ key: "i", level: "info", title: "Scan finished", dismissible: true });
    });
    expect(screen.getByText("Archive drive missing")).toBeVisible();
    expect(screen.getByText("Silo 0.12.0 is ready")).toBeVisible();
    expect(screen.getByText("Scan finished")).not.toBeVisible();
    expect(screen.getByRole("button", { name: "+1 more" })).toBeInTheDocument();
  });

  it("every blocking notice and every notice with progress stays visible; '+N more' resets", async () => {
    const user = userEvent.setup();
    render(NoticeArea);
    await act(() => {
      setNotice({ key: "a", level: "blocking", title: "Mods folder missing", dismissible: false });
      setNotice({ key: "b", level: "blocking", title: "A diagnosis was interrupted", dismissible: false });
      setNotice({
        key: "c",
        level: "info",
        title: "Moving your library",
        progress: { done: 1, total: 4, label: "1 of 4 mods" },
        dismissible: false,
      });
      setNotice({ key: "d", level: "info", title: "Database restored", dismissible: true });
    });
    expect(screen.getAllByRole("alert")).toHaveLength(2);
    expect(screen.getByText("Moving your library")).toBeVisible();
    expect(screen.getByText("Database restored")).not.toBeVisible();
    await user.click(screen.getByRole("button", { name: "+1 more" }));
    expect(screen.getByText("Database restored")).toBeVisible();
    await user.click(screen.getByRole("button", { name: "Dismiss: Database restored" }));
    expect(screen.queryByRole("button", { name: /more|Show less/ })).toBeNull();
    // The next notice to land behind "+N more" starts collapsed.
    await act(() => setNotice({ key: "e", level: "info", title: "Scan finished", dismissible: true }));
    expect(screen.getByRole("button", { name: "+1 more" })).toHaveAttribute("aria-expanded", "false");
    expect(screen.getByText("Scan finished")).not.toBeVisible();
  });
});

describe("ActivityButton", () => {
  it("shows an empty state when nothing has happened", async () => {
    const user = userEvent.setup();
    render(ActivityButton);
    const button = screen.getByRole("button", { name: "Activity" });
    expect(button).toHaveAttribute("aria-expanded", "false");
    await user.click(button);
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByRole("dialog", { name: "Activity" })).toHaveTextContent(
      "Nothing has happened yet this session.",
    );
  });

  it("flags running ops and unseen errors, lists them, and clears", async () => {
    const user = userEvent.setup();
    render(ActivityButton);
    const op = start("Installing FS25_Foo", { progress: true });
    op.progress(2, 4, "2 of 4 files");
    await act();
    expect(screen.getByRole("button", { name: "Activity, 1 running" })).toBeInTheDocument();

    await act(() => {
      info("Mod set saved");
      error(new AppError("GAME_RUNNING", "running"));
      error(new AppError("GAME_RUNNING", "running"));
    });
    const button = screen.getByRole("button", { name: "Activity, 1 running, 1 new error" });
    expect(document.querySelector(".dot-error")).not.toBeNull();

    await user.click(button);
    const panel = screen.getByRole("dialog", { name: "Activity" });
    expect(within(panel).getByRole("progressbar", { name: "Installing FS25_Foo: 2 of 4 files" })).toBeInTheDocument();
    const items = within(panel).getAllByRole("listitem").map((li) => li.textContent ?? "");
    expect(items[1]).toMatch(/^\d\d:\d\d.*Close Farming Simulator 25 first, then try again\..*×2.*Copy details/);
    expect(items[2]).toMatch(/Mod set saved/);
    expect(within(panel).getAllByRole("button", { name: "Copy details" })).toHaveLength(1);
    expect(screen.getByRole("button", { name: "Activity, 1 running" })).toBeInTheDocument();

    await user.click(within(panel).getByRole("button", { name: "Copy details" }));
    await vi.waitFor(() =>
      expect(rawInvoke).toHaveBeenCalledWith("copy_text", { text: expect.stringContaining("Code: GAME_RUNNING") }),
    );

    await user.click(within(panel).getByRole("button", { name: "Clear" }));
    expect(activity).toEqual([]);
    op.end();
    await act();
    // The finished op is the only entry now.
    expect(within(panel).getAllByRole("listitem")).toHaveLength(1);
    expect(panel).toHaveTextContent("Installing FS25_Foo");
  });
});

describe("ViewBoundary", () => {
  it("catches a render error, logs it quietly, and reloads the view", async () => {
    const user = userEvent.setup();
    failing.now = true;
    render(BoundaryHarness);
    expect(screen.getByRole("heading", { name: "This part of Silo hit a problem." })).toBeInTheDocument();
    // No claim about the mods unless the view says it can't change any.
    expect(screen.getByText("Something went wrong showing this part of Silo.")).toBeInTheDocument();
    expect(screen.queryByText("Your mods weren't changed.")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(screen.getByRole("button", { name: "Copy details" })).toBeInTheDocument();
    expect(toasts).toEqual([]);
    expect(activity[0]).toMatchObject({ level: "error", message: "Something went wrong while showing Library." });
    expect(rawInvoke).toHaveBeenCalledWith(
      "log_frontend_error",
      expect.objectContaining({ message: "showing Library: render blew up" }),
    );

    failing.now = false;
    await user.click(screen.getByRole("button", { name: "Reload this view" }));
    expect(screen.getByText("Library content")).toBeInTheDocument();
  });
});

describe("ViewBoundary as an overlay", () => {
  it("shows the failure as a fixed card with Close and Reload, and its own body text", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    failing.now = true;
    render(BoundaryHarness, { overlay: true, onClose, bodyText: "Your mods weren't changed." });
    const card = screen.getByRole("alertdialog", { name: "Library hit a problem" });
    expect(card).toHaveClass("boundary-overlay");
    expect(within(card).getByText("Your mods weren't changed.")).toBeInTheDocument();
    await user.click(within(card).getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledTimes(1);
    failing.now = false;
    await user.click(within(card).getByRole("button", { name: "Reload this view" }));
    expect(screen.getByText("Library content")).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });
});

describe("installGlobalErrorHandlers", () => {
  it("reports uncaught errors and rejections quietly, ignores ResizeObserver noise, and uninstalls", async () => {
    const uninstall = installGlobalErrorHandlers();
    window.dispatchEvent(new ErrorEvent("error", { message: "ResizeObserver loop completed with undelivered notifications." }));
    expect(activity).toEqual([]);
    window.dispatchEvent(new ErrorEvent("error", { message: "x is undefined", error: new TypeError("x is undefined") }));
    const rejection = new Event("unhandledrejection") as PromiseRejectionEvent;
    Object.defineProperty(rejection, "reason", { value: new AppError("NET_OFFLINE", "dns error") });
    window.dispatchEvent(rejection);
    expect(activity.map((a) => a.message)).toEqual([
      "Silo can't reach the internet right now.",
      "Something went wrong while running Silo.",
    ]);
    expect(toasts).toEqual([]);
    expect(rawInvoke).toHaveBeenCalledWith("app_version", undefined);

    uninstall();
    window.dispatchEvent(new ErrorEvent("error", { message: "later" }));
    expect(activity).toHaveLength(2);
  });
});

describe("ToastRegion with the message model", () => {
  it("shows three at a time and brings in the next when one leaves", async () => {
    const user = userEvent.setup();
    render(ToastRegion);
    await act(() => {
      for (let i = 1; i <= 4; i++) info(`Toast ${i}`);
    });
    // The newest always shows; the one before it waits.
    expect(screen.getByText("Toast 4")).toBeInTheDocument();
    expect(screen.queryByText("Toast 3")).toBeNull();
    await user.click(screen.getAllByRole("button", { name: "Dismiss" })[0]);
    expect(screen.getByText("Toast 3")).toBeInTheDocument();
  });

  it("errors stay, count repeats, offer Try again and Copy details, in the polite region", async () => {
    vi.useFakeTimers();
    const retry = vi.fn();
    render(ToastRegion);
    await act(() => {
      error(new AppError("LIBRARY_BUSY", "busy"), { op: "Moving mods", retry });
      error(new AppError("LIBRARY_BUSY", "busy"), { op: "Moving mods", retry });
    });
    const region = screen.getByRole("status");
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(screen.queryByRole("alert")).toBeNull();
    expect(region).toHaveTextContent("×2");
    await act(() => vi.advanceTimersByTime(60_000));
    expect(region).toHaveTextContent("Silo is finishing another change to your library.");
    vi.useRealTimers();

    await fireEvent.click(screen.getByRole("button", { name: "Copy details" }));
    await vi.waitFor(() =>
      expect(rawInvoke).toHaveBeenCalledWith("copy_text", { text: expect.stringContaining("While: Moving mods") }),
    );
    await vi.waitFor(() => expect(screen.getByRole("button", { name: "Copied" })).toBeInTheDocument());

    await fireEvent.click(screen.getByRole("button", { name: /^Try again: / }));
    await vi.waitFor(() => expect(retry).toHaveBeenCalledTimes(1));
    expect(toasts).toEqual([]);
  });
});
