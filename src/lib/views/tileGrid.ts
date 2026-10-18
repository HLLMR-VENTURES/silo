// Library tile grid: how many landscape tiles fit side by side in the list's width, and the rows
// the virtual list renders. Pure, so it unit-tests without the app.
//   tileColumns(1240) -> 2   (the 1280px content cap)
//   tileColumns(1880) -> 3   (the wide 1920px cap)
//   chunk(["a", "b", "c"], 2) -> [["a", "b"], ["c"]]

/** A tile wider than this reads as stretched: past it, another column opens. */
export const TILE_MAX_W = 640;
/** Below this list width a single column reads better than two cramped tiles. */
export const TILE_SINGLE_BELOW = 600;
/** The gap between tiles, matching the grid's CSS gap. */
export const TILE_GAP = 16;

/**
 * Columns for a tile grid of the given width.
 * @param width the list's inner width in px; 0 (not measured yet) gives the two-column default
 * @returns 1 when narrow, otherwise at least 2, adding columns so no tile exceeds TILE_MAX_W
 */
export function tileColumns(width: number): number {
  if (!Number.isFinite(width) || width <= 0) return 2;
  if (width < TILE_SINGLE_BELOW) return 1;
  return Math.max(2, Math.ceil((width + TILE_GAP) / (TILE_MAX_W + TILE_GAP)));
}

/**
 * Split items into rows of `size` (the last row may be shorter).
 * @param items the items in display order
 * @param size items per row; values below 1 count as 1
 * @returns the rows
 */
export function chunk<T>(items: readonly T[], size: number): T[][] {
  const n = Math.max(1, Math.floor(size));
  const out: T[][] = [];
  for (let i = 0; i < items.length; i += n) out.push(items.slice(i, i + n));
  return out;
}
