// @vitest-environment jsdom
// use:tooltip: created lazily on first show, hover delay, aria-describedby wiring and cleanup, and
// never lingering once its trigger is gone or turns disabled.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { tooltip } from "./tooltip";

let node: HTMLButtonElement;

beforeEach(() => {
  vi.useFakeTimers();
  node = document.createElement("button");
  node.textContent = "Rescan";
  document.body.append(node);
});

afterEach(() => {
  node.remove();
  vi.useRealTimers();
});

const tips = () => document.querySelectorAll("[role='tooltip']");

describe("tooltip", () => {
  it("creates nothing until it first shows, then wires aria-describedby", () => {
    const action = tooltip(node, "Rescan library");
    expect(tips()).toHaveLength(0);
    expect(node).not.toHaveAttribute("aria-describedby");

    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(499);
    expect(tips()).toHaveLength(0);
    vi.advanceTimersByTime(1);
    const tip = tips()[0] as HTMLElement;
    expect(tip).toHaveTextContent("Rescan library");
    expect(tip.hidden).toBe(false);
    expect(node.getAttribute("aria-describedby")).toBe(tip.id);

    action.destroy();
    expect(tips()).toHaveLength(0);
    expect(node).not.toHaveAttribute("aria-describedby");
  });

  it("keeps the trigger's own aria-describedby ids", () => {
    node.setAttribute("aria-describedby", "hint");
    const action = tooltip(node, "Rescan library");
    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    expect(node.getAttribute("aria-describedby")).toBe(`hint ${tips()[0].id}`);
    action.destroy();
    expect(node.getAttribute("aria-describedby")).toBe("hint");
  });

  it("an update before the first show doesn't create it", () => {
    const action = tooltip(node, "One");
    action.update("Two");
    expect(tips()).toHaveLength(0);
    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    expect(tips()[0]).toHaveTextContent("Two");
    action.destroy();
  });

  it("hides at once when its trigger is removed from the page", async () => {
    const action = tooltip(node, "Delete loadout");
    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    const tip = tips()[0] as HTMLElement;
    expect(tip.hidden).toBe(false);
    node.remove(); // the row went away before the action was destroyed
    await Promise.resolve(); // MutationObserver callbacks run as a microtask
    expect(tip.hidden).toBe(true);
    action.destroy();
    expect(tips()).toHaveLength(0);
  });

  it("never shows after a hover delay if the trigger left the page meanwhile", () => {
    const action = tooltip(node, "Delete loadout");
    node.dispatchEvent(new Event("pointerenter"));
    node.remove();
    vi.advanceTimersByTime(500);
    expect(tips()).toHaveLength(0);
    action.destroy();
  });

  it("hides at once when its trigger becomes disabled", async () => {
    const action = tooltip(node, "Overwrite");
    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    const tip = tips()[0] as HTMLElement;
    expect(tip.hidden).toBe(false);
    node.disabled = true;
    await Promise.resolve();
    expect(tip.hidden).toBe(true);
    action.destroy();
  });

  it("keeps a tip that explains why an already-disabled trigger is off", async () => {
    node.disabled = true;
    const action = tooltip(node, "Silo is reclaiming disk space.");
    node.dispatchEvent(new Event("pointerenter"));
    vi.advanceTimersByTime(500);
    const tip = tips()[0] as HTMLElement;
    document.body.append(document.createElement("span")); // unrelated page change
    await Promise.resolve();
    expect(tip.hidden).toBe(false);
    action.destroy();
  });
});
