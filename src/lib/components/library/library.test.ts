// @vitest-environment jsdom
// The library's virtual list/grid (keyed rendering, roving focus, the keys) and its rows and cards
// (the Active switch, parked styling, capped badges, the ⋯ menu). #242
import { beforeAll, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ListHarness from "./__tests__/ListHarness.svelte";
import GridHarness from "./__tests__/GridHarness.svelte";
import ModListRow from "./ModListRow.svelte";
import ModCard from "./ModCard.svelte";
import { rowBadges } from "./rowBadges";
import type { ModEntry } from "../../types";

beforeAll(() => {
  // jsdom has no ResizeObserver; Svelte's bind:clientWidth/Height needs one.
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});

const items = Array.from({ length: 200 }, (_, i) => ({ id: `k${i}`, name: `Item ${i}` }));

function itemEl(container: HTMLElement, key: string): HTMLElement | null {
  return container.querySelector<HTMLElement>(`[role="listitem"][data-key="${key}"]`);
}
function viewportOf(container: HTMLElement): HTMLElement {
  return container.querySelector<HTMLElement>(".vg-viewport")!;
}
async function scrollTo(container: HTMLElement, top: number) {
  const vp = viewportOf(container);
  vp.scrollTop = top;
  await fireEvent.scroll(vp);
  await tick();
}
async function settle() {
  await tick();
  await tick();
}

describe("VirtualList", () => {
  it("is a named list of listitems with set size and position", () => {
    const { container } = render(ListHarness, { items });
    expect(screen.getByRole("list", { name: "Things" })).toBeInTheDocument();
    const first = itemEl(container, "k0")!;
    expect(first).toHaveAttribute("aria-setsize", "200");
    expect(first).toHaveAttribute("aria-posinset", "1");
    // Virtualized: far rows aren't rendered.
    expect(itemEl(container, "k150")).toBeNull();
  });

  it("keeps the same DOM node for a row that stays on screen while scrolling", async () => {
    const { container } = render(ListHarness, { items });
    const before = itemEl(container, "k10");
    expect(before).not.toBeNull();
    await scrollTo(container, 560); // row 10 at the top
    expect(itemEl(container, "k10")).toBe(before);
    expect(itemEl(container, "k0")).toBeNull(); // scrolled out past the overscan
    expect(itemEl(container, "k25")).not.toBeNull();
  });

  it("has one tab stop and moves focus with the arrows, Home and End", async () => {
    const user = userEvent.setup();
    const { container } = render(ListHarness, { items });
    const tabbable = container.querySelectorAll('[role="listitem"][tabindex="0"]');
    expect(tabbable).toHaveLength(1);
    expect(tabbable[0]).toHaveAttribute("data-key", "k0");

    itemEl(container, "k0")!.focus();
    await user.keyboard("{ArrowDown}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k1"));
    expect(itemEl(container, "k1")).toHaveAttribute("tabindex", "0");
    expect(itemEl(container, "k0")).toHaveAttribute("tabindex", "-1");

    await user.keyboard("{End}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k199"));

    await user.keyboard("{Home}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k0"));

    await user.keyboard("{PageDown}");
    await settle();
    expect((document.activeElement as HTMLElement).dataset.key).not.toBe("k0");
  });

  it("Enter opens, Space toggles only when the row itself has focus", async () => {
    const user = userEvent.setup();
    const onopen = vi.fn();
    const ontoggle = vi.fn();
    const oninner = vi.fn();
    const { container } = render(ListHarness, { items, onopen, ontoggle, oninner });
    itemEl(container, "k0")!.focus();
    await user.keyboard("{Enter}");
    expect(onopen).toHaveBeenCalledWith(items[0]);
    await user.keyboard(" ");
    expect(ontoggle).toHaveBeenCalledTimes(1);
    expect(ontoggle).toHaveBeenCalledWith(items[0]);

    // Space on a control inside the row is the control's.
    screen.getByRole("button", { name: "Inner Item 0" }).focus();
    await user.keyboard(" ");
    expect(oninner).toHaveBeenCalledTimes(1);
    expect(ontoggle).toHaveBeenCalledTimes(1);
  });

  it("Shift+arrows select a range, Ctrl+Space toggles one, Shift+F10 opens the menu", async () => {
    const user = userEvent.setup();
    const onselect = vi.fn();
    const onmenu = vi.fn();
    const { container } = render(ListHarness, { items, onselect, onmenu });
    itemEl(container, "k0")!.focus();
    await user.keyboard("{Shift>}{ArrowDown}{/Shift}");
    await settle();
    expect(onselect).toHaveBeenLastCalledWith(items[1], { shift: true, ctrl: false, range: true, from: "k0" });
    expect(document.activeElement).toBe(itemEl(container, "k1"));

    await user.keyboard("{Control>} {/Control}");
    expect(onselect).toHaveBeenLastCalledWith(items[1], { shift: false, ctrl: true });

    await user.keyboard("{Shift>}{F10}{/Shift}");
    expect(onmenu).toHaveBeenCalledWith(items[1], itemEl(container, "k1"));
    await user.keyboard("{ContextMenu}");
    expect(onmenu).toHaveBeenCalledTimes(2);
  });

  it("keeps the focused row rendered when it scrolls away", async () => {
    const { container } = render(ListHarness, { items });
    const first = itemEl(container, "k0")!;
    first.focus();
    await tick();
    await scrollTo(container, 5000);
    expect(itemEl(container, "k0")).toBe(first);
    expect(first).toHaveAttribute("tabindex", "0");
    expect(document.activeElement).toBe(first);
  });

  it("focusKey moves focus to a row by key, scrolling it into view", async () => {
    const { container, component } = render(ListHarness, { items });
    expect(component.focusKey("nope")).toBe(false);
    expect(component.focusKey("k150")).toBe(true);
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k150"));
    expect(viewportOf(container).scrollTop).toBeGreaterThan(0);
  });
});

