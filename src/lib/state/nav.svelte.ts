// Where the user is in the app: the current view, each view's sub-tab, a short back history and
// the rail pin preference.
//   go("diagnose", "conflicts") // switch view (and that view's tab); pushes history
//   setTab("modSets", "share") // change a view's tab without a history entry (Tabs bindings)
//   back()                     // returns false when there is nothing to go back to
//   setRailPin("collapsed")    // persisted; railCollapsedFor(width, nav.rail) gives the layout
// The last view and the rail pin persist in localStorage (silo.nav.view, silo.nav.rail); a launch
// never reopens Browse (that would call the catalog unasked), it starts on the Library instead.
// Storage can be missing or throw (private mode, tests): every access is guarded and falls back
// to defaults.

export type ViewId =
  | "library"
  | "browse"
  | "modSets"
  | "diagnose"
  | "updates"
  | "tools"
  | "settings"
  | "help";

export const VIEW_IDS: readonly ViewId[] = [
  "library",
  "browse",
  "modSets",
  "diagnose",
  "updates",
  "tools",
  "settings",
  "help",
];

/** Known sub-tabs (views store them as plain strings in nav.tab). Settings uses section ids. */
export type ModSetsTab = "sets" | "share" | "multiplayer" | "savegames";
/** Guided bisection runs inside Last run, so it has no tab of its own. */
export type DiagnoseTab = "lastRun" | "conflicts" | "problems";
export type ToolsTab = "stats" | "controls" | "bridge" | "graphics";

/** Rail preference: follow the window width, or stay expanded / collapsed. */
export type RailPin = "auto" | "expanded" | "collapsed";

export const NAV_VIEW_KEY = "silo.nav.view";
export const NAV_RAIL_KEY = "silo.nav.rail";
export const HISTORY_MAX = 20;
/** Below this window width the rail collapses to icons unless pinned expanded. */
export const RAIL_COLLAPSE_BELOW = 1200;

const DEFAULT_VIEW: ViewId = "library";
const RAIL_PINS: readonly RailPin[] = ["auto", "expanded", "collapsed"];

interface HistoryEntry {
  view: ViewId;
  tab: string | undefined;
}

interface NavState {
  view: ViewId;
  tab: Record<ViewId, string | undefined>;
  rail: RailPin;
}

function emptyTabs(): Record<ViewId, string | undefined> {
  return Object.fromEntries(VIEW_IDS.map((v) => [v, undefined])) as Record<ViewId, string | undefined>;
}

/** Reactive navigation state. Change it through go / back / setTab / setRailPin. */
export const nav: NavState = $state({ view: DEFAULT_VIEW, tab: emptyTabs(), rail: "auto" });

const history: HistoryEntry[] = $state([]);

function storage(): Storage | null {
  try {
    return globalThis.localStorage ?? null;
  } catch {
    return null;
  }
}

function read(key: string): string | null {
  try {
    return storage()?.getItem(key) ?? null;
  } catch {
    return null;
  }
}

function write(key: string, value: string): void {
  try {
    storage()?.setItem(key, value);
  } catch {
    /* storage full or blocked: the preference just doesn't persist */
  }
}

/** Is `v` one of the view ids? */
export function isViewId(v: unknown): v is ViewId {
  return typeof v === "string" && (VIEW_IDS as readonly string[]).includes(v);
}

/** Views a launch doesn't reopen: Browse loads the catalog, which nobody asked for yet. */
const NOT_RESTORED: readonly ViewId[] = ["browse"];

/** Load the persisted view and rail pin (called at module load; tests call it again). */
export function restoreNav(): void {
  const view = read(NAV_VIEW_KEY);
  nav.view = isViewId(view) && !NOT_RESTORED.includes(view) ? view : DEFAULT_VIEW;
  const rail = read(NAV_RAIL_KEY);
  nav.rail = RAIL_PINS.includes(rail as RailPin) ? (rail as RailPin) : "auto";
}

/** Back to defaults with empty history. Test helper; does not touch storage. */
export function resetNav(): void {
  nav.view = DEFAULT_VIEW;
  nav.tab = emptyTabs();
  nav.rail = "auto";
  history.length = 0;
}

/**
 * Go to a view, optionally to one of its tabs. A no-op when already there.
 * @param view the destination view
 * @param tab the view's sub-tab; leave undefined to keep the tab it last showed
 */
export function go(view: ViewId, tab?: string): void {
  const sameView = view === nav.view;
  if (sameView && (tab === undefined || tab === nav.tab[view])) return;
  history.push({ view: nav.view, tab: nav.tab[nav.view] });
  if (history.length > HISTORY_MAX) history.splice(0, history.length - HISTORY_MAX);
  if (tab !== undefined) nav.tab[view] = tab;
  if (!sameView) {
    nav.view = view;
    write(NAV_VIEW_KEY, view);
  }
}

/**
 * Return to the previous view (and the tab it showed then).
 * @return true when there was somewhere to go back to
 */
export function back(): boolean {
  const prev = history.pop();
  if (!prev) return false;
  nav.tab[prev.view] = prev.tab;
  if (prev.view !== nav.view) {
    nav.view = prev.view;
    write(NAV_VIEW_KEY, prev.view);
  }
  return true;
}

/** Is there a history entry for back()? */
export function canGoBack(): boolean {
  return history.length > 0;
}

/** How many entries back() can walk (capped at HISTORY_MAX). */
export function historyDepth(): number {
  return history.length;
}

/**
 * Set a view's sub-tab without adding a history entry (for a view's own Tabs).
 * @param view the view whose tab changes
 * @param tab the new tab id
 */
export function setTab(view: ViewId, tab: string | undefined): void {
  nav.tab[view] = tab;
}

/** Set and persist the rail pin. */
export function setRailPin(pin: RailPin): void {
  nav.rail = pin;
  write(NAV_RAIL_KEY, pin);
}

/**
 * Whether the rail shows icons only.
 * @param width the window's inner width in CSS px
 * @param pin the user's rail preference
 */
export function railCollapsedFor(width: number, pin: RailPin): boolean {
  if (pin === "expanded") return false;
  if (pin === "collapsed") return true;
  return width < RAIL_COLLAPSE_BELOW;
}

/**
 * The pin to store when the user flips the rail. When the new state is what "auto" would give at
 * this width, it goes back to "auto" so a one-off flip doesn't stick across window sizes.
 * @param collapsed whether the rail is collapsed now
 * @param width the window's inner width in CSS px
 */
export function nextRailPin(collapsed: boolean, width: number): RailPin {
  const wantCollapsed = !collapsed;
  if (railCollapsedFor(width, "auto") === wantCollapsed) return "auto";
  return wantCollapsed ? "collapsed" : "expanded";
}

/** Header search placeholder for a view. */
export function searchPlaceholderFor(view: ViewId): string {
  return view === "browse" ? "Search the catalog" : "Search your mods";
}

/** Which view a header search runs in: Browse searches the catalog, everything else the Library. */
export function searchTargetFor(view: ViewId): "library" | "browse" {
  return view === "browse" ? "browse" : "library";
}

// Browser only: under plain Node, touching localStorage at import prints an experimental warning.
if (typeof window !== "undefined") restoreNav();
