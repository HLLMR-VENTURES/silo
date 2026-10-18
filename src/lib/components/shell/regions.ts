// Shell regions for F6 / Shift+F6 cycling. Each region element carries `data-region="<name>"`;
// AppShell sets them on the rail, header, notices, main and drawer.
//   nextRegion("header", 1, presentRegions(shellEl)) // -> "notices" (or "main" with no notices)

import { focusableIn } from "../../ui/overlayStack";

export const REGIONS = ["rail", "header", "notices", "main", "drawer"] as const;
export type Region = (typeof REGIONS)[number];

/**
 * The region F6 (dir 1) or Shift+F6 (dir -1) moves to, wrapping at the ends.
 * @param current the region focus is in now, or null when it is in none
 * @param dir 1 forward, -1 backward
 * @param present the regions on screen, in any order (they are visited in REGIONS order)
 * @return the next region, or null when none is present
 */
export function nextRegion(
  current: Region | null,
  dir: 1 | -1,
  present: readonly Region[] = REGIONS,
): Region | null {
  const order = REGIONS.filter((r) => present.includes(r));
  if (!order.length) return null;
  const i = current === null ? -1 : order.indexOf(current);
  if (i === -1) {
    // Not in a known region: forward starts at main (the usual target), backward at the last one.
    if (dir === 1) return order.includes("main") ? "main" : order[0];
    return order[order.length - 1];
  }
  return order[(i + dir + order.length) % order.length];
}

/** The regions rendered under `root`, in REGIONS order. */
export function presentRegions(root: ParentNode): Region[] {
  const found = new Set<string>();
  root.querySelectorAll<HTMLElement>("[data-region]").forEach((el) => {
    if (el.dataset.region) found.add(el.dataset.region);
  });
  return REGIONS.filter((r) => found.has(r));
}

/** The region element that contains `node`, if any. */
export function regionOf(node: Node | null): Region | null {
  const el = node instanceof Element ? node : node?.parentElement;
  const hit = el?.closest<HTMLElement>("[data-region]")?.dataset.region;
  return (REGIONS as readonly string[]).includes(hit ?? "") ? (hit as Region) : null;
}

/** A region worth landing on: it has something to read or operate (an empty notices strip has not). */
function hasContent(el: HTMLElement): boolean {
  return (el.textContent ?? "").trim() !== "" || focusableIn(el).length > 0;
}

/**
 * Move focus into a region: its first element in the tab order, else the region itself (made
 * focusable with tabindex -1 if it isn't already).
 * @param el the region element
 */
export function focusRegion(el: HTMLElement): void {
  const first = focusableIn(el).find((n) => n.tabIndex >= 0);
  if (first) {
    first.focus();
    return;
  }
  if (!el.hasAttribute("tabindex")) el.setAttribute("tabindex", "-1");
  el.focus();
}

/**
 * F6 / Shift+F6: focus the next (or previous) region under `root` that has content.
 * @param root the shell's container (usually `document`)
 * @param dir 1 forward, -1 backward
 * @param active the element that has focus now
 * @return the region focused, or null when there is none
 */
export function cycleRegion(root: ParentNode, dir: 1 | -1, active: Element | null): Region | null {
  const els = new Map<Region, HTMLElement>();
  root.querySelectorAll<HTMLElement>("[data-region]").forEach((el) => {
    const r = el.dataset.region as Region;
    if ((REGIONS as readonly string[]).includes(r) && !els.has(r) && hasContent(el)) els.set(r, el);
  });
  const next = nextRegion(regionOf(active), dir, [...els.keys()]);
  if (!next) return null;
  focusRegion(els.get(next)!);
  return next;
}
