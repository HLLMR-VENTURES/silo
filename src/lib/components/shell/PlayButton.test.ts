// @vitest-environment jsdom
// PlayButton: launch, disabled with a reason, loading, and the launch menu.
import { describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import PlayButton from "./PlayButton.svelte";
import type { PlayMenuItem } from "./PlayButton.svelte";

const menuItems: PlayMenuItem[] = [
  { id: "save3", label: "Savegame 3 needs 2 parked mods: Turn them on" },
  { id: "locked", label: "Nothing here", disabled: true },
  { id: "folder", label: "Open the game folder", icon: "showFile" },
];

function setup(props: Record<string, unknown> = {}) {
  const onplay = vi.fn();
  const onmenuselect = vi.fn();
  render(PlayButton, { count: 142, onplay, onmenuselect, menuItems, ...props });
  const main = screen.getByRole("button", { name: /^Play/ });
  const chevron = screen.getByRole("button", { name: "More launch options" });
  return { onplay, onmenuselect, main, chevron, user: userEvent.setup() };
}

describe("PlayButton", () => {
  it("shows the count and launches on click", async () => {
    const { main, onplay, user } = setup();
    expect(main).toHaveTextContent("Play · 142");
    expect(main).toHaveAccessibleName("Play 142 mods");
    await user.click(main);
    expect(onplay).toHaveBeenCalledTimes(1);
  });

  it("formats large counts", () => {
    const { main } = setup({ count: 1204 });
    expect(main).toHaveTextContent("1,204");
  });

  it("disabled: stays focusable, does not launch, and describes the reason", async () => {
    const { main, onplay, user } = setup({ disabled: true, disabledReason: "Silo is moving mods." });
    expect(main).toHaveAttribute("aria-disabled", "true");
    expect(main).toHaveAccessibleDescription("Silo is moving mods.");
    await user.click(main);
    expect(onplay).not.toHaveBeenCalled();
    main.blur();
    await user.tab();
    expect(main).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(onplay).not.toHaveBeenCalled();
    const tip = await screen.findByRole("tooltip");
    expect(tip).toHaveTextContent("Silo is moving mods.");
  });

  it("loading: busy and inert", async () => {
    const { main, onplay, user } = setup({ loading: true });
    expect(main).toHaveAttribute("aria-busy", "true");
    await user.click(main);
    expect(onplay).not.toHaveBeenCalled();
  });

  it("the menu opens with the active set line as a heading, not an item", async () => {
    const { chevron, user } = setup({ activeSetLine: "Harvest 2026, unchanged" });
    expect(chevron).toHaveAttribute("aria-haspopup", "menu");
    expect(chevron).toHaveAttribute("aria-expanded", "false");
    await user.click(chevron);
    const menu = await screen.findByRole("menu", { name: "Launch options" });
    expect(chevron).toHaveAttribute("aria-expanded", "true");
    expect(menu).toHaveAccessibleDescription("Harvest 2026, unchanged");
    expect(screen.getAllByRole("menuitem")).toHaveLength(3);
    expect(screen.queryByRole("menuitem", { name: /Harvest 2026/ })).toBeNull();
    await waitFor(() => expect(screen.getByRole("menuitem", { name: /^Savegame 3/ })).toHaveFocus());
  });

  it("selects by click, closes and refocuses the chevron", async () => {
    const { chevron, onmenuselect, user } = setup();
    await user.click(chevron);
    await user.click(await screen.findByRole("menuitem", { name: "Open the game folder" }));
    expect(onmenuselect).toHaveBeenCalledWith("folder");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(chevron).toHaveFocus();
  });

  it("keyboard: ArrowDown opens, arrows skip disabled items, Enter selects", async () => {
    const { chevron, onmenuselect, user } = setup();
    chevron.focus();
    await user.keyboard("{ArrowDown}");
    await waitFor(() => expect(screen.getByRole("menuitem", { name: /^Savegame 3/ })).toHaveFocus());
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: "Open the game folder" })).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(screen.getByRole("menuitem", { name: /^Savegame 3/ })).toHaveFocus();
    await user.keyboard("{End}{Enter}");
    expect(onmenuselect).toHaveBeenCalledWith("folder");
  });

  it("disabled items cannot be chosen; the menu works while Play is disabled", async () => {
    const { chevron, onmenuselect, user } = setup({ disabled: true, disabledReason: "Blocked" });
    await user.click(chevron);
    await user.click(await screen.findByRole("menuitem", { name: "Nothing here" }));
    expect(onmenuselect).not.toHaveBeenCalled();
    await user.click(screen.getByRole("menuitem", { name: /^Savegame 3/ }));
    expect(onmenuselect).toHaveBeenCalledWith("save3");
  });

  it("Esc closes the menu", async () => {
    const { chevron, user } = setup();
    await user.click(chevron);
    await screen.findByRole("menu");
    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).not.toBeInTheDocument());
    expect(chevron).toHaveFocus();
  });
});
