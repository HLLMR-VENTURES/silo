// Where focus goes when the mod drawer closes (✕, Esc, Find in Browse…): to the row of the mod it
// showed (by key, since virtualization may have recycled the row it was opened from), else to
// what had focus when it opened, else to a fallback (the main area), so focus never drops to
// <body>. Left alone when focus is still somewhere real (a rail click that left the Library).
// Call during component init:
//   returnFocusOnDrawerClose({
//     open: () => !!detailMod, key: () => detailMod?.path ?? null,
//     focusKey: (k) => libList?.focusKey(k) ?? false,
//     keepOpener: (el) => regionOf(el) === "main", fallback: () => document.getElementById("main"),
//   });
import { tick, untrack } from "svelte";

export interface DrawerFocusOptions {
  /** Is the drawer open (reactive)? */
  open: () => boolean;
  /** The key of the mod it shows (reactive); it follows the shown mod while open. */
  key: () => string | null;
  /** Focus the row for a key; false when it can't (the list is gone or the mod isn't listed). */
  focusKey: (key: string) => boolean;
  /** Should the element that had focus at open be the second choice? */
  keepOpener: (el: HTMLElement) => boolean;
  /** The last resort. */
  fallback: () => HTMLElement | null;
}

/**
 * Hand focus back when the drawer closes.
 * @param o how to tell the drawer's state and where focus can go
 */
export function returnFocusOnDrawerClose(o: DrawerFocusOptions): void {
  let wasOpen = false;
  let opener: HTMLElement | null = null;
  let shownKey: string | null = null;
  $effect(() => {
    const open = o.open();
    const key = o.key();
    untrack(() => {
      if (open) shownKey = key;
      if (open && !wasOpen) {
        const a = document.activeElement;
        opener = a instanceof HTMLElement && o.keepOpener(a) ? a : null;
      } else if (!open && wasOpen) {
        const back = shownKey;
        const prev = opener;
        opener = null;
        shownKey = null;
        void tick().then(() => {
          const a = document.activeElement;
          if (a && a !== document.body && a.isConnected) return;
          if (back && o.focusKey(back)) return;
          (prev?.isConnected ? prev : o.fallback())?.focus();
        });
      }
      wasOpen = open;
    });
  });
}
