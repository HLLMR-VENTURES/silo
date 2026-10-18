// @vitest-environment jsdom
// NavRail: aria-current, roving focus, badge text, collapsed labels and tooltips, pin toggle.
import { afterEach, describe, expect, it, vi } from "vitest";
import { tick } from "svelte";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import NavRail from "./NavRail.svelte";
import { BISECT_BADGE, navItemName, NAV_ITEMS } from "./navItems";

afterEach(() => {
  document.querySelectorAll("[role='tooltip']").forEach((t) => t.remove());
});

function setup(props: Record<string, unknown> = {}) {
  const onnavigate = vi.fn();
  const onTogglePin = vi.fn();
  const r = render(NavRail, { current: "library", onnavigate, onTogglePin, ...props });
  return { ...r, onnavigate, onTogglePin, user: userEvent.setup() };
}

describe("NavRail", () => {
  it("lists the five destinations and the bottom three, marking the current page", () => {
    setup({ current: "diagnose" });
    const names = screen.getAllByRole("button").map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual([
      "Library",
      "Browse",
      "Mod sets",
      "Diagnose",
      "Updates",
      "Tools",
      "Settings",
      "Help",
      "Collapse sidebar",
    ]);
    expect(screen.getByRole("button", { name: "Diagnose" })).toHaveAttribute("aria-current", "page");
    expect(screen.getByRole("button", { name: "Library" })).not.toHaveAttribute("aria-current");
  });

  it("navigates on click", async () => {
    const { user, onnavigate } = setup();
    await user.click(screen.getByRole("button", { name: "Browse" }));
    expect(onnavigate).toHaveBeenCalledWith("browse");
  });

  it("names the keyboard shortcut of each destination that has one", () => {
    setup();
    const keys = (name: string) => screen.getByRole("button", { name }).getAttribute("aria-keyshortcuts");
    expect(keys("Library")).toBe("Alt+1");
    expect(keys("Updates")).toBe("Alt+5");
    expect(keys("Settings")).toBe("Control+,");
    expect(keys("Tools")).toBeNull();
  });

  it("has one tab stop on the current item; arrows move focus and wrap; Enter activates", async () => {
    const { user, onnavigate } = setup({ current: "browse" });
    const browse = screen.getByRole("button", { name: "Browse" });
    const tabbable = NAV_ITEMS.map((i) => screen.getByRole("button", { name: i.label })).filter(
      (b) => b.tabIndex === 0,
    );
    expect(tabbable).toEqual([browse]);

    await user.tab();
    expect(browse).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    const modSets = screen.getByRole("button", { name: "Mod sets" });
    expect(modSets).toHaveFocus();
    expect(modSets.tabIndex).toBe(0);
    expect(browse.tabIndex).toBe(-1);

    await user.keyboard("{ArrowUp}{ArrowUp}{ArrowUp}");
    expect(screen.getByRole("button", { name: "Help" })).toHaveFocus();
    await user.keyboard("{Home}");
    expect(screen.getByRole("button", { name: "Library" })).toHaveFocus();
    await user.keyboard("{End}");
    expect(screen.getByRole("button", { name: "Help" })).toHaveFocus();

    await user.keyboard("{Enter}");
    expect(onnavigate).toHaveBeenLastCalledWith("help");
    await user.keyboard("{ArrowUp} ");
    expect(onnavigate).toHaveBeenLastCalledWith("settings");
  });

  it("puts the tab stop back on the current page when the page changes or focus leaves", async () => {
    const { user, rerender } = setup({ current: "library" });
    const outside = document.createElement("button");
    outside.textContent = "Outside";
    document.body.appendChild(outside);
    await user.tab();
    await user.keyboard("{ArrowDown}{ArrowDown}");
    const modSets = screen.getByRole("button", { name: "Mod sets" });
    expect(modSets.tabIndex).toBe(0);
    outside.focus();
    await tick();
    expect(screen.getByRole("button", { name: "Library" }).tabIndex).toBe(0);
    expect(modSets.tabIndex).toBe(-1);

    modSets.focus();
    await tick();
    expect(modSets.tabIndex).toBe(0);
    await rerender({ current: "updates" });
    await tick();
    expect(screen.getByRole("button", { name: "Updates" }).tabIndex).toBe(0);
    expect(modSets.tabIndex).toBe(-1);
    outside.remove();
  });

  it("shows a dot on Diagnose while a bisection runs, read out as finding the problem mod", () => {
    setup({ badges: { diagnose: BISECT_BADGE } });
    const diag = screen.getByRole("button", { name: "Diagnose, finding the problem mod" });
    expect(diag).toHaveTextContent(BISECT_BADGE);
    expect(navItemName({ id: "diagnose", label: "Diagnose" }, BISECT_BADGE)).toBe(
      "Diagnose, finding the problem mod",
    );
  });

  it("names the Updates badge for screen readers and shows the count", () => {
    setup({ badges: { updates: 4 } });
    const updates = screen.getByRole("button", { name: "Updates, 4 available" });
    expect(updates).toHaveTextContent("4");
  });

  it("hides zero badges", () => {
    setup({ badges: { updates: 0 } });
    expect(screen.getByRole("button", { name: "Updates" })).not.toHaveTextContent("0");
  });

  it("collapsed: icon only, names in aria-label, tooltips carry labels, badge becomes a dot", async () => {
    const { user, container } = setup({ collapsed: true, badges: { updates: 2 } });
    const library = screen.getByRole("button", { name: "Library" });
    expect(library).toHaveTextContent("");
    const updates = screen.getByRole("button", { name: "Updates, 2 available" });
    expect(updates).not.toHaveTextContent("2");
    expect(container.querySelector(".dot")).not.toBeNull();

    await user.tab();
    expect(library).toHaveFocus();
    const tip = document.getElementById(library.getAttribute("aria-describedby") ?? "");
    expect(tip).toHaveAttribute("role", "tooltip");
    expect(tip).toHaveTextContent("Library");

    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
  });

  it("expanded: no tooltips", async () => {
    const { user } = setup();
    await user.tab();
    expect(screen.getByRole("button", { name: "Library" })).not.toHaveAttribute("aria-describedby");
  });

  it("pin toggle calls onTogglePin", async () => {
    const { user, onTogglePin } = setup();
    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(onTogglePin).toHaveBeenCalledTimes(1);
  });

  it("navItemName covers counts and words", () => {
    expect(navItemName({ id: "updates", label: "Updates" }, 3)).toBe("Updates, 3 available");
    expect(navItemName({ id: "diagnose", label: "Diagnose" }, 2)).toBe("Diagnose, 2");
    expect(navItemName({ id: "browse", label: "Browse" }, "New")).toBe("Browse, New");
    expect(navItemName({ id: "browse", label: "Browse" }, " ")).toBe("Browse");
    expect(navItemName({ id: "browse", label: "Browse" })).toBe("Browse");
  });
});
