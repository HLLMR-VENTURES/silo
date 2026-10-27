// @vitest-environment jsdom
// toast() + ToastRegion: rendering, action, auto-dismiss, pause on hover / focus / window blur,
// and inert buttons while a dialog is open.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { pushLayer } from "./overlayStack";
import ToastRegion from "./ToastRegion.svelte";
import { dismissToast, toast, toasts } from "./toasts.svelte";

let focusAnchor: HTMLButtonElement;

beforeEach(() => {
  // jsdom's document.hasFocus() is false until something has been focused.
  focusAnchor = document.createElement("button");
  document.body.append(focusAnchor);
  focusAnchor.focus();
});

afterEach(() => {
  for (const t of [...toasts]) dismissToast(t.id);
  focusAnchor.remove();
  vi.useRealTimers();
});

describe("toast", () => {
  it("renders in a polite status region without taking focus", async () => {
    render(ToastRegion);
    await act(() => toast({ message: "Mod set saved", tone: "success" }));
    const region = screen.getByRole("status");
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(region).toHaveTextContent("Mod set saved");
    expect(focusAnchor).toHaveFocus();
  });

  it("the action runs and dismisses the toast", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    render(ToastRegion);
    await act(() => toast({ message: "Moved 12 mods", action: { label: "Undo", run } }));
    await user.click(screen.getByRole("button", { name: "Undo: Moved 12 mods" }));
    await vi.waitFor(() => expect(run).toHaveBeenCalledTimes(1));
    expect(screen.queryByText("Moved 12 mods")).not.toBeInTheDocument();
  });

  it("a second action shows before the first, and runs on its own", async () => {
    const user = userEvent.setup();
    const park = vi.fn();
    const undo = vi.fn();
    render(ToastRegion);
    await act(() =>
      toast({
        message: "Removed 2 mods from 3 sets.",
        secondaryAction: { label: "Park them too", run: park },
        action: { label: "Undo", run: undo },
      }),
    );
    const buttons = screen.getAllByRole("button").map((b) => b.textContent?.trim());
    expect(buttons.indexOf("Park them too")).toBeLessThan(buttons.indexOf("Undo"));
    await user.click(screen.getByRole("button", { name: "Park them too: Removed 2 mods from 3 sets." }));
    await vi.waitFor(() => expect(park).toHaveBeenCalledTimes(1));
    expect(undo).not.toHaveBeenCalled();
    expect(screen.queryByText("Removed 2 mods from 3 sets.")).not.toBeInTheDocument();
  });

  it("names the action after the toast, or uses its ariaLabel", async () => {
    render(ToastRegion);
    await act(() => {
      toast({ message: "Switched to Harvest.", action: { label: "Undo", run: () => {} } });
      const restore = { label: "Undo", ariaLabel: "Restore Harvest", run: () => {} };
      toast({ message: "Deleted Harvest.", action: restore });
    });
    const named = screen.getByRole("button", { name: "Undo: Switched to Harvest." });
    expect(named).toHaveTextContent("Undo");
    expect(screen.getByRole("button", { name: "Restore Harvest" })).toBeInTheDocument();
  });

  it("its buttons are inert while a modal is open; the text stays announced", async () => {
    const user = userEvent.setup();
    const run = vi.fn();
    render(ToastRegion);
    await act(() => toast({ message: "Moved 12 mods", action: { label: "Undo", run } }));
    const undo = screen.getByRole("button", { name: "Undo: Moved 12 mods" });
    const close = screen.getByRole("button", { name: "Dismiss" });
    expect(undo.closest("[inert]")).toBeNull();

    const host = document.createElement("div");
    document.body.append(host);
    const layer = pushLayer({ host, modal: true });
    await act();
    expect(undo.closest("[inert]")).not.toBeNull();
    expect(close.closest("[inert]")).not.toBeNull();
    const region = screen.getByRole("status");
    expect(region).not.toHaveAttribute("inert");
    expect(region).toHaveTextContent("Moved 12 mods");
    await user.click(undo); // jsdom ignores inert; the toast refuses the click itself too
    expect(run).not.toHaveBeenCalled();

    layer.pop();
    host.remove();
    await act();
    expect(undo.closest("[inert]")).toBeNull();
    await user.click(undo);
    await vi.waitFor(() => expect(run).toHaveBeenCalledTimes(1));
  });

  it("the close button and the returned handle dismiss it", async () => {
    const user = userEvent.setup();
    render(ToastRegion);
    await act(() => toast({ message: "First" }));
    const second = toast({ message: "Second" });
    await act();
    await user.click(screen.getAllByRole("button", { name: "Dismiss" })[0]);
    expect(screen.queryByText("First")).not.toBeInTheDocument();
    await act(() => second.dismiss());
    expect(screen.queryByText("Second")).not.toBeInTheDocument();
  });

  it("auto-dismisses after its duration (6 s default, 10 s with an action)", async () => {
    vi.useFakeTimers();
    render(ToastRegion);
    await act(() => {
      toast({ message: "Plain" });
      toast({ message: "With action", action: { label: "Undo", run: () => {} } });
    });
    await act(() => vi.advanceTimersByTime(5_900));
    expect(screen.getByText("Plain")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTime(200));
    expect(screen.queryByText("Plain")).not.toBeInTheDocument();
    expect(screen.getByText("With action")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTime(4_000));
    expect(screen.queryByText("With action")).not.toBeInTheDocument();
  });

  it("pauses while hovered and resumes with the time left", async () => {
    vi.useFakeTimers();
    render(ToastRegion);
    await act(() => toast({ message: "Hover me", duration: 1_000 }));
    await act(() => vi.advanceTimersByTime(600));
    const card = screen.getByText("Hover me").closest(".toast")!;
    await fireEvent.pointerEnter(card);
    await act(() => vi.advanceTimersByTime(5_000));
    expect(screen.getByText("Hover me")).toBeInTheDocument();
    await fireEvent.pointerLeave(card);
    await act(() => vi.advanceTimersByTime(300));
    expect(screen.getByText("Hover me")).toBeInTheDocument();
    await act(() => vi.advanceTimersByTime(200));
    expect(screen.queryByText("Hover me")).not.toBeInTheDocument();
  });

  it("pauses while the window is in the background", async () => {
    vi.useFakeTimers();
    render(ToastRegion);
    await act(() => toast({ message: "Background", duration: 1_000 }));
    await fireEvent.blur(window);
    await act(() => vi.advanceTimersByTime(5_000));
    expect(screen.getByText("Background")).toBeInTheDocument();
    await fireEvent.focus(window);
    await act(() => vi.advanceTimersByTime(1_000));
    expect(screen.queryByText("Background")).not.toBeInTheDocument();
  });

  it("pauses while focus is inside it", async () => {
    vi.useFakeTimers();
    render(ToastRegion);
    await act(() => toast({ message: "Focus me", duration: 1_000 }));
    await act(() => screen.getByRole("button", { name: "Dismiss" }).focus());
    await act(() => vi.advanceTimersByTime(5_000));
    expect(screen.getByText("Focus me")).toBeInTheDocument();
  });
});