describe("VirtualGrid", () => {
  it("moves by one with ←/→ and by a row with ↑/↓", async () => {
    const user = userEvent.setup();
    const { container } = render(GridHarness, { items: items.slice(0, 30), cols: 3 });
    itemEl(container, "k0")!.focus();
    await user.keyboard("{ArrowRight}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k1"));
    await user.keyboard("{ArrowDown}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k4"));
    await user.keyboard("{ArrowLeft}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k3"));
    await user.keyboard("{ArrowUp}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k0"));
    await user.keyboard("{End}");
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k29"));
  });
});

function mod(over: Partial<ModEntry> = {}): ModEntry {
  return {
    techName: "FS25_Courseplay",
    path: "C:/mods/FS25_Courseplay.zip",
    kind: "zip",
    size: 6.2 * 1024 * 1024,
    mtimeMs: 0,
    title: "Courseplay",
    author: "Courseplay Dev Team",
    version: "8.0.0.4",
    descVersion: 96,
    iconFilename: null,
    isMap: false,
    mapTitle: null,
    category: "Scripts & Tools",
    subcategory: null,
    organized: true,
    active: true,
    dependencies: [],
    scriptCount: 0,
    registrationCount: 0,
    uniqueType: null,
    storeItemCount: 0,
    mpSupported: true,
    ignoredDigitPrefix: false,
    error: null,
    hasSettings: false,
    settingsSource: "none",
    ...over,
  };
}

const rowProps = (over: Record<string, unknown> = {}) => ({
  mod: mod(),
  active: true,
  badges: [],
  onToggle: vi.fn(),
  onOpen: vi.fn(),
  onMenu: vi.fn(),
  onSelect: vi.fn(),
  ...over,
});

