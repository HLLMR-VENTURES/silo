// @vitest-environment jsdom
// The shortcut listener in a real shell: F6 / Shift+F6 cycle the regions (skipping an empty
// notices strip, working from inside the search field), Ctrl+K and / focus search, ? opens the
// Keyboard shortcuts dialog, and nothing fires behind that dialog.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import ShortcutsHarness from "./__tests__/ShortcutsHarness.svelte";
import { resetNav } from "../../state/nav.svelte";
import { regionOf } from "./regions";
import { SHORTCUT_DOCS } from "../../state/shortcuts";

// A working localStorage before nav.svelte loads (Node's own global one only warns under jsdom).
vi.hoisted(() => {
  const map = new Map<string, string>();
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: {
      getItem: (k: string) => map.get(k) ?? null,
      setItem: (k: string, v: string) => void map.set(k, String(v)),
      removeItem: (k: string) => void map.delete(k),
      clear: () => map.clear(),
    },
  });
});

beforeEach(() => {
  resetNav();
  Object.defineProperty(window, "innerWidth", { configurable: true, value: 1400 });
});

const where = () => regionOf(document.activeElement);

describe("F6 region cycling", () => {
  it("cycles rail, header, notices, main, drawer and round; Shift+F6 in reverse", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    screen.getByRole("button", { name: "Library" }).focus();
    expect(where()).toBe("rail");

    const forward: (string | null)[] = [];
    for (let i = 0; i < 5; i++) {
      await user.keyboard("{F6}");
      forward.push(where());
    }
    expect(forward).toEqual(["header", "notices", "main", "drawer", "rail"]);
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Library" }));

    const backward: (string | null)[] = [];
    for (let i = 0; i < 5; i++) {
      await user.keyboard("{Shift>}{F6}{/Shift}");
      backward.push(where());
    }
    expect(backward).toEqual(["drawer", "main", "notices", "header", "rail"]);
  });

  it("lands on each region's first tab stop, or the region itself when it has none", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    screen.getByRole("button", { name: "Library" }).focus();
    await user.keyboard("{F6}");
    expect(document.activeElement).toBe(screen.getByRole("searchbox"));
    await user.keyboard("{F6}");
    const notices = document.querySelector<HTMLElement>("[data-region='notices']")!;
    expect(document.activeElement).toBe(notices);
    expect(notices).toHaveAttribute("tabindex", "-1");
    await user.keyboard("{F6}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "In content" }));
    await user.keyboard("{F6}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close details" }));
  });

  it("skips an empty notices strip and a missing drawer; from outside any region it starts at main", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness, { noticeText: "", withDrawer: false });
    (document.activeElement as HTMLElement | null)?.blur();
    await user.keyboard("{F6}");
    expect(where()).toBe("main");
    await user.keyboard("{F6}");
    expect(where()).toBe("rail");
    await user.keyboard("{F6}");
    expect(where()).toBe("header");
    await user.keyboard("{F6}");
    expect(where()).toBe("main");
  });

  it("works while typing in the search field", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    await user.click(screen.getByRole("searchbox"));
    await user.keyboard("abc{F6}");
    expect(where()).toBe("notices");
    expect(screen.getByRole("searchbox")).toHaveValue("abc");
  });
});

describe("search and the shortcuts dialog", () => {
  it("Ctrl+K and / focus the header search; the / is not typed into it", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    const content = screen.getByRole("button", { name: "In content" });
    content.focus();
    await user.keyboard("{Control>}k{/Control}");
    const search = screen.getByRole("searchbox");
    expect(document.activeElement).toBe(search);
    content.focus();
    await user.keyboard("/");
    expect(document.activeElement).toBe(search);
    expect(search).toHaveValue("");
    await user.keyboard("/");
    expect(search).toHaveValue("/");
  });

  it("? opens the Keyboard shortcuts dialog listing every shortcut; Esc closes it", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    screen.getByRole("button", { name: "In content" }).focus();
    await user.keyboard("?");
    const dialog = await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    for (const doc of SHORTCUT_DOCS) expect(dialog).toHaveTextContent(doc.label);
    expect(dialog.querySelectorAll("kbd").length).toBeGreaterThan(SHORTCUT_DOCS.length);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("? typed in the search field stays text", async () => {
    const user = userEvent.setup();
    render(ShortcutsHarness);
    await user.click(screen.getByRole("searchbox"));
    await user.keyboard("?");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("searchbox")).toHaveValue("?");
  });

  it("ignores shortcuts while the dialog is open", async () => {
    const user = userEvent.setup();
    const onaction = vi.fn();
    render(ShortcutsHarness, { onaction });
    screen.getByRole("button", { name: "In content" }).focus();
    await user.keyboard("?");
    await screen.findByRole("dialog", { name: "Keyboard shortcuts" });
    onaction.mockClear();
    await user.keyboard("{F6}{Control>}k{/Control}{Alt>}1{/Alt}{F5}");
    expect(onaction).not.toHaveBeenCalled();
  });
});
