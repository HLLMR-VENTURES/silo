// The Library's list/cards choice, remembered in localStorage under `silo.library.view`. List is
// the default for every library size. Storage can be missing or throw (private mode, locked-down
// WebView), so both calls are guarded and never break the view.
//   loadLibraryView() -> "list" | "cards"
//   saveLibraryView("cards")
// Also the All / Active / Parked segment (`silo.library.state`) and the list density
// (`silo.library.density`, Settings → General).
import type { StatusFilter } from "./filters";
import type { Density } from "./ModListRow.svelte";

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

// ── The segment, under `silo.library.state` (JSON, room for more later), and the density ──
export const LIBRARY_STATE_KEY = "silo.library.state";
export const LIBRARY_DENSITY_KEY = "silo.library.density";

/** What the Library remembers between launches. */
export interface LibraryState {
  status: StatusFilter;
}

/**
 * The saved Library state, or the defaults (All) when nothing valid is saved.
 * @param storage where to read (defaults to localStorage)
 */
export function loadLibraryState(storage: StorageLike | null = defaultStorage()): LibraryState {
  try {
    const raw = storage?.getItem(LIBRARY_STATE_KEY);
    const v = raw ? (JSON.parse(raw) as Partial<LibraryState>) : null;
    const status = v?.status === "active" || v?.status === "parked" ? v.status : "all";
    return { status };
  } catch {
    return { status: "all" };
  }
}

/**
 * Remember the Library state; a storage failure is ignored.
 * @param storage where to write (defaults to localStorage)
 */
export function saveLibraryState(state: LibraryState, storage: StorageLike | null = defaultStorage()): void {
  try {
    storage?.setItem(LIBRARY_STATE_KEY, JSON.stringify({ status: state.status }));
  } catch {
    // Not kept this time.
  }
}

/**
 * The saved list density, or "comfortable".
 * @param storage where to read (defaults to localStorage)
 */
export function loadLibraryDensity(storage: StorageLike | null = defaultStorage()): Density {
  try {
    return storage?.getItem(LIBRARY_DENSITY_KEY) === "compact" ? "compact" : "comfortable";
  } catch {
    return "comfortable";
  }
}

/**
 * Remember the list density; a storage failure is ignored.
 * @param storage where to write (defaults to localStorage)
 */
export function saveLibraryDensity(d: Density, storage: StorageLike | null = defaultStorage()): void {
  try {
    storage?.setItem(LIBRARY_DENSITY_KEY, d);
  } catch {
    // Not kept this time.
  }
}
