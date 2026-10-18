// @vitest-environment jsdom
// LibraryToolbar's "show only" facets (moved from the stat bar, #230): one at a time, click again
// to clear.
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import LibraryToolbar from "./LibraryToolbar.svelte";

const props = {
  selectedTag: null,
  shownCount: 10,
  allActive: false,
  activeInFilter: 0,
  disabled: false,
  libView: "tiles" as const,
  statCounts: { maps: 2, scripts: 5, unique: 1, active: 7 },
  onSelectAll: () => {},
  onClearTag: () => {},
};

describe("LibraryToolbar facets", () => {
  it("shows the counts and toggles one facet at a time", async () => {
    const user = userEvent.setup();
    render(LibraryToolbar, props);
    const maps = screen.getByRole("button", { name: "2 maps" });
    const scripts = screen.getByRole("button", { name: "5 script mods" });
    expect(screen.getByRole("button", { name: "1 uniqueType" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "7 active" })).toBeInTheDocument();
    expect(maps).toHaveAttribute("aria-pressed", "false");

    await user.click(maps);
    expect(maps).toHaveAttribute("aria-pressed", "true");
    await user.click(scripts);
    expect(maps).toHaveAttribute("aria-pressed", "false");
    expect(scripts).toHaveAttribute("aria-pressed", "true");
    await user.click(scripts);
    expect(scripts).toHaveAttribute("aria-pressed", "false");
  });
});
