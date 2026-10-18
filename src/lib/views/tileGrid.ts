// Library card grid: how many landscape cards fit side by side in the list's width. Pure, so it
// unit-tests without the app.
//   tileColumns(1240) -> 2   (the 1280px content cap)
//   tileColumns(1880) -> 3   (the wide 1920px cap)

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
