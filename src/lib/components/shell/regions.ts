// Shell regions for F6 / Shift+F6 cycling. Each region element carries `data-region="<name>"`;
// AppShell sets them on the rail, header, notices, main and drawer.
//   nextRegion("header", 1, presentRegions(shellEl)) // -> "notices" (or "main" with no notices)

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
