/** Allowed UI sizes (root font scale). 1 = 100 %. */
export const UI_SCALES = [0.9, 1, 1.1, 1.25] as const;
export type UiScale = (typeof UI_SCALES)[number];
/** Parse a stored value; anything not in UI_SCALES falls back to 1. */
export function parseUiScale(raw: string | null | undefined): UiScale {
  const n = Number(raw);
  return (UI_SCALES as readonly number[]).includes(n) ? (n as UiScale) : 1;
}
