// @vitest-environment jsdom
// Focus is never lost to the page in the Library (#242): the focused row leaving the list, the
// BulkBar going away, the last filter chip removed; plus duplicate keys, row names, the tab stop
// across a view switch, and the BulkBar's arrow keys.
import { afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { tick } from "svelte";
import ListHarness from "./__tests__/ListHarness.svelte";
import FocusHarness from "./__tests__/FocusHarness.svelte";
import ActiveFilters from "./ActiveFilters.svelte";
import BulkBar from "./BulkBar.svelte";

beforeAll(() => {
  if (typeof globalThis.ResizeObserver === "undefined") {
    globalThis.ResizeObserver = class {
      observe() {}
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
  }
});
afterEach(() => vi.restoreAllMocks());

const make = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `k${i}`, name: `Item ${i}` }));
const itemEl = (c: HTMLElement, key: string) =>
  c.querySelector<HTMLElement>(`[role="listitem"][data-key="${key}"]`);
async function settle() {
  for (let i = 0; i < 4; i++) await tick();
}

describe("VirtualGrid: the focused item leaves the list", () => {
  it("focus moves to the item now at the same index", async () => {
    const items = make(10);
    const { container, rerender } = render(ListHarness, { items });
    itemEl(container, "k3")!.focus();
    await settle();
    await rerender({ items: items.filter((it) => it.id !== "k3") });
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k4"));
    expect(itemEl(container, "k4")).toHaveAttribute("tabindex", "0");
  });

  it("the last one: focus moves to the new last item", async () => {
    const items = make(5);
    const { container, rerender } = render(ListHarness, { items });
    itemEl(container, "k4")!.focus();
    await settle();
    await rerender({ items: items.slice(0, 3) });
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k2"));
  });

  it("nothing left: focus goes to the grid itself", async () => {
    const items = make(2);
    const { container, rerender } = render(ListHarness, { items });
    itemEl(container, "k0")!.focus();
    await settle();
    await rerender({ items: [] });
    await settle();
    expect(document.activeElement).toBe(container.querySelector(".vg-viewport"));
  });

  it("focus elsewhere: it stays there", async () => {
    const items = make(5);
    const { container, rerender } = render(ListHarness, { items });
    itemEl(container, "k2")!.focus();
    await settle();
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    await rerender({ items: items.filter((it) => it.id !== "k2") });
    await settle();
    expect(document.activeElement).toBe(outside);
    outside.remove();
  });
});

describe("VirtualGrid: keys and names", () => {
  it("drops items sharing a key (keeping the first) and warns, instead of throwing", async () => {
    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    const items = [
      { id: "a", name: "First" },
      { id: "a", name: "Second" },
      { id: "b", name: "Third" },
    ];
    const { container } = render(ListHarness, { items });
    await settle();
    const rows = container.querySelectorAll('[role="listitem"]');
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveTextContent("First");
    expect(rows[0]).toHaveAttribute("aria-setsize", "2");
    expect(warn).toHaveBeenCalled();
  });

  it("names each item with itemLabel", () => {
    const { container } = render(ListHarness, {
      items: make(3),
      itemLabel: (it: { name: string }) => `${it.name}, active`,
    });
    expect(itemEl(container, "k1")).toHaveAttribute("aria-label", "Item 1, active");
  });

  it("showKey makes an item the tab stop without taking focus; currentKey reads it", async () => {
    const { container, component } = render(ListHarness, { items: make(200) });
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    expect(component.showKey("k120")).toBe(true);
    await settle();
    expect(component.currentKey()).toBe("k120");
    expect(itemEl(container, "k120")).toHaveAttribute("tabindex", "0");
    expect(document.activeElement).toBe(outside);
    expect(component.showKey("nope")).toBe(false);
    outside.remove();
  });
});

describe("the BulkBar going away", () => {
  it("with focus in it: focus goes to the list's row", async () => {
    const user = userEvent.setup();
    const items = make(10);
    const { container, rerender } = render(FocusHarness, { items, selected: false });
    itemEl(container, "k3")!.focus(); // the row the user was on
    await rerender({ items, selected: true });
    await settle();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await rerender({ items, selected: false });
    await settle();
    expect(document.activeElement).toBe(itemEl(container, "k3"));
  });

  it("an empty list: the toolbar's first control", async () => {
    const user = userEvent.setup();
    const { rerender } = render(FocusHarness, { items: [], selected: true });
    await settle();
    await user.click(screen.getByRole("button", { name: "Clear" }));
    await rerender({ items: [], selected: false });
    await settle();
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "First tool" }));
  });

  it("with focus elsewhere: left alone", async () => {
    const items = make(3);
    const { rerender } = render(FocusHarness, { items, selected: true });
    const outside = document.createElement("button");
    document.body.append(outside);
    outside.focus();
    await rerender({ items, selected: false });
    await settle();
    expect(document.activeElement).toBe(outside);
    outside.remove();
  });
});

describe("ActiveFilters: the last chip removed", () => {
  const chips = [{ id: "favorites", label: "Favorites" }];
  it("hands focus to onemptied (the Filters button)", async () => {
    const user = userEvent.setup();
    const onemptied = vi.fn();
    const onremove = vi.fn();
    const { rerender } = render(ActiveFilters, { chips, onremove, onclear: vi.fn(), onemptied });
    await user.click(screen.getByRole("button", { name: "Remove filter: Favorites" }));
    expect(onremove).toHaveBeenCalledWith("favorites");
    await rerender({ chips: [], onremove, onclear: vi.fn(), onemptied });
    await settle();
    expect(onemptied).toHaveBeenCalledTimes(1);
  });

  it("not when focus was somewhere else", async () => {
    const onemptied = vi.fn();
    const { rerender } = render(ActiveFilters, { chips, onremove: vi.fn(), onclear: vi.fn(), onemptied });
    await rerender({ chips: [], onremove: vi.fn(), onclear: vi.fn(), onemptied });
    await settle();
    expect(onemptied).not.toHaveBeenCalled();
  });
});

describe("BulkBar keys", () => {
  const h = () => ({
    count: 2,
    shownCount: 5,
    onturnon: vi.fn(),
    onpark: vi.fn(),
    onfavorite: vi.fn(),
    onhide: vi.fn(),
    onselectall: vi.fn(),
    onclear: vi.fn(),
  });

  it("one Tab stop; ←/→ move between buttons (wrapping), Home/End jump", async () => {
    const user = userEvent.setup();
    render(BulkBar, h());
    await settle();
    const bar = screen.getByRole("toolbar", { name: "Selected mods" });
    expect(bar.querySelectorAll('button[tabindex="0"]')).toHaveLength(1);
    const turnOn = screen.getByRole("button", { name: "Turn on" });
    turnOn.focus();
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Park" }));
    expect(screen.getByRole("button", { name: "Park" })).toHaveAttribute("tabindex", "0");
    expect(turnOn).toHaveAttribute("tabindex", "-1");
    await user.keyboard("{End}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Clear" }));
    await user.keyboard("{ArrowRight}");
    expect(document.activeElement).toBe(turnOn);
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Clear" }));
    await user.keyboard("{Home}");
    expect(document.activeElement).toBe(turnOn);
  });

  it("skips buttons that are off", async () => {
    const user = userEvent.setup();
    render(BulkBar, { ...h(), writeReason: "Silo is moving mods." });
    await settle();
    const fav = screen.getByRole("button", { name: "Favorite" });
    fav.focus();
    await user.keyboard("{ArrowLeft}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Clear" }));
  });
});