describe("ModListRow", () => {
  it("has an Active switch named for the mod that only asks to flip", async () => {
    const props = rowProps();
    render(ModListRow, props);
    const sw = screen.getByRole("switch", { name: "Active: Courseplay" });
    expect(sw).toHaveAttribute("aria-checked", "true");
    expect(sw).toHaveAttribute("tabindex", "-1");
    await userEvent.click(sw);
    expect(props.onToggle).toHaveBeenCalledTimes(1);
    expect(props.onOpen).not.toHaveBeenCalled();
    // The parent owns the state: still on until `active` changes.
    expect(sw).toHaveAttribute("aria-checked", "true");
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  it("shows the meta line: author, version, category, size", () => {
    render(ModListRow, rowProps());
    expect(screen.getByText("Courseplay Dev Team")).toBeInTheDocument();
    expect(screen.getByText("8.0.0.4")).toBeInTheDocument();
    expect(screen.getByText("Scripts & Tools")).toBeInTheDocument();
    expect(screen.getByText("6 MB")).toBeInTheDocument();
  });

  it("says Parked without fading the text", () => {
    const { container } = render(ModListRow, rowProps({ active: false }));
    const sw = screen.getByRole("switch", { name: "Active: Courseplay" });
    expect(sw).toHaveAttribute("aria-checked", "false");
    const word = screen.getByText("Parked");
    const row = container.querySelector(".row") as HTMLElement;
    expect(row).toHaveClass("parked");
    expect(row).not.toHaveClass("dimmed");
    for (const el of [row, word, screen.getByText("Courseplay"), screen.getByText("Courseplay Dev Team")]) {
      expect(el.style.opacity).toBe("");
      const op = getComputedStyle(el).opacity;
      expect(op === "" || Number(op) === 1).toBe(true);
    }
  });

  it("caps the badges at three, then +n", () => {
    const badges = rowBadges(mod({ error: "bad zip", ignoredDigitPrefix: true, isMap: true, scriptCount: 2 }), {
      verdict: "modified",
      hasUpdate: true,
    });
    expect(badges.length).toBe(6);
    render(ModListRow, rowProps({ badges }));
    expect(screen.getByText("Modified")).toBeInTheDocument();
    expect(screen.getByText("Can't load")).toBeInTheDocument();
    expect(screen.getByText("Ignored name")).toBeInTheDocument();
    expect(screen.queryByText("Update")).toBeNull();
    expect(screen.getByText("+3")).toBeInTheDocument();
    expect(screen.getByText(/more: Update, Map, 2 scripts/)).toBeInTheDocument();
  });

  it("opens the menu from ⋯ (anchored) and right-click (at the pointer); body click opens details", async () => {
    const props = rowProps();
    const { container } = render(ModListRow, props);
    const more = screen.getByRole("button", { name: "More actions for Courseplay" });
    expect(more).toHaveAttribute("aria-haspopup", "menu");
    await userEvent.click(more);
    expect(props.onMenu).toHaveBeenLastCalledWith({ anchor: more });
    await fireEvent.contextMenu(container.querySelector(".row")!, { clientX: 40, clientY: 50 });
    expect(props.onMenu).toHaveBeenLastCalledWith({ point: { x: 40, y: 50 } });
    expect(props.onOpen).not.toHaveBeenCalled();
    await userEvent.click(screen.getByText("Courseplay"));
    expect(props.onOpen).toHaveBeenCalledTimes(1);
  });

  it("reports checkbox clicks to onSelect and leaves `selected` to the parent", async () => {
    const props = rowProps();
    render(ModListRow, props);
    const box = screen.getByRole("checkbox", { name: "Select Courseplay" });
    await userEvent.click(box);
    expect(props.onSelect).toHaveBeenCalledWith("C:/mods/FS25_Courseplay.zip", { shift: false, ctrl: false });
    expect(box).not.toBeChecked();
    expect(props.onOpen).not.toHaveBeenCalled();
  });

  it("supports the compact density", () => {
    const { container } = render(ModListRow, rowProps({ density: "compact" }));
    expect(container.querySelector(".row")).toHaveClass("compact");
  });
});

describe("ModCard", () => {
  it("has the Active switch, the ⋯ menu and at most three badges", async () => {
    const badges = rowBadges(mod({ isMap: true, scriptCount: 1, kind: "dir" }), { hasUpdate: true, hasSettings: true });
    const props = rowProps({ active: false, badges });
    render(ModCard, props);
    expect(screen.getByRole("switch", { name: "Active: Courseplay" })).toHaveAttribute("aria-checked", "false");
    expect(screen.getByText("Parked")).toBeInTheDocument();
    expect(screen.getByText("Update")).toBeInTheDocument();
    expect(screen.getByText("Folder mod")).toBeInTheDocument();
    expect(screen.getByText("Map")).toBeInTheDocument();
    expect(screen.getByText("+2")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "More actions for Courseplay" }));
    expect(props.onMenu).toHaveBeenCalledTimes(1);
  });
});
