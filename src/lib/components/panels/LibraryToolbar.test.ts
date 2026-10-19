// @vitest-environment jsdom
// LibraryToolbar (#242): the All / Active / Parked segment, the Favorites and Needs attention
// chips, and the Filters popover's checkboxes and tag select. App owns the filters, so each
// control calls onchange with the next filters (Needs update goes through onneedsupdate).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import LibraryToolbar from "./LibraryToolbar.svelte";
import { NO_FILTERS, type LibraryFilters } from "../library/filters";

function setup(filters: LibraryFilters = { ...NO_FILTERS }, extra: Record<string, unknown> = {}) {
  const onchange = vi.fn();
  const onneedsupdate = vi.fn();
  const user = userEvent.setup();
  render(LibraryToolbar, {
    filters,
    onchange,
    onneedsupdate,
    shownCount: 10,
    libView: "list" as const,
    counts: { attention: 3, maps: 2, conflicted: 0 },
    ...extra,
  });
  return { user, onchange, onneedsupdate };
}

beforeEach(() => {
  document.body.innerHTML = "";
});

describe("LibraryToolbar", () => {
  it("offers All / Active / Parked and reports the choice", async () => {
    const { user, onchange } = setup();
    const group = screen.getByRole("radiogroup", { name: "Show" });
    expect([...group.querySelectorAll("[role=radio]")].map((r) => r.textContent?.trim())).toEqual([
      "All",
      "Active",
      "Parked",
    ]);
    await user.click(screen.getByRole("radio", { name: "Parked" }));
    expect(onchange).toHaveBeenLastCalledWith({ ...NO_FILTERS, status: "parked" });
  });

  it("toggles the Favorites and Needs attention chips (aria-pressed)", async () => {
    const { user, onchange } = setup({ ...NO_FILTERS, favorites: true });
    const fav = screen.getByRole("button", { name: "Favorites" });
    expect(fav).toHaveAttribute("aria-pressed", "true");
    await user.click(fav);
    expect(onchange).toHaveBeenLastCalledWith({ ...NO_FILTERS, favorites: false });
    const attention = screen.getByRole("button", { name: "Needs attention (3)" });
    expect(attention).toHaveAttribute("aria-pressed", "false");
    await user.click(attention);
    expect(onchange).toHaveBeenLastCalledWith({ ...NO_FILTERS, favorites: true, attention: true });
  });

  it("opens the Filters popover with the old toggles as checkboxes", async () => {
    const { user, onchange, onneedsupdate } = setup({ ...NO_FILTERS, hidden: true }, { tags: ["harvest"] });
    expect(screen.getByRole("button", { name: "Filters (1)" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Filters (1)" }));
    const panel = await screen.findByRole("dialog", { name: "Filters" });
    for (const name of ["Include hidden mods", "Flagged", "In conflict", "Has settings", "Needs update", "Maps (2)", "Script mods"]) {
      expect(screen.getByRole("checkbox", { name })).toBeInTheDocument();
    }
    expect(screen.getByRole("checkbox", { name: "Include hidden mods" })).toBeChecked();
    // No conflicts: In conflict can't narrow anything.
    expect(screen.getByRole("checkbox", { name: "In conflict" })).toBeDisabled();
    await user.click(screen.getByRole("checkbox", { name: "Maps (2)" }));
    expect(onchange).toHaveBeenLastCalledWith({ ...NO_FILTERS, hidden: true, maps: true });
    await user.click(screen.getByRole("checkbox", { name: "Needs update" }));
    expect(onneedsupdate).toHaveBeenCalledWith(true);
    await user.selectOptions(panel.querySelector("select")!, "harvest");
    expect(onchange).toHaveBeenLastCalledWith({ ...NO_FILTERS, hidden: true, tag: "harvest" });
  });

  it("keeps a checkbox showing what's in effect when App doesn't switch it on", async () => {
    const { user } = setup();
    await user.click(screen.getByRole("button", { name: "Filters" }));
    const box = await screen.findByRole("checkbox", { name: "Needs update" });
    await user.click(box);
    // onneedsupdate found nothing to update, so the filters stay as they were: unchecked.
    expect(box).not.toBeChecked();
  });
});
