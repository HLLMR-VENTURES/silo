// @vitest-environment jsdom
// AppShell: landmarks, regions, skip link, rail collapse by width and pin; one h1; StatusStrip
// and ViewHeader basics.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import ShellHarness from "./__tests__/ShellHarness.svelte";
import ViewHeaderHarness from "./__tests__/ViewHeaderHarness.svelte";
import StatusStrip from "./StatusStrip.svelte";
import { nav, resetNav } from "../../state/nav.svelte";
import { presentRegions } from "./regions";

// A working localStorage before nav.svelte loads (Node's own global one only warns under jsdom).
const store = vi.hoisted(() => {
  const map = new Map<string, string>();
  const s = {
    getItem: (k: string) => map.get(k) ?? null,
    setItem: (k: string, v: string) => void map.set(k, String(v)),
    removeItem: (k: string) => void map.delete(k),
    clear: () => map.clear(),
  };
  Object.defineProperty(globalThis, "localStorage", { configurable: true, value: s });
  return s;
});

function setWidth(w: number) {
  Object.defineProperty(window, "innerWidth", { configurable: true, value: w });
  window.dispatchEvent(new Event("resize"));
}

beforeEach(() => {
  resetNav();
  store.clear();
  setWidth(1400);
});

afterEach(() => {
  document.querySelectorAll("[role='tooltip']").forEach((t) => t.remove());
});

describe("AppShell", () => {
  it("renders the landmarks and data-region attributes", () => {
    const { container } = render(ShellHarness, { withDrawer: true });
    expect(screen.getByRole("navigation", { name: "Main" })).toHaveAttribute("data-region", "rail");
    expect(screen.getByRole("banner")).toHaveAttribute("data-region", "header");
    const main = screen.getByRole("main");
    expect(main).toHaveAttribute("id", "main");
    expect(main).toHaveAttribute("tabindex", "-1");
    expect(screen.getByRole("complementary", { name: "Details" })).toHaveAttribute("data-region", "drawer");
    expect(presentRegions(container)).toEqual(["rail", "header", "notices", "main", "drawer"]);
  });

  it("omits the notices and drawer regions when not given", () => {
    const { container } = render(ShellHarness, { withNotices: false });
    expect(presentRegions(container)).toEqual(["rail", "header", "main"]);
    expect(screen.queryByRole("complementary")).toBeNull();
  });

  it("the skip link is the first tab stop and focuses main", async () => {
    const user = userEvent.setup();
    render(ShellHarness);
    await user.tab();
    const skip = screen.getByRole("link", { name: "Skip to content" });
    expect(skip).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("main")).toHaveFocus();
  });

  it("has exactly one h1 (Silo) and the view title as an h2", () => {
    render(ShellHarness);
    const h1s = screen.getAllByRole("heading", { level: 1 });
    expect(h1s).toHaveLength(1);
    expect(h1s[0]).toHaveTextContent("Silo");
    expect(screen.getByRole("heading", { level: 2, name: "Library" })).toBeInTheDocument();
  });

  it("the default rail follows nav.view and navigates", async () => {
    const user = userEvent.setup();
    render(ShellHarness);
    expect(screen.getByRole("button", { name: "Library" })).toHaveAttribute("aria-current", "page");
    await user.click(screen.getByRole("button", { name: "Diagnose" }));
    expect(nav.view).toBe("diagnose");
    expect(screen.getByRole("button", { name: "Diagnose" })).toHaveAttribute("aria-current", "page");
  });

  it("collapses below 1200px unless pinned expanded", async () => {
    setWidth(1000);
    const { rerender } = render(ShellHarness);
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
    await rerender({ pin: "expanded" });
    expect(screen.getByRole("button", { name: "Collapse sidebar" })).toBeInTheDocument();
  });

  it("collapses when the window shrinks", async () => {
    render(ShellHarness);
    expect(screen.getByRole("button", { name: "Collapse sidebar" })).toBeInTheDocument();
    setWidth(900);
    expect(await screen.findByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
  });

  it("the pin toggle stores a pin that differs from auto", async () => {
    const user = userEvent.setup();
    render(ShellHarness);
    await user.click(screen.getByRole("button", { name: "Collapse sidebar" }));
    expect(nav.rail).toBe("collapsed");
    expect(store.getItem("silo.nav.rail")).toBe("collapsed");
    expect(screen.getByRole("button", { name: "Expand sidebar" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Expand sidebar" }));
    expect(nav.rail).toBe("auto");
  });
});

describe("ViewHeader", () => {
  it("renders an h2 title, description, actions and tabs with the panel", async () => {
    const user = userEvent.setup();
    render(ViewHeaderHarness);
    expect(screen.getByRole("heading", { level: 2, name: "Diagnose" })).toBeInTheDocument();
    expect(screen.getByText("Find what breaks the game.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run check" })).toBeInTheDocument();
    expect(screen.getByRole("tablist", { name: "Diagnose" })).toBeInTheDocument();
    expect(screen.getByText("Panel lastRun")).toBeInTheDocument();
    await user.click(screen.getByRole("tab", { name: /Conflicts/ }));
    expect(screen.getByText("Panel conflicts")).toBeInTheDocument();
  });
});

describe("StatusStrip", () => {
  it("shows the summary and at most two items, running their actions", async () => {
    const user = userEvent.setup();
    const runs = [vi.fn(), vi.fn(), vi.fn()];
    render(StatusStrip, {
      left: "Ready to play · 142 of 737 active · Harvest 2026",
      items: [
        { id: "a", text: "4 updates", action: "Review", run: runs[0] },
        { id: "b", text: "2 problems", action: "Diagnose", run: runs[1] },
        { id: "c", text: "Third", action: "Never", run: runs[2] },
      ],
    });
    expect(screen.getByText("Ready to play · 142 of 737 active · Harvest 2026")).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(2);
    expect(screen.queryByText("Third")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Diagnose" }));
    expect(runs[1]).toHaveBeenCalledTimes(1);
    expect(runs[0]).not.toHaveBeenCalled();
  });

  it("renders without items", () => {
    render(StatusStrip, { left: "Ready to play" });
    expect(screen.queryByRole("list")).toBeNull();
  });
});
