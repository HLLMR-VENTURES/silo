// Focus that would be lost when a piece of UI goes away (the BulkBar when the selection clears,
// the filter chips when the last one is removed): before its elements are removed, note whether
// focus was inside, and if so hand it on once the DOM has updated. Call during component init.
//   returnFocusWhenHidden(() => selection.size > 0, () => bulkEl, () => list?.focusCurrent());
import { tick, untrack } from "svelte";

/**
 * True when focus is on `el` or inside it.
 * @param el the container (may be missing)
 */
export function focusInside(el: Element | null | undefined): boolean {
  if (!el || typeof document === "undefined") return false;
  const a = document.activeElement;
  return !!a && el.contains(a);
}

/**
 * Hand focus on when something stops being shown while it held focus.
 * @param shown whether it's shown now (reactive)
 * @param el its root element while shown
 * @param restore moves focus somewhere sensible; runs after the DOM update
 */
export function returnFocusWhenHidden(
  shown: () => boolean,
  el: () => Element | null | undefined,
  restore: () => void,
): void {
  let was = untrack(shown);
  // A pre-effect runs before the DOM update, while the elements (and focus) are still there.
  $effect.pre(() => {
    const now = shown();
    const hiding = was && !now;
    was = now;
    if (!hiding) return;
    if (untrack(() => focusInside(el()))) void tick().then(restore);
  });
}
