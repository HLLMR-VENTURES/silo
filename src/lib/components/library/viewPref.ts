// The Library's list/cards choice, remembered in localStorage under `silo.library.view`. List is
// the default for every library size. Storage can be missing or throw (private mode, locked-down
// WebView), so both calls are guarded and never break the view.
//   loadLibraryView() -> "list" | "cards"
//   saveLibraryView("cards")

export type LibraryViewMode = "list" | "cards";
export const LIBRARY_VIEW_KEY = "silo.library.view";

type StorageLike = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): StorageLike | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

/**
 * The saved view, or "list" when nothing valid is saved.
 * @param storage where to read (defaults to localStorage)
 */
export function loadLibraryView(storage: StorageLike | null = defaultStorage()): LibraryViewMode {
  try {
    return storage?.getItem(LIBRARY_VIEW_KEY) === "cards" ? "cards" : "list";
  } catch {
    return "list";
  }
}

/**
 * Remember the view; a storage failure is ignored (the choice just isn't kept).
 * @param storage where to write (defaults to localStorage)
 */
export function saveLibraryView(mode: LibraryViewMode, storage: StorageLike | null = defaultStorage()): void {
  try {
    storage?.setItem(LIBRARY_VIEW_KEY, mode);
  } catch {
    // Not kept this time.
  }
}
