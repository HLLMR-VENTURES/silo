// @vitest-environment jsdom
// use:tooltip: created lazily on first show, hover delay, aria-describedby wiring and cleanup.
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
});
