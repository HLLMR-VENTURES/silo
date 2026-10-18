// @vitest-environment jsdom
// MenuButton + Menu keyboard behavior: arrows, Home/End, type-ahead, Enter, Esc, outside click.
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import MenuButton from "./MenuButton.svelte";
import type { MenuItem } from "./overlayTypes";

const items: MenuItem[] = [
  { id: "open", label: "Open folder", icon: "showFile" },
  { id: "copy", label: "Copy name", shortcut: "Ctrl+C" },
  { id: "hide", label: "Hide", disabled: true },
  { id: "check", label: "Check for updates" },
  { id: "delete", label: "Delete", danger: true, separatorBefore: true },
];

async function openMenu() {
  const user = userEvent.setup();
  const onselect = vi.fn();
  render(MenuButton, { label: "Mod actions", items, onselect });
  const trigger = screen.getByRole("button", { name: "Mod actions" });
  expect(trigger).toHaveAttribute("aria-haspopup", "menu");
  expect(trigger).toHaveAttribute("aria-expanded", "false");
  await user.click(trigger);
  const menu = await screen.findByRole("menu", { name: "Mod actions" });
  const item = (name: string) => screen.getByRole("menuitem", { name: new RegExp(`^${name}`) });
  await waitFor(() => expect(item("Open folder")).toHaveFocus());
  return { user, onselect, trigger, menu, item };
}

describe("Menu", () => {
  it("opens with the first enabled item focused and aria-expanded set", async () => {
    const { trigger, menu } = await openMenu();
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(trigger).toHaveAttribute("aria-controls", menu.id);
    expect(screen.getAllByRole("menuitem")).toHaveLength(5);
  });

  it("ArrowDown/Up move focus, skip disabled items and wrap", async () => {
    const { user, item } = await openMenu();
    await user.keyboard("{ArrowDown}");
    expect(item("Copy name")).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(item("Check for updates")).toHaveFocus();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    expect(item("Open folder")).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(item("Delete")).toHaveFocus();
  });

  it("Home and End jump to the ends", async () => {
    const { user, item } = await openMenu();
    await user.keyboard("{End}");
    expect(item("Delete")).toHaveFocus();
    await user.keyboard("{Home}");
    expect(item("Open folder")).toHaveFocus();
  });

  it("type-ahead jumps by first letters", async () => {
    const { user, item } = await openMenu();
    await user.keyboard("c");
    expect(item("Copy name")).toHaveFocus();
    await user.keyboard("c");
    expect(item("Check for updates")).toHaveFocus();
    await user.keyboard("d");
    // "ccd" matches nothing, so focus stays.
    expect(item("Check for updates")).toHaveFocus();
  });

  it("type-ahead buffer resets after 500 ms", async () => {
    vi.useFakeTimers();
    try {
      const user = userEvent.setup({ advanceTimers: vi.advanceTimersByTime });
      const onselect = vi.fn();
      render(MenuButton, { label: "Mod actions", items, onselect });
      await user.click(screen.getByRole("button", { name: "Mod actions" }));
      await vi.advanceTimersByTimeAsync(0);
      await user.keyboard("d");
      expect(screen.getByRole("menuitem", { name: "Delete" })).toHaveFocus();
      vi.advanceTimersByTime(600);
      await user.keyboard("o");
      expect(screen.getByRole("menuitem", { name: "Open folder" })).toHaveFocus();
    } finally {
      vi.useRealTimers();
    }
  });

  it("Enter selects the focused item, closes, and refocuses the trigger", async () => {
    const { user, onselect, trigger } = await openMenu();
    await user.keyboard("{ArrowDown}{Enter}");
    expect(onselect).toHaveBeenCalledWith("copy");
    expect(onselect).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
  });

  it("disabled items can't be chosen by click", async () => {
    const { user, onselect } = await openMenu();
    await user.click(screen.getByRole("menuitem", { name: "Hide" }));
    expect(onselect).not.toHaveBeenCalled();
    expect(screen.getByRole("menu")).toBeInTheDocument();
  });

  it("Esc closes and returns focus to the trigger", async () => {
    const { user, onselect, trigger } = await openMenu();
    await user.keyboard("{ArrowDown}{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(trigger).toHaveFocus();
    expect(trigger).toHaveAttribute("aria-expanded", "false");
    expect(onselect).not.toHaveBeenCalled();
  });

  it("a pointerdown outside closes it", async () => {
    const { user } = await openMenu();
    await user.click(document.body);
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
  });
});
