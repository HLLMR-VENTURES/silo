<script lang="ts">
  import { onMount, tick, untrack } from "svelte";
  import { get } from "svelte/store";
  import {
    DialogHost,
    Menu,
    ToastRegion,
    ask,
    askText,
    toast,
    type ConfirmFact,
    type ToastHandle,
  } from "./lib/ui";
  import {
    modsRootStatus,
    setModsRoot,
    clearModsRoot,
    pointGameAtModsRoot,
    createModsFolder,
    folderLooksEmpty,
    pickFolderFor,
    type ModsRootStatus,
    scanMods,
    onScanProgress,
    parseCollectionDeepLink,
    getCuration,
    setCuration,
    getOverrides,
    setOverride,
    planOrganize,
    applyOrganize,
    setActive,
    flatten,
    clearScanCache,
    getLoadouts,
    saveLoadout,
    deleteLoadout,
    exportLoadoutFile,
    importLoadoutFile,
    getSavegames,
    backupSavegame,
    detectConflicts,
    detectGame,
    launchGame,
    modsWithSettings,
    getTags,
    setTags,
    getModRepos,
    getModSources,
    markSourceChecked,
    openExternal,
    checkModUpdate,
    catalogCheckUpdates,
    downloadUpdate,
    appBusy,
    forceQuit,
    setSelfUpdating,
    onCloseBlocked,
    saveTextFile,
    diagnosticsBundle,
    copyText,
    userDirPath,
    bisectSnapshotGet,
    bisectSnapshotClear,
    appVersion,
    takeStartupNotices,
    detectForeignFiles,
    adoptForeignFile,
    restoreForeignFile,
    relinkProjections,
    onRelinkProgress,
    hardlinkSupport,
    type ForeignFile,
    confirmDialog,
    archiveMovePreflight,
    archiveMoveStart,
    archiveUseDefault,
    archiveRepoint,
    archiveAdoptPreview,
    archiveAdoptApply,
    type ArchivePreflight,
    openKnownFolder,
  } from "./lib/api";
  import { ArchiveMove } from "./lib/archiveMove.svelte";
  import { capList, modsCount, problemLines, unDot } from "./lib/archiveMoveLogic";
  import {
    activeSetDiff,
    findLoadoutByName,
    loadoutNameCheck,
    sameMods,
    type ActiveSetDiff,
  } from "./lib/loadoutSafety";
  import { onOpenUrl, getCurrent } from "@tauri-apps/plugin-deep-link";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { checkUpdate, installUpdate, type AvailableUpdate } from "./lib/updater";
  import {
    GAME_GRAPHICS_FIELDS,
    GAME_GRAPHICS_PRESETS,
    SAVEGAME_FIELDS,
    SAVEGAME_PRESETS,
    type CfgField,
    type CfgPreset,
  } from "./lib/configSchemas";
  import type {
    ModEntry,
    ScanResult,
    CurationRow,
    ModInput,
    PlannedMove,
    Loadout,
    Savegame,
    Conflict,
    GameInfo,
    UpdateRow,
    SourceRow,
  } from "./lib/types";
  import { checkYourselfList, type CheckYourselfRow } from "./lib/sources";
  import { copyOrSave } from "./lib/clipboard";
  import { parseUiScale, type UiScale } from "./lib/uiScale";
  import {
    blocked,
    blockedForeground,
    current as currentOp,
    start,
    statusLineOp,
    tryStart,
    type OpToken,
  } from "./lib/state/ops.svelte";
  import { rescanGate } from "./lib/state/rescanGate";
  import { describeError, MOVE_UNFINISHED_COPY } from "./lib/errorCopy";
  import {
    info,
    inlineError,
    reportError,
    setNotice,
    success,
    syncNotice,
  } from "./lib/state/notify.svelte";
  import { archiveNotice, ARCHIVE_MOVE_KEY, ARCHIVE_UNAVAILABLE_KEY } from "./lib/archiveMoveNotice";

  const CATEGORIES = [
    "Maps",
    "Tractors",
    "Harvesters",
    "Implements",
    "Cars & Trucks",
    "Vehicles",
    "Placeables",
    "Objects",
    "Decorations",
    "Textures",
    "Sounds",
    "Realism",
    "Cheats",
    "Scripts & Tools",
    "Other",
  ];
  import VirtualList from "./lib/components/library/VirtualList.svelte";
  import VirtualGrid from "./lib/components/library/VirtualGrid.svelte";
  import ModListRow, { ROW_HEIGHTS, type Density } from "./lib/components/library/ModListRow.svelte";
  import ModCard, { CARD_HEIGHT } from "./lib/components/library/ModCard.svelte";
  import { rowBadges } from "./lib/components/library/rowBadges";
  import type { MenuTarget, SelectIntent } from "./lib/components/library/roving";
  import {
    loadLibraryView,
    saveLibraryView,
    type LibraryViewMode,
  } from "./lib/components/library/viewPref";
  import { getVerdict } from "./lib/provenanceCache.svelte";
  import FilterBar from "./lib/components/FilterBar.svelte";
  import ModSettings from "./lib/components/ModSettings.svelte";
  import ModDetail from "./lib/components/ModDetail.svelte";
  import ConfigEditor from "./lib/components/ConfigEditor.svelte";
  import ModBrowser from "./lib/components/ModBrowser.svelte";
  import LogTriage from "./lib/components/LogTriage.svelte";
  import BindingsView from "./lib/components/BindingsView.svelte";
  import MpSync from "./lib/components/MpSync.svelte";
  import Collections from "./lib/components/Collections.svelte";
  import BridgeTool from "./lib/components/BridgeTool.svelte";
  import ConflictsPanel from "./lib/components/panels/ConflictsPanel.svelte";
  import HealthPanel from "./lib/components/panels/HealthPanel.svelte";
  import StatsPanel from "./lib/components/panels/StatsPanel.svelte";
  import UpdatesPanel from "./lib/components/panels/UpdatesPanel.svelte";
  import LoadoutsPanel from "./lib/components/panels/LoadoutsPanel.svelte";
  import SavegamesPanel from "./lib/components/panels/SavegamesPanel.svelte";
  import GeneralSettings from "./lib/components/settings/GeneralSettings.svelte";
  import ModsFolderSettings from "./lib/components/settings/ModsFolderSettings.svelte";
  import LibrarySettings from "./lib/components/settings/LibrarySettings.svelte";
  import AccountSettings from "./lib/components/settings/AccountSettings.svelte";
  import DiagnosticsSettings from "./lib/components/settings/DiagnosticsSettings.svelte";
  import MaintenanceSettings from "./lib/components/settings/MaintenanceSettings.svelte";
  import AboutSettings from "./lib/components/settings/AboutSettings.svelte";
  import CategoryMenu from "./lib/components/panels/CategoryMenu.svelte";
  import LibraryToolbar, { type StatFilter } from "./lib/components/panels/LibraryToolbar.svelte";
  import OrganizePreview from "./lib/components/OrganizePreview.svelte";
  import NoticeArea from "./lib/components/NoticeArea.svelte";
  import ViewBoundary from "./lib/components/ViewBoundary.svelte";
  import KeepMounted from "./lib/components/KeepMounted.svelte";
  import AppShell from "./lib/components/shell/AppShell.svelte";
  import AppHeader from "./lib/components/shell/AppHeader.svelte";
  import ShortcutsDialog from "./lib/components/shell/ShortcutsDialog.svelte";
  import { cycleRegion, regionOf } from "./lib/components/shell/regions";
  import { BISECT_BADGE } from "./lib/components/shell/navItems";
  import { installShortcuts, shortcutsBlocked, type ShortcutAction } from "./lib/state/shortcuts";
  import { modalOpen } from "./lib/ui/overlayStack";
  import StatusChip from "./lib/components/shell/StatusChip.svelte";
  import { libraryReadState } from "./lib/components/shell/status";
  import PlayButton, { type PlayMenuItem } from "./lib/components/shell/PlayButton.svelte";
  import type { StatusStripItem } from "./lib/components/shell/StatusStrip.svelte";
  import ViewOutlet from "./lib/views/ViewOutlet.svelte";
  import LibraryView from "./lib/views/LibraryView.svelte";
  import BrowseView from "./lib/views/BrowseView.svelte";
  import ModSetsView from "./lib/views/ModSetsView.svelte";
  import DiagnoseView from "./lib/views/DiagnoseView.svelte";
  import UpdatesView from "./lib/views/UpdatesView.svelte";
  import ToolsView from "./lib/views/ToolsView.svelte";
  import SettingsView from "./lib/views/SettingsView.svelte";
  import HelpView from "./lib/views/HelpView.svelte";
  import Button from "./lib/ui/Button.svelte";
  import {
    activeSetLine,
    libraryStatusCounts,
    stripItemSpecs,
    stripLeft,
  } from "./lib/views/libraryStatus";
  import { back, go, nav, searchPlaceholderFor, searchTargetFor } from "./lib/state/nav.svelte";
  import type { MenuItem } from "./lib/ui/overlayTypes";
  import Icon from "./lib/ui/Icon.svelte";
  import { fmtSize } from "./lib/format";
  import {
    MODS_FOLDER_KEY,
    OVERRIDE_FOLLOWED_KEY,
    OVERRIDE_KEY,
    modsFolderNotices,
    rootsFor,
    showOverrideNotice,
    switchBlockReason as switchGate,
  } from "./lib/modsStatus";

  let roots = $state<string[]>([]);
  let modsStatus = $state<ModsRootStatus | null>(null);
  let overrideNoticeOpen = $state(false);
  let mods = $state<ModEntry[]>([]);
  // True once a scan of the current mods folder finished (even with no mods), so "not in your
  // library" counts are only shown against a library Silo has actually read.
  let libraryScanned = $state(false);
  let foreignFiles = $state<ForeignFile[]>([]);
  let scanning = $state(false);
  // The last scan failed (its notice is up): the status chip says so instead of "All good".
  let scanFailed = $state(false);
  // Epoch-ms of the last scan attempt — throttles the auto-rescan-on-focus so alt-tabbing
  // doesn't re-scan on every focus.
  let lastScanAt = 0;
  /** The running scan's op (progress events report into it). */
  let scanOp: OpToken | null = null;
  let progress = $state({ done: 0, total: 0 });
  let result = $state<ScanResult | null>(null);
  let query = $state("");
  // applyActive() and bisectApply() still report through these two names, so their bodies stay
  // exactly as they were. Nothing else writes them: this adapter hands each message on (an
  // error toast or an info toast) and empties the channel again.
  let errorMsg = $state<string | null>(null);
  let infoMsg = $state<string | null>(null);
  $effect(() => {
    const err = errorMsg;
    const note = infoMsg;
    if (err === null && note === null) return;
    untrack(() => {
      errorMsg = null;
      infoMsg = null;
      if (err !== null) reportError(err, "Applying your mods");
      if (note !== null) info(note, { duration: 10_000 });
    });
  });
  // Library category filter, driven by the shared FilterBar's parent/child dropdowns
  // ("" = all). Replaces the old category rail + `selected`.
  let libCat = $state("");
  let libSub = $state("");
  let curationMap = $state<Record<string, CurationRow>>({});
  let overrideMap = $state<
    Record<string, { category: string; subcategory: string | null }>
  >({});
  let showHidden = $state(false);
  let favoritesOnly = $state(false);
  let flaggedOnly = $state(false);
  let conflictedOnly = $state(false);
  let hasSettingsOnly = $state(false);
  let needsUpdateOnly = $state(false);
  // A single categorical facet (maps/scripts/uniqueType/active), from LibraryToolbar's "show only"
  // counts (they were the stat bar's) — click one to scope the list to it; again to clear.
  let statFilter = $state<StatFilter>("");
  let editing = $state<{ techName: string; x: number; y: number } | null>(null);
  let activeSet = $state<Set<string>>(new Set());
  // Library writes hold the "library-write" op class, one at a time: a second is refused with the
  // reason. `writeLock` counts every holder (the archive move too) and turns off the controls
  // that write; `fgLock` leaves out background ops (the archive move), so navigation, Settings,
  // read-only panels and scans stay usable during a move. Writes started from those still refuse
  // in beginWrite.
  const writeLock = $derived(blocked("library-write"));
  const fgLock = $derived(blockedForeground("library-write"));
  /** The op the status line under the top bar names (quiet ops show their progress elsewhere). */
  const statusOp = $derived(statusLineOp());
  /** Say why something the user asked for can't happen right now (nothing failed). */
  function refuse(why: string) {
    toast({ message: why, tone: "warn" });
  }
  /**
   * Start a library write, or say why it can't start now.
   * @param label what Silo is doing, fits "Silo is …" ("Filing 3 mods into the library")
   * @param progress show a progress bar for it
   * @param opts `silent`: refuse without a toast (background work that retries on its own);
   *   `duringMove`: it doesn't touch the library archive (a savegame backup), so a running or
   *   paused archive move doesn't stop it; only a foreground write does
   * @return the op's token (end it before rescanning), or null when refused
   */
  function beginWrite(
    label: string,
    progress = false,
    opts: { silent?: boolean; duringMove?: boolean } = {},
  ): OpToken | null {
    const say = (why: string) => {
      if (!opts.silent) refuse(why);
    };
    if (opts.duringMove) {
      const fg = blockedForeground("library-write");
      if (fg.blocked) {
        say(`${fg.reason} Try again when it's done.`);
        return null;
      }
      return start(label, { exclusive: "library-write", progress });
    }
    // A paused (or otherwise unfinished) move: nothing changes the library until it's resumed
    // and finished, or cancelled. A running one holds the class itself (below).
    if (archiveMove.unfinished && !archiveMove.running) {
      say(MOVE_UNFINISHED_COPY);
      return null;
    }
    const op = tryStart(label, { exclusive: "library-write", progress });
    if (!op) say(`${blocked("library-write").reason} Try again when it's done.`);
    return op;
  }
  /** The scan's own notice (it failed, or recovery left problems). A scan touches no other key. */
  const SCAN_KEY = "scan";
  /** What Silo did while starting (e.g. restored the database), until dismissed. */
  const STARTUP_KEY = "startup";
  // Mods folder is on a cloud-sync filesystem that can't do hardlinks → Silo uses copies.
  let cloudSynced = $state(false);
  let cloudNoticeOpen = $state(false);
  function dismissCloudNotice() {
    cloudNoticeOpen = false;
    try {
      localStorage.setItem("silo.cloudNoticeDismissed", "1");
    } catch {
      /* private-mode / storage-off — the notice just returns next launch */
    }
  }
  // Auto-file newly-appeared mods into the archive on load (kept active). OPT-IN:
  // defaults OFF so a first run never moves the user's files without them choosing to.
  // Enable it via Settings, or use the explicit "Organize" button. Persisted.
  let autoFileNew = $state(
    typeof localStorage !== "undefined"
      ? localStorage.getItem("silo.autoFile") === "true"
      : false,
  );
  function setAutoFile(v: boolean) {
    autoFileNew = v;
    try {
      localStorage.setItem("silo.autoFile", String(v));
    } catch {}
  }

  // Appearance: system (follow OS) / light / dark. Persisted; applied to <html>.
  let theme = $state<"system" | "light" | "dark">(
    (typeof localStorage !== "undefined"
      ? (localStorage.getItem("silo.theme") as "system" | "light" | "dark" | null)
      : null) ?? "system",
  );
  function setTheme(t: "system" | "light" | "dark") {
    theme = t;
    try {
      localStorage.setItem("silo.theme", t);
    } catch {}
  }
  $effect(() => {
    const el = document.documentElement;
    if (theme === "system") el.removeAttribute("data-theme");
    else el.setAttribute("data-theme", theme);
  });

  // UI size: scales the root font size (rem-based tokens follow). Persisted; applied to <html>.
  let uiScale = $state<UiScale>(
    parseUiScale(typeof localStorage !== "undefined" ? localStorage.getItem("silo.uiScale") : null),
  );
  function setUiScale(s: UiScale) {
    uiScale = s;
    try {
      localStorage.setItem("silo.uiScale", String(s));
    } catch {}
  }
  $effect(() => {
    document.documentElement.style.setProperty("--ui-scale", String(uiScale));
  });

  // Silo only organizes .zip mods (the commercial distribution format). Unpacked
  // dir mods are left in place — that's how dev/work-in-progress mods live until
  // they're officially packaged.
  const isFileable = (m: ModEntry) => !m.organized && m.kind === "zip";
  const organizedCount = $derived(mods.filter((m) => m.organized).length);
  const unorganizedCount = $derived(mods.filter(isFileable).length);

  let loadouts = $state<Loadout[]>([]);
  let savegames = $state<Savegame[]>([]);
  let conflicts = $state<Conflict[]>([]);
  let conflictTimer: ReturnType<typeof setTimeout> | undefined;
  // Set when a silo://collection?url=… deep link arrives — opens Mod sets → Share (Collections),
  // pre-filled. Leaving that tab forgets it, as closing the Collections panel did.
  let collImportUrl = $state<string | null>(null);
  $effect(() => {
    const onShare = nav.view === "modSets" && nav.tab.modSets === "share";
    if (!onShare) untrack(() => (collImportUrl = null));
  });

  function handleDeepLinks(urls: string[] | null | undefined) {
    if (!urls) return;
    for (const raw of urls) {
      const target = parseCollectionDeepLink(raw);
      if (target) {
        collImportUrl = target;
        go("modSets", "share");
        break;
      }
    }
  }
  // Active set as manifest refs for MP sync (techName, path, kind, version).
  const activeModRefs = $derived(
    mods
      .filter((m) => activeSet.has(m.techName))
      .map((m) => ({ techName: m.techName, path: m.path, kind: m.kind, version: m.version })),
  );
  // Set when a previous bisection didn't finish (app closed mid-run) — the user's mod
  // set may be partially applied, so offer to restore it.
  let bisectRecovery = $state<string[] | null>(null);
  // True while a guided bisection is running (snapshot saved, test halves applied). The
  // focus rescan holds off meanwhile so returning from FS25 can't rewrite the active set.
  let bisecting = $state(false);
  /** Diagnose → Last run's slot while that tab is on screen (DiagnoseView binds it), else null. */
  let diagnoseLastRunSlot = $state<HTMLElement | null>(null);
  // The current view is nav.view (lib/state/nav). The mod drawer belongs to the Library: leaving
  // it closes the drawer, as switching to Browse did. The Browse drawer lives inside ModBrowser,
  // which unmounts on switch, so it closes itself.
  $effect(() => {
    if (nav.view !== "library") untrack(() => (detailMod = null));
  });
  let userDir = $state<string | null>(null);
  let appVer = $state<string | null>(null);
  let update = $state<AvailableUpdate | null>(null);
  let updating = $state(false);
  let updatePct = $state<number | null>(null);

  const FINISH_FIRST = "Finish what Silo is doing first, then update.";

  /** Silo is in the middle of something an install (which relaunches) must not cut short. */
  async function changingFiles(): Promise<boolean> {
    let writing = false;
    try {
      writing = await appBusy();
    } catch {
      /* the check failed; the state below still guards */
    }
    return !!(
      writeLock.blocked ||
      archiveMove.running ||
      archiveMove.unfinished ||
      bisecting ||
      bisectRecovery ||
      writing
    );
  }

  async function applyUpdate() {
    if (updating) return;
    // Installing relaunches Silo: never in the middle of a change to the library.
    if (await changingFiles()) {
      refuse(FINISH_FIRST);
      return;
    }
    updating = true;
    let blocking = false;
    try {
      // The download changes nothing. Right before installing, block new library changes in
      // the backend, then check again that nothing is running.
      const installed = await installUpdate(
        (p) => (updatePct = p),
        async () => {
          await setSelfUpdating(true);
          blocking = true;
          return !(await changingFiles());
        },
      );
      if (!installed) {
        refuse(FINISH_FIRST);
        await setSelfUpdating(false);
        blocking = false;
        updating = false;
        updatePct = null;
      }
      // Installed: Silo relaunches; if we're still here, it was a no-op.
    } catch (e) {
      reportError(e, "Updating Silo", { retry: applyUpdate });
      if (blocking) await setSelfUpdating(false).catch(() => {});
      updating = false;
      updatePct = null;
    }
  }
  let configEditor = $state<{
    title: string;
    path: string;
    fields: CfgField[];
    presets: CfgPreset[];
    footnote?: string;
  } | null>(null);

  function openGameGraphics() {
    if (!userDir) return;
    configEditor = {
      title: "Graphics settings: game.xml",
      path: `${userDir}/game.xml`,
      fields: GAME_GRAPHICS_FIELDS,
      presets: GAME_GRAPHICS_PRESETS,
      footnote: "Changes take effect the next time you launch the game.",
    };
  }

  function openSaveConfig(s: Savegame) {
    if (!userDir) return;
    configEditor = {
      title: `Configure: ${s.name}`,
      path: `${userDir}/${s.folder}/careerSavegame.xml`,
      fields: SAVEGAME_FIELDS,
      presets: SAVEGAME_PRESETS,
      footnote: "Edits this savegame's settings. Back up the save first if unsure.",
    };
  }

  const libStats = $derived.by(() => {
    let totalSize = 0;
    let rated = 0;
    let ratingSum = 0;
    let tagged = 0;
    const byCat: Record<string, { count: number; size: number }> = {};
    for (const m of effectiveMods) {
      totalSize += m.size;
      (byCat[m.category] ??= { count: 0, size: 0 });
      byCat[m.category].count++;
      byCat[m.category].size += m.size;
      const r = cur(m.techName).rating;
      if (r > 0) {
        rated++;
        ratingSum += r;
      }
      if (tagsOf(m.techName).length > 0) tagged++;
    }
    const cats = Object.entries(byCat)
      .map(([name, v]) => ({ name, ...v }))
      .sort((a, b) => b.size - a.size);
    const largest = [...effectiveMods].sort((a, b) => b.size - a.size).slice(0, 8);
    const maxCatSize = cats.reduce((m, c) => Math.max(m, c.size), 1);
    return {
      totalSize,
      cats,
      maxCatSize,
      largest,
      rated,
      avgRating: rated ? ratingSum / rated : 0,
      tagged,
    };
  });
  let gameInfo = $state<GameInfo | null>(null);
  let settingsModsSet = $state<Set<string>>(new Set());
  let settingsMod = $state<{ techName: string; title: string } | null>(null);

  async function launch() {
    if (
      criticalCount > 0 &&
      !(await ask({
        title: `Launch with ${criticalCount} critical conflict${criticalCount === 1 ? "" : "s"}?`,
        body: "The game may crash or misbehave with these mods together. Launching changes nothing in your library; you can fix the conflicts after you quit.",
        confirm: "Launch anyway",
      }))
    )
      return;
    try {
      await launchGame();
    } catch (e) {
      reportError(e, "Launching the game");
    }
  }

  // Library health: missing dependencies, corrupt/unreadable mods, and mods the
  // game silently ignores (name starts with a digit).
  const health = $derived.by(() => {
    const lib = new Set(mods.map((m) => m.techName));
    const missingDeps: { mod: ModEntry; missing: string[] }[] = [];
    const corrupt: ModEntry[] = [];
    const ignored: ModEntry[] = [];
    for (const m of mods) {
      if (m.error) corrupt.push(m);
      if (m.ignoredDigitPrefix) ignored.push(m);
      const miss = m.dependencies.filter((d) => !lib.has(d));
      if (miss.length > 0) missingDeps.push({ mod: m, missing: miss });
    }
    return { missingDeps, corrupt, ignored, foreign: foreignFiles };
  });
  const healthCount = $derived(
    health.missingDeps.length +
      health.corrupt.length +
      health.ignored.length +
      health.foreign.length,
  );

  const criticalCount = $derived(
    conflicts.filter((c) => c.severity === "critical").length,
  );

  // Every mod named in a non-info conflict, for the "In conflict" library filter.
  // Conflict.mods carries tech names (and sometimes titles), so we match on both.
  const conflictedSet = $derived.by(() => {
    const s = new Set<string>();
    for (const c of conflicts) {
      if (c.severity === "info") continue;
      for (const name of c.mods) s.add(name);
    }
    return s;
  });

  async function runConflictCheck() {
    const active = mods.filter((m) => activeSet.has(m.techName));
    if (active.length < 2) {
      conflicts = [];
      return;
    }
    try {
      conflicts = await detectConflicts(
        active.map((m) => ({
          techName: m.techName,
          title: m.title,
          path: m.path,
          kind: m.kind,
        })),
      );
    } catch (e) {
      reportError(e, "Checking for conflicts", { background: true });
    }
  }

  // Re-check conflicts (debounced) whenever the active set or library changes.
  $effect(() => {
    void activeSet;
    void mods;
    clearTimeout(conflictTimer);
    conflictTimer = setTimeout(runConflictCheck, 400);
  });

  const libraryTechNames = $derived(new Set(mods.map((m) => m.techName)));

  // Cross-reference a save's user mods (non-DLC) against the library.
  function saveStats(s: Savegame) {
    const userMods = s.mods.filter((m) => !m.isDlc);
    const present = userMods.filter((m) => libraryTechNames.has(m.modName));
    const missing = userMods.filter((m) => !libraryTechNames.has(m.modName));
    return { total: userMods.length, present, missing };
  }

  const saveRows = $derived(savegames.map((s) => ({ s, st: saveStats(s) })));

  async function loadSavegames() {
    try {
      savegames = await getSavegames();
    } catch (e) {
      reportError(e, "Reading your savegames", { background: true });
    }
  }

  function buildReport(): string {
    const L: string[] = [];
    L.push("# Silo diagnostics report");
    L.push(`Generated: ${new Date().toISOString()}`, "");
    L.push("## Environment");
    L.push(`- Mods folder: ${roots[0] ?? "not detected"}`);
    L.push(`- Game: ${gameInfo ? gameInfo.installDir : "not detected"}`, "");
    L.push("## Library");
    L.push(`- Mods: ${mods.length}  (organized ${organizedCount}, active ${activeSet.size})`);
    L.push(`- Total size: ${fmtSize(libStats.totalSize)}`);
    L.push(`- Maps: ${stats.maps}  ·  Script mods: ${stats.scripts}  ·  Tagged: ${libStats.tagged}`, "");
    L.push("### By category");
    for (const c of libStats.cats) L.push(`- ${c.name}: ${c.count}  (${fmtSize(c.size)})`);
    L.push("");
    const al = loadouts.find((l) => l.id === activeLoadoutId);
    L.push(`## Active set${al ? `, loadout “${al.name}”` : ""} (${activeSet.size} mods)`, "");
    L.push(`## Conflicts (${criticalCount} critical, ${conflicts.length} total)`);
    for (const c of conflicts.filter((x) => x.severity !== "info")) {
      L.push(`- [${c.severity}] ${c.kind} “${c.name}”: ${c.mods.join(", ")}`);
    }
    L.push("");
    L.push(`## Health (${healthCount} issues)`);
    for (const d of health.missingDeps) {
      L.push(`- Missing dependency: ${d.mod.title ?? d.mod.techName} needs ${d.missing.join(", ")}`);
    }
    for (const m of health.ignored) L.push(`- Ignored (digit prefix): ${m.techName}`);
    for (const m of health.corrupt) L.push(`- Corrupt: ${m.techName}: ${m.error}`);
    for (const f of health.foreign)
      L.push(`- Foreign file at a managed name: ${f.fileName} (isn't Silo's, left untouched)`);
    return L.join("\n");
  }

  async function exportReport() {
    try {
      await saveTextFile("silo-report.md", await diagnosticsBundle(buildReport(), false));
    } catch (e) {
      reportError(e, "Exporting the report");
    }
  }

  async function copyDiagnostics() {
    try {
      const text = await diagnosticsBundle(buildReport(), true);
      const out = await copyOrSave(text, "silo-diagnostics.md", copyText, saveTextFile);
      if (out === "copied") success("Diagnostics copied. Paste them into your bug report.");
      else if (out === "saved")
        info("Silo couldn't use the clipboard, so it saved the diagnostics to a file instead.");
    } catch (e) {
      reportError(e, "Copying diagnostics");
    }
  }

  async function backupSave(s: Savegame) {
    // Held as a write like before (it copies files): nothing else changes them meanwhile. It
    // never touches the library archive, so an archive move doesn't stop it.
    const op = beginWrite(`Backing up ${s.name}`, false, { duringMove: true });
    if (!op) return;
    try {
      await backupSavegame(s.folder);
      success(`Backed up ${s.name}.`);
    } catch (e) {
      reportError(e, `Backing up ${s.name}`);
    } finally {
      op.end();
    }
  }

  async function loadoutFromSave(s: Savegame) {
    const { present, missing } = saveStats(s);
    if (present.length === 0) {
      refuse(`None of ${s.name}'s mods are in your library yet.`);
      return;
    }
    const target = present.map((m) => m.modName);
    const diff = activeSetDiff([...activeSet], target, libraryScanned ? libraryTechNames : undefined);
    const sum = switchSummary(diff, missing.length);
    const note =
      missing.length > 0
        ? ` ${modsCount(missing.length)} the save used ${missing.length === 1 ? "isn't" : "aren't"} in your library and ${missing.length === 1 ? "is" : "are"} left out.`
        : "";
    if (
      !(await ask({
        title: `Use ${s.name}'s mods?`,
        body: `Silo saves ${modsCount(present.length)} from this save as a loadout called ${s.name} and switches to it. ${sum.body}${note}`,
        facts: sum.facts,
        list: sum.list,
        confirm: "Switch",
      }))
    )
      return;
    try {
      const id = await saveLoadout(null, s.name, target);
      await loadLoadouts();
      // Apply exactly the mods the confirmation showed (already confirmed above). The new
      // loadout is found by its id, never by name: another loadout may share the save's name.
      const name = loadouts.find((l) => l.id === id)?.name ?? s.name;
      if (diff.on.length + diff.off.length === 0) toast({ message: `Already using ${name}` });
      else await switchTo(name, target);
    } catch (e) {
      reportError(e, "Switching to the save's mods");
    }
  }

  /**
   * Body, facts and list for a "switch the active set" confirmation.
   * @param diff what the switch changes (activeSetDiff)
   * @param missingCount mods the target names that aren't in the library (default diff.missing)
   */
  function switchSummary(diff: ActiveSetDiff, missingCount = diff.missing.length) {
    const titles = new Map(mods.map((m) => [m.techName, m.title ?? m.techName]));
    const label = (t: string) => titles.get(t) ?? t;
    const on = diff.on.length;
    const off = diff.off.length;
    const facts: ConfirmFact[] = [
      { label: "Turn on", value: modsCount(on) },
      { label: "Park", value: modsCount(off) },
      ...(missingCount ? [{ label: "Not in your library", value: modsCount(missingCount) }] : []),
    ];
    const changed = [...diff.on.map((t) => `On: ${label(t)}`), ...diff.off.map((t) => `Park: ${label(t)}`)];
    const list = changed.slice(0, 50);
    if (changed.length > 50) list.push(`…and ${changed.length - 50} more`);
    const body = `${modsCount(on)} ${on === 1 ? "turns" : "turn"} on and ${off} ${off === 1 ? "is" : "are"} parked. Nothing is deleted.`;
    return { body, facts, list };
  }

  /** Set the active set through the loadout apply path (optimistic set, then project it).
   *  @return true when it applied without errors (failures are reported), null when another
   *  library write was running so nothing changed */
  async function setActiveSetTo(techNames: string[], label: string): Promise<boolean | null> {
    const op = beginWrite(label);
    if (!op) return null;
    activeSet = new Set(techNames);
    let clean = false;
    try {
      const rep = await applyActive(techNames);
      clean = rep.errors.length === 0;
    } catch (e) {
      reportError(e, "Applying your mods");
    } finally {
      op.end();
    }
    return clean;
  }

  /** The Undo notice of the last loadout switch; a new switch dismisses it. */
  let switchUndo: ToastHandle | null = null;

  /**
   * Why an Undo that puts the active set back can't run now, or null.
   * @param expected the active set the Undo assumes (the one the switch left)
   */
  function activeUndoBlocked(expected: string[]): string | null {
    if (bisecting) return "Can't undo while a diagnosis is running.";
    if (archiveMove.running) return "Can't undo while a library move is running.";
    if (!sameMods(expected, activeSet)) return "Can't undo: your active mods changed since the switch.";
    return null;
  }

  /** Switch to a loadout's mods (already confirmed), then offer to switch back. Errors the
   *  apply reports are reported on their own; the switch toast still shows so Undo is reachable. */
  async function switchTo(name: string, techNames: string[]) {
    switchUndo?.dismiss();
    switchUndo = null;
    const previous = [...activeSet];
    const clean = await setActiveSetTo(techNames, `Switching to ${name}`);
    if (clean === null) return; // refused: another write is running, nothing changed
    const switched = [...techNames];
    switchUndo = toast({
      message: clean ? `Switched to ${name}.` : `Switch to ${name} finished with problems.`,
      tone: clean ? "neutral" : "warn",
      action: {
        label: "Undo",
        run: async () => {
          const why = activeUndoBlocked(switched);
          if (why) {
            toast({ message: why, tone: "warn" });
            return;
          }
          await setActiveSetTo(previous, "Switching back");
        },
      },
    });
  }

  /**
   * After a loadout was overwritten, offer to put its previous mods back.
   * @param prev the loadout as it was (id, name, mods)
   * @param written the mods it was overwritten with; Undo refuses if it changed again since
   */
  function offerOverwriteUndo(prev: { id: number; name: string; mods: string[] }, written: string[]) {
    toast({
      message: `Saved your current mods to ${prev.name}.`,
      action: {
        label: "Undo",
        run: async () => {
          const now = loadouts.find((l) => l.id === prev.id);
          if (!now || !sameMods(now.mods, written)) {
            toast({ message: `Can't undo: ${prev.name} changed since.`, tone: "warn" });
            return;
          }
          try {
            await saveLoadout(prev.id, prev.name, prev.mods);
            await loadLoadouts();
          } catch (e) {
            reportError(e, "Undoing the loadout change");
          }
        },
      },
    });
  }

  // The loadout whose mod set exactly matches the current active set (if any).
  const activeLoadoutId = $derived.by(() => {
    for (const l of loadouts) {
      if (
        l.mods.length === activeSet.size &&
        l.mods.every((m) => activeSet.has(m))
      ) {
        return l.id;
      }
    }
    return null;
  });

  async function loadLoadouts() {
    try {
      loadouts = await getLoadouts();
    } catch (e) {
      reportError(e, "Loading your loadouts", { retry: loadLoadouts, background: true });
    }
  }

  async function applyLoadout(l: Loadout) {
    // With no scan yet every mod would look missing, so only count missing once mods are known.
    const diff = activeSetDiff([...activeSet], l.mods, libraryScanned ? libraryTechNames : undefined);
    if (diff.on.length + diff.off.length === 0) {
      toast({ message: `Already using ${l.name}` });
      return;
    }
    const sum = switchSummary(diff);
    if (
      !(await ask({
        title: `Switch to ${l.name}?`,
        body: sum.body,
        facts: sum.facts,
        list: sum.list,
        confirm: "Switch",
      }))
    )
      return;
    await switchTo(l.name, l.mods);
  }

  /** Loadout saves, replaces, deletes and imports wait for a foreground library change (the
   *  panel's buttons are off then too); a library move doesn't stop them. True when refused. */
  function loadoutEditRefused(): boolean {
    const fg = blockedForeground("library-write");
    if (!fg.blocked) return false;
    refuse(`${fg.reason} Try again when it's done.`);
    return true;
  }

  async function saveCurrentLoadout() {
    if (loadoutEditRefused()) return;
    const name = await askText({
      title: "Save current mods as a loadout",
      label: "Name",
      confirm: "Save",
      validate: (v) =>
        loadoutNameCheck(
          v,
          loadouts.map((l) => l.name),
        ),
    });
    if (name === null || !name.trim()) return;
    if (loadoutEditRefused()) return;
    // A duplicate name was accepted with "Replace": overwrite that loadout, don't add a second.
    const existing = findLoadoutByName(name, loadouts);
    const current = [...activeSet];
    try {
      if (existing) {
        const prev = { id: existing.id, name: existing.name, mods: [...existing.mods] };
        await saveLoadout(existing.id, existing.name, current);
        await loadLoadouts();
        offerOverwriteUndo(prev, current);
      } else {
        await saveLoadout(null, name.trim(), current);
        await loadLoadouts();
      }
    } catch (e) {
      reportError(e, "Saving the loadout");
    }
  }

  async function overwriteLoadout(l: Loadout) {
    if (loadoutEditRefused()) return;
    const current = [...activeSet];
    const d = activeSetDiff(l.mods, current);
    if (
      !(await ask({
        title: `Replace ${l.name} with your current ${modsCount(current.length)}?`,
        body:
          `It had ${l.mods.length}. ${d.on.length} ${d.on.length === 1 ? "is" : "are"} added and ${d.off.length} removed. ` +
          "Your active mods aren't changed.",
        confirm: "Replace",
      }))
    )
      return;
    if (loadoutEditRefused()) return;
    const prev = { id: l.id, name: l.name, mods: [...l.mods] };
    try {
      await saveLoadout(l.id, l.name, current);
      await loadLoadouts();
      offerOverwriteUndo(prev, current);
    } catch (e) {
      reportError(e, "Saving the loadout");
    }
  }

  async function removeLoadout(l: Loadout) {
    if (loadoutEditRefused()) return;
    if (
      !(await ask({
        title: `Delete ${l.name}?`,
        body: "Your mods aren't affected.",
        confirm: "Delete",
        tone: "danger",
      }))
    )
      return;
    if (loadoutEditRefused()) return;
    // Kept for Undo: saving it again recreates it with the same name and mods (under a new id).
    const copy = { name: l.name, mods: [...l.mods] };
    try {
      await deleteLoadout(l.id);
      await loadLoadouts();
      toast({
        message: `Deleted ${copy.name}.`,
        action: {
          label: "Undo",
          run: async () => {
            try {
              await saveLoadout(null, copy.name, copy.mods);
              await loadLoadouts();
            } catch (e) {
              reportError(e, "Restoring the loadout");
            }
          },
        },
      });
    } catch (e) {
      reportError(e, "Deleting the loadout");
    }
  }

  async function exportLoadout(l: Loadout) {
    try {
      await exportLoadoutFile(l.id, l.name);
    } catch (e) {
      reportError(e, "Exporting the loadout");
    }
  }

  async function importLoadout() {
    if (loadoutEditRefused()) return;
    try {
      const id = await importLoadoutFile();
      if (id !== null) {
        await loadLoadouts();
        go("modSets", "sets");
      }
    } catch (e) {
      reportError(e, "Importing a loadout");
    }
  }

  function fileName(path: string): string {
    return path.split(/[\\/]/).pop() ?? path;
  }

  // Apply an active set and surface any warnings the projection engine reports — e.g. it
  // refused to remove a file you'd swapped in at a mod's name. These come back in the
  // report, not as a thrown error, so a plain `await setActive` would drop them silently.
  async function applyActive(techNames: string[]) {
    const rep = await setActive(techNames);
    if (rep.errors.length) errorMsg = rep.errors.slice(0, 3).join("; ");
    if (rep.notices?.length) infoMsg = rep.notices.join(" ");
    return rep;
  }

  // Guided bisection applies its test halves and the final restore through here, so
  // problems and notices reach the user. Rejects unless the set applied cleanly, so the
  // flow stops (and keeps its snapshot) instead of carrying on with the wrong mods.
  async function bisectApply(techNames: string[]) {
    const rep = await applyActive(techNames).catch((e) => {
      errorMsg = String(e);
      throw e;
    });
    // A plain string, which BisectFlow's String(e) shows as is.
    if (rep.errors.length) throw rep.errors.slice(0, 3).join("; ");
  }

  /** One bisection round (or its final restore) as a library write. Refused, it rejects with
   *  the reason, so the flow stops and keeps its snapshot. */
  async function bisectRound(techNames: string[]) {
    const op = tryStart("Applying the next test set", { exclusive: "library-write" });
    if (!op) throw blocked("library-write").reason;
    try {
      await bisectApply(techNames);
    } finally {
      op.end();
    }
  }

  // When a bisection ends, a snapshot still on disk means the restore didn't finish (or
  // the panel was closed mid-run): offer the restore banner now rather than next launch.
  // After a clean finish, rescan so the library shows the restored set.
  async function bisectingChanged(active: boolean) {
    const wasRunning = bisecting;
    bisecting = active;
    if (active) return;
    try {
      bisectRecovery = await bisectSnapshotGet();
    } catch {
      return; /* non-critical */
    }
    if (wasRunning && !bisectRecovery) await runScan(false);
  }

  async function toggleActive(techName: string) {
    const turningOff = activeSet.has(techName);
    const mod = effectiveMods.find((m) => m.techName === techName);
    // A loose (unorganized) mod isn't a Silo projection — it's the user's own file sitting in the
    // live folder, so set_active can't remove it (and would silently no-op, the toggle bouncing
    // back on the next scan). To park it, file it into the library (flat→archive, no re-project):
    // it leaves the live folder but stays in the library. Reversible — toggling it on re-projects.
    if (turningOff && mod && isFileable(mod)) {
      await applyLoose(
        [
          {
            techName: mod.techName,
            fileName: fileName(mod.path),
            kind: mod.kind,
            category: mod.category,
            subcategory: mod.subcategory,
          },
        ],
        [mod.techName],
        false,
      );
      return;
    }
    const op = beginWrite(turningOff ? "Parking a mod" : "Activating a mod");
    if (!op) return;
    const before = activeSet;
    const next = new Set(activeSet);
    if (next.has(techName)) next.delete(techName);
    else next.add(techName);
    activeSet = next; // optimistic: the switch moves at once
    try {
      await applyActive([...next]);
    } catch (e) {
      // Nothing was applied: put the switch back (unless something else set the mods since).
      if (activeSet === next) activeSet = before;
      reportError(e, "Changing which mods are active");
    } finally {
      op.end();
    }
  }

  // Resolve a foreign file (a mod-folder file that isn't Silo's projection — typically a newer
  // build the user dropped in to update). Adopt = promote it to the managed copy; restore = put
  // Silo's copy back. Shared by the health panel and the mod drawer's "you dropped an update" banner.
  // Returns true when it worked; false when it was refused (another write) or failed, so the
  // drawer stays open.
  async function resolveForeign(fileName: string, action: "adopt" | "restore"): Promise<boolean> {
    const op = beginWrite(action === "adopt" ? "Adopting the dropped build" : "Restoring Silo's copy");
    if (!op) return false;
    let done = false;
    try {
      if (action === "adopt") await adoptForeignFile(fileName);
      else await restoreForeignFile(fileName);
      done = true;
    } catch (e) {
      reportError(e, action === "adopt" ? "Adopting the dropped build" : "Restoring Silo's copy");
    } finally {
      op.end();
    }
    await runScan(false);
    foreignFiles = await detectForeignFiles(roots[0]).catch(() => []);
    return done;
  }

  // Build the organize inputs for every loose (unorganized) zip currently in view.
  function looseInputs(): { inputs: ModInput[]; techNames: string[] } {
    const targets = effectiveMods.filter(isFileable);
    return {
      inputs: targets.map((m) => ({
        techName: m.techName,
        fileName: fileName(m.path),
        kind: m.kind,
        category: m.category,
        subcategory: m.subcategory,
      })),
      techNames: targets.map((m) => m.techName),
    };
  }

  // Apply a prepared set of organize inputs. `keepActive` re-projects them so their
  // loaded state is preserved (used by both the auto-filer and the manual button). `auto`: the
  // auto-filer, which says nothing when another write is running (the next scan retries).
  async function applyLoose(
    inputs: ModInput[],
    techNames: string[],
    keepActive: boolean,
    auto = false,
  ) {
    if (inputs.length === 0) return;
    const op = beginWrite(`Filing ${modsCount(inputs.length)} into the library`, false, {
      silent: auto,
    });
    if (!op) return;
    /** What went wrong: the thrown error (keeps its code), or the report's problems. */
    let problem: unknown = null;
    try {
      const rep = await applyOrganize(inputs);
      if (rep.errors.length) {
        problem = rep.errors.slice(0, 3).join("; ");
      } else if (keepActive) {
        const next = new Set([...activeSet, ...techNames]);
        activeSet = next;
        await applyActive([...next]); // reports its own problems
      }
    } catch (e) {
      problem = e;
    } finally {
      op.end();
    }
    await runScan(false);
    if (problem !== null) {
      // Try again files whichever of these mods are still loose, as they are now.
      const wanted = new Set(techNames);
      const retry = () => {
        const fresh = looseInputs();
        return applyLoose(
          fresh.inputs.filter((i) => wanted.has(i.techName)),
          fresh.techNames.filter((t) => wanted.has(t)),
          keepActive,
        );
      };
      reportError(problem, "Filing mods into the library", { retry });
    }
  }

  // Auto-filer (opt-in): file new mods and keep them active — no prompt, they chose it.
  async function autoFile() {
    const { inputs, techNames } = looseInputs();
    await applyLoose(inputs, techNames, true, true);
  }

  // ── Manual "Organize N" button → dry-run preview → confirm ───────────────────
  // A read-only preview of exactly what will move, shown before any file is touched.
  // Dismissible for good ("Don't preview next time"), so power users aren't nagged.
  let organizePreview = $state<{
    inputs: ModInput[];
    techNames: string[];
    plan: PlannedMove[];
    loading: boolean;
    applying: boolean;
    /** Filing loose mods already in the library archive (#166), not organizing mods/. */
    adopt?: { path: string | null; note: string | null };
  } | null>(null);

  const PREVIEW_SKIP_KEY = "silo.organizePreviewSkip";

  async function organizeNew() {
    const { inputs, techNames } = looseInputs();
    if (inputs.length === 0) return;
    // Honor a remembered "don't preview" choice.
    if (localStorage.getItem(PREVIEW_SKIP_KEY) === "true") {
      await applyLoose(inputs, techNames, true);
      return;
    }
    organizePreview = { inputs, techNames, plan: [], loading: true, applying: false };
    try {
      const plan = await planOrganize(inputs);
      if (organizePreview) organizePreview = { ...organizePreview, plan, loading: false };
    } catch (e) {
      reportError(e, "Previewing what Organize moves");
      organizePreview = null;
    }
  }

  async function confirmOrganize(skipNext: boolean) {
    if (!organizePreview) return;
    if (organizePreview.adopt) {
      const fileNames = organizePreview.inputs.map((m) => m.fileName);
      const op = beginWrite(`Filing ${modsCount(fileNames.length)} into the library`);
      if (!op) return;
      organizePreview = { ...organizePreview, applying: true };
      let problem: unknown = null;
      try {
        const rep = await archiveAdoptApply(fileNames);
        if (rep.errors.length) problem = rep.errors.slice(0, 3).join("; ");
      } catch (e) {
        problem = e;
      } finally {
        op.end();
      }
      organizePreview = null;
      await runScan(false);
      if (problem !== null) reportError(problem, "Filing mods from the archive folder");
      return;
    }
    if (skipNext) localStorage.setItem(PREVIEW_SKIP_KEY, "true");
    const { inputs, techNames } = organizePreview;
    organizePreview = { ...organizePreview, applying: true };
    await applyLoose(inputs, techNames, true);
    organizePreview = null;
  }

  async function rebuildLibrary() {
    const op = beginWrite("Clearing the scan cache");
    if (!op) return;
    // The write is held through the rescan, so nothing changes the library until it has been
    // read again. The status line names it: it re-reads every mod, so it takes a while.
    try {
      try {
        await clearScanCache();
      } catch (e) {
        reportError(e, "Clearing the scan cache", { retry: rebuildLibrary });
        return;
      }
      op.setLabel("Rebuilding your library (re-scanning every mod)");
      await runScan(false, undefined, true);
    } finally {
      op.end();
    }
  }

  async function restoreVanilla() {
    if (
      !(await ask({
        title: "Put every mod back in your mods folder?",
        body: "Silo moves all parked and active mods out of the library folder and removes its links. Nothing is deleted.",
        confirm: "Undo organizing",
      }))
    )
      return;
    const op = beginWrite("Undoing organizing");
    if (!op) return;
    try {
      const rep = await flatten();
      if (rep.errors.length)
        reportError(rep.errors.slice(0, 3).join("; "), "Undoing organizing");
      if (rep.notices?.length) info(rep.notices.join(" "), { duration: 10_000 });
    } catch (e) {
      reportError(e, "Undoing organizing");
    } finally {
      op.end();
    }
    await runScan();
  }

  // Overrides applied as a display layer over the scanned category.
  const effectiveMods = $derived(
    mods.map((m) => {
      const o = overrideMap[m.techName];
      return o ? { ...m, category: o.category, subcategory: o.subcategory } : m;
    }),
  );

  function openEditor(techName: string, ev: MouseEvent) {
    ev.stopPropagation();
    editing = { techName, x: ev.clientX, y: ev.clientY };
  }

  // ── Library row menu: the ⋯ button, right-click and Shift+F10 open the same items ──
  let rowMenu = $state<{ mod: ModEntry; at: MenuTarget } | null>(null);
  let rowMenuOpen = $state(false);
  // Kept outside the reactive state: Menu calls onclose (which clears rowMenu) before onselect.
  let rowMenuRun: ((id: string) => void) | null = null;
  function openRowMenu(mod: ModEntry, at: MenuTarget) {
    rowMenuRun = rowMenuFor(mod, at).run;
    rowMenu = { mod, at };
    rowMenuOpen = true;
  }
  // Forget the menu once it has closed, after its close handling (focus return reads the anchor).
  $effect(() => {
    if (!rowMenuOpen) untrack(() => (rowMenu = null));
  });
  // Seeds ModBrowser's catalog search when "Find in Browse" is used. ModBrowser remounts
  // on every view switch, so setting this before switching views seeds the fresh mount.
  let browseSeed = $state<string | null>(null);
  let browseSeedTech = $state<string | null>(null);

  function openInBrowse(mod: ModEntry) {
    browseSeed = mod.title ?? mod.techName;
    browseSeedTech = mod.techName; // so Browse can open this exact mod's drawer, not just filter to it
    go("browse");
  }

  // The reverse of openInBrowse: jump from a Browse card to the installed mod in the Library,
  // filtering to it and opening its drawer.
  function openInLibrary(techName: string) {
    const mod = mods.find((m) => m.techName === techName);
    if (!mod) return;
    query = mod.title ?? mod.techName; // seed the library search so the tile surfaces behind the drawer
    go("library");
    detailMod = mod; // open this mod's drawer
  }

  /**
   * The row menu for one mod: its items, and what each id does.
   * @param mod the row's mod
   * @param at where the menu opened (the category menu opens there too)
   */
  function rowMenuFor(mod: ModEntry, at: MenuTarget): { items: MenuItem[]; run: (id: string) => void } {
    const c = cur(mod.techName);
    const isActive = activeSet.has(mod.techName);
    const hasSettings = settingsModsSet.has(mod.techName);
    const items: MenuItem[] = [
      { id: "active", label: isActive ? "Park (deactivate)" : "Activate", icon: isActive ? "circle" : "dot" },
      { id: "details", label: "Open details", icon: "info" },
      { id: "browse", label: "Find in Browse", icon: "search" },
      { id: "category", label: "Change category…", icon: "tag", separatorBefore: true },
    ];
    if (hasSettings) items.push({ id: "settings", label: "Edit settings…", icon: "modSettings" });
    items.push(
      { id: "favorite", label: c.favorite ? "Remove favorite" : "Favorite", icon: "favorite", separatorBefore: true },
      { id: "hidden", label: c.hidden ? "Unhide" : "Hide", icon: c.hidden ? "visible" : "hidden" },
      { id: "broken", label: c.broken ? "Remove flag" : "Flag", icon: "flagged", danger: !c.broken },
    );
    const run = (id: string) => {
      switch (id) {
        case "active":
          return void toggleActive(mod.techName);
        case "details":
          detailMod = mod;
          return;
        case "browse":
          return openInBrowse(mod);
        case "category": {
          const r = at.anchor?.getBoundingClientRect();
          const x = at.point?.x ?? r?.left ?? 0;
          const y = at.point?.y ?? r?.bottom ?? 0;
          editing = { techName: mod.techName, x, y };
          return;
        }
        case "settings":
          settingsMod = { techName: mod.techName, title: mod.title ?? mod.techName };
          return;
        case "favorite":
        case "hidden":
        case "broken":
          return void toggleCuration(mod.techName, id);
      }
    };
    return { items, run };
  }
  const rowMenuSpec = $derived(rowMenu ? rowMenuFor(rowMenu.mod, rowMenu.at) : null);

  async function setCategory(
    techName: string,
    category: string,
    subcategory: string | null = null,
  ) {
    overrideMap = {
      ...overrideMap,
      [techName]: { category, subcategory },
    };
    editing = null;
    try {
      await setOverride({ techName, category, subcategory });
    } catch (e) {
      reportError(e, "Saving the category");
    }
  }

  async function resetCategory(techName: string) {
    const next = { ...overrideMap };
    delete next[techName];
    overrideMap = next;
    editing = null;
    try {
      await setOverride({ techName, category: "", subcategory: null });
    } catch (e) {
      reportError(e, "Resetting the category");
    }
  }

  function cur(techName: string): CurationRow {
    return (
      curationMap[techName] ?? {
        techName,
        favorite: false,
        hidden: false,
        broken: false,
        rating: 0,
        note: null,
      }
    );
  }

  // Tags: techName -> string[]
  let tagMap = $state<Record<string, string[]>>({});
  let detailMod = $state<ModEntry | null>(null);
  let selectedTag = $state<string | null>(null);

  const allTags = $derived.by(() => {
    const s = new Set<string>();
    for (const arr of Object.values(tagMap)) for (const t of arr) s.add(t);
    return [...s].sort((a, b) => a.localeCompare(b));
  });

  function tagsOf(techName: string): string[] {
    return tagMap[techName] ?? [];
  }
  async function loadTags() {
    try {
      const rows = await getTags();
      const m: Record<string, string[]> = {};
      for (const r of rows) (m[r.techName] ??= []).push(r.tag);
      tagMap = m;
    } catch (e) {
      reportError(e, "Loading your tags", { background: true });
    }
  }

  // GitHub repo links: techName -> {owner, repo}
  let repoMap = $state<Record<string, { owner: string; repo: string }>>({});
  async function loadRepos() {
    try {
      const rows = await getModRepos();
      repoMap = Object.fromEntries(rows.map((r) => [r.techName, { owner: r.owner, repo: r.repo }]));
    } catch (e) {
      reportError(e, "Loading linked GitHub repos", { background: true });
    }
  }

  // Source pages (where each mod came from, #196): techName -> saved page. Silo never requests
  // these; Open page hands them to the browser.
  let sourceMap = $state<Record<string, SourceRow>>({});
  async function loadSources() {
    try {
      const rows = await getModSources();
      sourceMap = Object.fromEntries(rows.map((r) => [r.techName, r]));
    } catch (e) {
      reportError(e, "Loading saved source pages", { background: true });
    }
  }
  function setSource(techName: string, row: SourceRow | null) {
    const next = { ...sourceMap };
    if (row) next[techName] = row;
    else delete next[techName];
    sourceMap = next;
  }
  async function markChecked(row: CheckYourselfRow) {
    try {
      const at = await markSourceChecked(row.techName);
      const s = sourceMap[row.techName];
      if (s) setSource(row.techName, { ...s, lastChecked: at });
    } catch (e) {
      reportError(e, "Saving the check date");
    }
  }

  let updateChecking = $state(false);
  let updateResults = $state<UpdateRow[]>([]);
  // Mods with a saved page that this check couldn't answer: the user checks those by hand.
  const checkYourself = $derived(
    updateChecking ? [] : checkYourselfList(mods, sourceMap, updateResults),
  );
  const linkedCount = $derived(Object.keys(repoMap).length);

  // "Needs update" is on-demand: the first time it's switched on it runs a quiet update check,
  // then filters to whatever came back (or reports that everything's current). No launch-time work.
  async function toggleNeedsUpdate() {
    if (needsUpdateOnly) {
      needsUpdateOnly = false;
      return;
    }
    if (updateResults.length === 0 && !updateChecking) await checkAllUpdates(true);
    if (availableUpdates.length > 0) {
      needsUpdateOnly = true;
    } else {
      success("All mods are up to date");
    }
  }

  async function checkAllUpdates(quiet = false) {
    if (!quiet) go("updates");
    updateChecking = true;
    updateResults = [];
    const byTech = new Map(mods.map((m) => [m.techName, m]));
    const rows = new Map<string, UpdateRow>();

    // 1. One catalog request covers the whole library (GitHub + ModHub mods alike).
    try {
      const catalog = await catalogCheckUpdates(
        mods.map((m) => ({ techName: m.techName, version: m.version ?? undefined })),
      );
      for (const c of catalog) {
        const m = byTech.get(c.techName);
        if (!m) continue;
        rows.set(c.techName, {
          techName: c.techName,
          title: m.title ?? c.techName,
          path: m.path,
          current: m.version ?? undefined,
          latest: c.latest ?? undefined,
          hasUpdate: c.hasUpdate,
          assetUrl: c.downloadUrl,
          source: c.source ?? undefined,
        });
      }
      updateResults = [...rows.values()];
    } catch {
      // Catalog unreachable (e.g. endpoint not yet deployed) — fall back to GitHub only.
    }

    // 2. Linked GitHub repos are authoritative for the user's chosen source — check
    //    them directly and let them override the catalog row for that mod.
    for (const [techName, r] of Object.entries(repoMap)) {
      const m = byTech.get(techName);
      if (!m) continue;
      const row: UpdateRow =
        rows.get(techName) ?? { techName, title: m.title ?? techName, path: m.path };
      try {
        const info = await checkModUpdate(r.owner, r.repo, m.version ?? "0");
        row.current = info.current;
        row.latest = info.release.tag;
        row.hasUpdate = info.hasUpdate;
        row.assetUrl = info.release.assetUrl;
        row.source = "github";
        row.error = undefined;
      } catch (e) {
        if (!rows.has(techName)) row.error = inlineError(e, "Checking for an update");
      }
      rows.set(techName, row);
      updateResults = [...rows.values()];
    }

    updateChecking = false;
  }

  async function installFromRow(row: UpdateRow) {
    if (!row.assetUrl) return;
    const op = beginWrite(`Installing ${row.title}`);
    if (!op) return;
    /** What went wrong: the thrown error (keeps its code), or the install's problems. */
    let warning: unknown = null;
    let failed = false;
    try {
      const out = await downloadUpdate(row.path, row.assetUrl);
      if (out.backup) {
        success(`Updated ${row.title}. The previous version is kept in ${out.backup}.`, {
          duration: 10_000,
        });
      }
      if (out.problems.length) warning = out.problems.join(" ");
      updateResults = updateResults.map((r) =>
        r.techName === row.techName ? { ...r, hasUpdate: false, current: r.latest } : r,
      );
    } catch (e) {
      warning = e;
      failed = true;
    } finally {
      op.end();
    }
    await runScan(false);
    if (warning !== null)
      reportError(warning, `Installing ${row.title}`, failed ? { retry: () => installFromRow(row) } : {});
  }

  const availableUpdates = $derived(updateResults.filter((r) => r.hasUpdate));
  const updatesSet = $derived(new Set(availableUpdates.map((r) => r.techName)));

  async function toggleCuration(
    techName: string,
    flag: "favorite" | "hidden" | "broken",
  ) {
    const c = cur(techName);
    const next: CurationRow = { ...c, [flag]: !c[flag] };
    curationMap = { ...curationMap, [techName]: next };
    try {
      await setCuration(next);
    } catch (e) {
      reportError(e, "Saving your mod flags");
    }
  }

  const q = $derived(query.trim().toLowerCase());
  // Library category tree for the FilterBar's parent/child dropdowns, built from the scan
  // (ModEntry.category + subcategory). Local counts, sorted A–Z. Shape matches FilterBar's CatNode.
  const libCatTree = $derived.by(() => {
    const map = new Map<string, { count: number; children: Map<string, number> }>();
    for (const m of effectiveMods) {
      let node = map.get(m.category);
      if (!node) {
        node = { count: 0, children: new Map() };
        map.set(m.category, node);
      }
      node.count++;
      if (m.subcategory)
        node.children.set(m.subcategory, (node.children.get(m.subcategory) ?? 0) + 1);
    }
    return [...map.entries()]
      .sort((a, b) => a[0].localeCompare(b[0]))
      .map(([name, n]) => ({
        name,
        count: n.count,
        children: [...n.children.entries()]
          .sort((a, b) => a[0].localeCompare(b[0]))
          .map(([cname, ccount]) => ({ name: cname, count: ccount, value: cname })),
      }));
  });
  const libFilterCount = $derived(
    (libCat ? 1 : 0) +
      (favoritesOnly ? 1 : 0) +
      (showHidden ? 1 : 0) +
      (flaggedOnly ? 1 : 0) +
      (conflictedOnly ? 1 : 0) +
      (hasSettingsOnly ? 1 : 0) +
      (needsUpdateOnly ? 1 : 0),
  );
  function clearLibFilters() {
    libCat = "";
    libSub = "";
    favoritesOnly = false;
    showHidden = false;
    flaggedOnly = false;
    conflictedOnly = false;
    hasSettingsOnly = false;
    needsUpdateOnly = false;
    selectedTag = null;
  }
  const filtered = $derived.by(() => {
    let list = effectiveMods;
    if (libCat) {
      list = list.filter(
        (m) => m.category === libCat && (!libSub || m.subcategory === libSub),
      );
    }
    if (!showHidden) {
      list = list.filter((m) => !cur(m.techName).hidden);
    }
    if (favoritesOnly) {
      list = list.filter((m) => cur(m.techName).favorite);
    }
    if (flaggedOnly) {
      list = list.filter((m) => cur(m.techName).broken);
    }
    if (conflictedOnly) {
      list = list.filter(
        (m) => conflictedSet.has(m.techName) || conflictedSet.has(m.title ?? ""),
      );
    }
    if (hasSettingsOnly) {
      list = list.filter((m) => m.hasSettings);
    }
    if (needsUpdateOnly) {
      list = list.filter((m) => updatesSet.has(m.techName));
    }
    if (statFilter === "maps") {
      list = list.filter((m) => m.isMap);
    } else if (statFilter === "scripts") {
      list = list.filter((m) => m.scriptCount > 0);
    } else if (statFilter === "unique") {
      list = list.filter((m) => m.uniqueType != null);
    } else if (statFilter === "active") {
      list = list.filter((m) => activeSet.has(m.techName));
    }
    if (selectedTag) {
      list = list.filter((m) => tagsOf(m.techName).includes(selectedTag!));
    }
    if (q !== "") {
      list = list.filter(
        (m) =>
          (m.title ?? "").toLowerCase().includes(q) ||
          m.techName.toLowerCase().includes(q) ||
          (m.author ?? "").toLowerCase().includes(q),
      );
    }
    return list;
  });

  type SortKey = "name" | "category" | "size" | "added" | "version" | "rating";
  let sortBy = $state<SortKey>("added");
  let sortDir = $state<"asc" | "desc">("desc");
  // Library view: the list (default, for every library size) or cards; remembered per machine.
  let libView = $state<LibraryViewMode>(loadLibraryView());
  $effect(() => saveLibraryView(libView));
  // Row density: comfortable (56px) by default; compact (44px) awaits a Settings toggle.
  const libDensity: Density = "comfortable";
  // The rendered list or grid, so the drawer can hand focus back to the row it was opened from.
  let libList = $state<{ focusKey: (key: string) => boolean }>();
  const modKey = (m: ModEntry) => m.path;
  // Selection lands in Task 2 of #242; the rows and keys already report the gestures.
  function selectMod(_key: string, _intent: SelectIntent) {}
  // A row's badges: problems, then status, then info (the row caps them at three).
  function badgesFor(mod: ModEntry) {
    const c = cur(mod.techName);
    return rowBadges(mod, {
      verdict: getVerdict(mod)?.status,
      hasUpdate: updatesSet.has(mod.techName),
      hasSettings: settingsModsSet.has(mod.techName),
      conflicted: conflictedSet.has(mod.techName),
      flagged: c.broken,
      hidden: c.hidden,
    });
  }

  const visible = $derived.by(() => {
    const arr = [...filtered];
    const mul = sortDir === "desc" ? -1 : 1;
    const name = (m: ModEntry) => (m.title ?? m.techName).toLowerCase();
    arr.sort((a, b) => {
      let r = 0;
      switch (sortBy) {
        case "name":
          r = name(a).localeCompare(name(b));
          break;
        case "size":
          r = a.size - b.size;
          break;
        case "added":
          r = a.mtimeMs - b.mtimeMs;
          break;
        case "version":
          r = (a.version ?? "").localeCompare(b.version ?? "", undefined, { numeric: true });
          break;
        case "rating":
          // My own star rating (0 = unrated sorts last on desc, which is the default use).
          r = cur(a.techName).rating - cur(b.techName).rating;
          break;
        case "category":
          r =
            a.category.localeCompare(b.category) ||
            (a.subcategory ?? "").localeCompare(b.subcategory ?? "") ||
            name(a).localeCompare(name(b));
          break;
      }
      return r * mul || name(a).localeCompare(name(b));
    });
    return arr;
  });

  // Select-all checkbox state over the filtered view.
  const filteredActiveCount = $derived(
    filtered.reduce((n, m) => n + (activeSet.has(m.techName) ? 1 : 0), 0),
  );
  const allFilteredActive = $derived(
    filtered.length > 0 && filteredActiveCount === filtered.length,
  );

  // Bulk activate/deactivate the currently-filtered set (fast loadout building).
  async function setActiveForFiltered(active: boolean) {
    // How many mods this actually changes (already-correct ones are free).
    const changing = filtered.filter((m) =>
      active ? !activeSet.has(m.techName) : activeSet.has(m.techName),
    );
    // Toggling the WHOLE library at once moves a lot of files — and on a cloud-synced folder each
    // one is a copy or delete, not a cheap link. Confirm before a big unfiltered sweep so it's not
    // an accidental gigabytes-long operation.
    if (libFilterCount === 0 && changing.length > 25) {
      const verb = active ? "Activate" : "Deactivate";
      const tail = cloudSynced
        ? " Your mods folder is cloud-synced, so each one is copied/removed individually. This can take a while."
        : "";
      if (
        !(await ask({
          title: `${verb} all ${changing.length} mods in your library?`,
          body:
            `Each one is ${active ? "linked into" : "removed from"} your mods folder; nothing is deleted from your library.${tail} ` +
            `To undo it, ${active ? "deactivate" : "activate"} them again or switch to a loadout.`,
          confirm: `${verb} ${changing.length} mods`,
        }))
      )
        return;
    }
    const op = beginWrite(active ? "Activating mods" : "Deactivating mods");
    if (!op) return;
    const next = new Set(activeSet);
    for (const m of filtered) {
      if (active) next.add(m.techName);
      else next.delete(m.techName);
    }
    activeSet = next;
    let problem: unknown = null;
    try {
      await applyActive([...next]); // reports its own problems
    } catch (e) {
      problem = e;
    } finally {
      op.end();
    }
    // Reconcile the optimistic set against what's actually on disk — a partial/failed projection
    // (e.g. a file Silo couldn't prove was its own) leaves the two out of sync otherwise.
    await runScan(false);
    if (problem !== null) reportError(problem, active ? "Activating mods" : "Deactivating mods");
  }

  // Repair copy-projections into hardlinks: reclaims the duplicated space and makes future
  // activate/deactivate O(1) instead of a full-library content hash. Progress-reported so a
  // large one-time pass shows a bar instead of looking frozen.
  async function optimizeLinks() {
    const op = beginWrite("Reclaiming disk space", true);
    if (!op) return;
    let unlisten: (() => void) | null = null;
    let done = "";
    /** `done` explains why nothing changed (an info toast), not a result (a success toast). */
    let explained = false;
    let problem: unknown = null;
    try {
      unlisten = await onRelinkProgress((d, total) => op.progress(d, total));
      const rep = await relinkProjections();
      if (rep.crossDriveCopies) {
        explained = true;
        done =
          "Your library archive is on another drive and Windows can't link across drives without Developer Mode. Active mods stay as copies.";
      } else if (rep.hardlinksUnsupported) {
        explained = true;
        cloudSynced = true;
        done = "Your mods folder is cloud-synced. Hardlinks aren't available, so there's no space to reclaim.";
      } else {
        if (rep.errors.length) problem = rep.errors.slice(0, 3).join("; ");
        const gb = rep.reclaimedBytes / 1024 ** 3;
        const freed =
          gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(rep.reclaimedBytes / 1024 ** 2)} MB`;
        done =
          rep.upgraded > 0
            ? `Reclaimed ${freed} from ${rep.upgraded} mod${rep.upgraded === 1 ? "" : "s"}.`
            : "Links are already optimized. Nothing to reclaim.";
      }
    } catch (e) {
      problem = e;
    } finally {
      unlisten?.();
      op.end();
    }
    await runScan(false); // reconcile + refresh sizes
    if (problem !== null) reportError(problem, "Reclaiming disk space");
    // Long enough to read: the cross-drive explanation is a full sentence.
    const duration = Math.max(6000, done.length * 50);
    if (done && explained) info(done, { duration });
    else if (done) success(done, { duration });
  }

  const stats = $derived.by(() => {
    let maps = 0,
      scripts = 0,
      unique = 0,
      issues = 0;
    for (const m of mods) {
      if (m.isMap) maps++;
      if (m.scriptCount > 0) scripts++;
      if (m.uniqueType) unique++;
      if (m.error || m.ignoredDigitPrefix) issues++;
    }
    return { maps, scripts, unique, issues };
  });

  const pct = $derived(
    progress.total > 0 ? Math.round((progress.done / progress.total) * 100) : 0,
  );

  /** Why switching folders isn't safe right now, or null. */
  function switchBlockReason(): Promise<string | null> {
    return switchGate(
      {
        moveUnfinished: archiveMove.unfinished,
        scanning,
        busy: writeLock.blocked ? (currentOp()?.label ?? null) : null,
        blocked: !!modsStatus?.layoutProblem,
      },
      async () => !!(await bisectSnapshotGet().catch(() => null)),
    );
  }

  /** Adopt a new status; if the live folder changed, reload everything folder-specific. */
  async function applyModsStatus(st: ModsRootStatus) {
    const prev = roots[0] ?? null;
    modsStatus = st;
    await archiveMove.refresh(); // the archive belongs to the mods folder
    roots = rootsFor(st);
    if ((roots[0] ?? null) === prev) return;
    result = null;
    mods = [];
    libraryScanned = false;
    scanFailed = false;
    activeSet = new Set();
    foreignFiles = [];
    if (roots.length) {
      await runScan();
      hardlinkSupport()
        .then((ok) => (cloudSynced = !ok))
        .catch(() => {});
    }
    await loadSavegames();
  }

  async function switchModsFolder(path: string) {
    const why = await switchBlockReason();
    if (why) {
      refuse(why);
      return;
    }
    try {
      if (
        (await folderLooksEmpty(path)) &&
        !(await ask({
          title: "Use an empty folder as your mods folder?",
          body: "This folder has no mods yet, so your library will look empty here. Nothing is moved; switch back to your old folder any time in Settings.",
          facts: [{ label: "Folder", value: path }],
          confirm: "Use this folder",
        }))
      )
        return;
      const st = await setModsRoot(path);
      // Switch first: if updating FS25's setting then fails, Silo is still on the chosen folder
      // and the mismatch banner explains (the error shows too).
      await applyModsStatus(st);
      if (
        st.mismatch &&
        st.canWriteGameSetting &&
        (await ask({
          title: "Point Farming Simulator at this folder too?",
          body:
            "Silo updates FS25's gameSettings.xml and keeps a backup of it, so you can switch back later. " +
            "Close FS25 first. The game rewrites that file when it exits.",
          facts: [
            { label: "FS25 reads now", value: st.gameReads ?? "(unknown)" },
            { label: "Silo uses", value: path },
          ],
          confirm: "Update FS25's setting",
          cancel: "Only in Silo",
        }))
      ) {
        await pointGameHere();
      }
    } catch (e) {
      reportError(e, "Switching mods folders");
    }
  }

  async function changeModsFolder() {
    const why = await switchBlockReason();
    if (why) {
      refuse(why);
      return;
    }
    const path = await pickFolderFor("modsFolder");
    if (path) await switchModsFolder(path);
  }

  async function useGameFolder() {
    const why = await switchBlockReason();
    if (why) {
      refuse(why);
      return;
    }
    try {
      await applyModsStatus(await clearModsRoot());
    } catch (e) {
      reportError(e, "Switching to the game's mods folder");
    }
  }

  async function pointGameHere() {
    try {
      await applyModsStatus(await pointGameAtModsRoot());
    } catch (e) {
      reportError(e, "Updating FS25's mods folder setting");
    }
  }

  async function retryModsFolder() {
    try {
      await applyModsStatus(await modsRootStatus());
    } catch (e) {
      reportError(e, "Checking the mods folder", { retry: retryModsFolder });
    }
  }

  async function makeModsFolder() {
    try {
      await applyModsStatus(await createModsFolder());
    } catch (e) {
      reportError(e, "Creating the mods folder");
    }
  }

  // ── Library archive location (#166) ──────────────────────────────────────────
  // The move's state, poll and drive/settle cycle live in archiveMove.svelte.ts; App supplies
  // the side effects that reach beyond the move.
  const archiveMove = new ArchiveMove({
    setError: (msg) => reportError(msg, "Moving your library"),
    refreshModsStatus: async () => {
      modsStatus = await modsRootStatus().catch(() => modsStatus);
    },
    runScan: () => runScan(),
    // One adopt offer at a time (a second settle pass mustn't replace an open preview).
    offerAdopt: async () => {
      if (!organizePreview) await offerAdopt();
    },
  });

  // A running move holds the library-write class, so other writes refuse and say why. It ends
  // as soon as the move stops running (finished or paused), before the move's settle rescans.
  // It's a background op: navigation, Settings, read-only panels and scans stay usable while it
  // runs (fgLock leaves it out). The move's own notice shows its progress, so it's quiet too
  // (no status line, no activity entry).
  let moveOp: OpToken | null = null;
  $effect(() => {
    const isRunning = archiveMove.running;
    untrack(() => {
      if (isRunning && !moveOp) {
        moveOp = start("Moving your library", {
          exclusive: "library-write",
          progress: true,
          background: true,
          quiet: true,
        });
      } else if (!isRunning && moveOp) {
        moveOp.end();
        moveOp = null;
      }
    });
  });
  $effect(() => {
    const p = archiveMove.progress;
    if (p && p.total > 0) untrack(() => moveOp?.progress(p.done, p.total));
  });
  $effect(() => () => moveOp?.end());

  // The archive's notice: an unfinished move (with Pause / Resume / Cancel), else an archive
  // whose folder isn't available.
  $effect(() => {
    const n = archiveNotice(
      {
        archive: archiveMove.archive,
        progress: archiveMove.progress,
        running: archiveMove.running,
        unfinished: archiveMove.unfinished,
        cancelling: archiveMove.cancelling,
        pauseRequested: archiveMove.pauseRequested,
      },
      {
        pause: () => archiveMove.pause(),
        resume: () => archiveMove.resume(),
        cancel: () => archiveMove.cancel(),
        retry: async () => {
          await archiveMove.refresh();
          await runScan();
        },
        chooseAnother: changeArchive,
      },
    );
    syncNotice(ARCHIVE_MOVE_KEY, n?.key === ARCHIVE_MOVE_KEY ? n : null);
    syncNotice(ARCHIVE_UNAVAILABLE_KEY, n?.key === ARCHIVE_UNAVAILABLE_KEY ? n : null);
  });

  // The startup read gave up: try once more whenever Settings opens.
  $effect(() => {
    if (nav.view === "settings" && archiveMove.loadFailed) void archiveMove.refresh();
  });

  /** Point Silo at another archive folder (`path`, or null = the default) without moving
   *  anything, then rescan so the library reconciles against it. `why`: the current archive is
   *  missing, or it holds no mods and a move to `path` was refused (`refusal`: the first reason). */
  async function repointArchive(path: string | null, why: "missing" | "empty", refusal = "") {
    const old = archiveMove.archive?.path ?? "its old folder";
    const target = path ?? "The default archive inside your mods folder";
    const ok = await ask(
      why === "empty"
        ? {
            title:
              path === null
                ? "Use the default library archive?"
                : `Use ${target} as your library archive?`,
            body:
              `Silo can't move your library there${refusal ? ` (${unDot(refusal)})` : ""}, but your current archive is empty, ` +
              "so Silo can use this folder as your library instead. Nothing is moved; you can choose another archive folder later in Settings.",
            facts: [
              { label: "Current archive", value: old },
              { label: "New archive", value: target },
            ],
            confirm: "Use this folder",
          }
        : {
            title:
              path === null
                ? "Start fresh with the default library archive?"
                : `Use ${target} as your library archive?`,
            body:
              path === null
                ? "Your library archive isn't available. Nothing is moved or copied, and mods still on the old drive stay there, untouched. If you restored your archive somewhere, cancel, use Choose another and pick that folder instead."
                : "Your library archive isn't available. Nothing is moved or copied. Use this if you moved or restored your archive there yourself. Mods still on the old drive stay there, untouched, and you can point Silo back at it later in Settings.",
            facts: [
              { label: "Missing archive", value: old },
              { label: "New archive", value: target },
            ],
            confirm: path === null ? "Use default" : "Use this folder",
          },
    );
    if (!ok) return;
    try {
      const op = beginWrite("Pointing Silo at the archive");
      if (!op) return;
      try {
        await archiveRepoint(path);
      } finally {
        op.end();
      }
      await archiveMove.refresh();
      modsStatus = await modsRootStatus().catch(() => modsStatus);
      await runScan();
    } catch (e) {
      reportError(e, "Pointing Silo at the archive");
    }
  }

  async function changeArchive() {
    const why = await switchBlockReason();
    if (why) {
      refuse(why);
      return;
    }
    const path = await pickFolderFor("archive");
    if (!path) return;
    if (archiveMove.missing) {
      await repointArchive(path, "missing");
      return;
    }
    try {
      const check = start("Checking the folder"); // a read: it changes nothing
      let pf: ArchivePreflight;
      try {
        pf = await archiveMovePreflight(path);
      } finally {
        check.end();
      }
      if (pf.problems.length) {
        // Nothing to move and the folder clears every path check: offer to use it as the
        // library as it is.
        if (pf.canRepoint) await repointArchive(path, "empty", pf.problems[0]);
        else
          toast({
            message: "Silo can't move your library there.",
            body: problemLines(pf.problems),
            tone: "warn",
            sticky: true,
          });
        return;
      }
      let body =
        "Active mods stay playable. Silo links them into your mods folder" +
        (pf.sameDrive ? "." : " (or copies them if links aren't available).");
      if (!pf.sameDrive && !pf.symlinksAvailable)
        body += " Turn on Windows Developer Mode to link active mods instead of copying them.";
      if (pf.looseMods > 0)
        body += ` That folder already holds ${pf.looseMods === 1 ? "1 loose mod" : `${pf.looseMods} loose mods`}. Silo will offer to file ${pf.looseMods === 1 ? "it" : "them"} in afterwards.`;
      body +=
        " Close Farming Simulator 25 first. You can pause the move to play and resume later, cancel it to put moved mods back, or move the library back afterwards.";
      const facts = [
        { label: "From", value: archiveMove.archive?.path ?? "(unknown)" },
        { label: "To", value: path },
        { label: "Mods", value: String(pf.modCount) },
        { label: "Size", value: fmtSize(pf.bytes) },
      ];
      if (!pf.sameDrive && pf.freeBytes != null)
        facts.push({ label: "Free on that drive", value: fmtSize(pf.freeBytes) });
      if (
        !(await ask({
          title: `Move your library to ${path}?`,
          body,
          facts,
          confirm: "Move library",
        }))
      )
        return;
      archiveMove.startTracking();
      await archiveMove.drive(() => archiveMoveStart(path));
    } catch (e) {
      reportError(e, "Moving your library");
    }
  }

  async function archiveDefault() {
    const why = await switchBlockReason();
    if (why) {
      refuse(why);
      return;
    }
    if (archiveMove.missing) {
      await repointArchive(null, "missing");
      return;
    }
    if (
      !(await ask({
        title: "Move your library back into your mods folder?",
        body: "Mods are moved, not copied, and active mods stay playable. Close Farming Simulator 25 first. You can pause or cancel the move, and move it out again later.",
        facts: [
          { label: "From", value: archiveMove.archive?.path ?? "(unknown)" },
          { label: "To", value: roots[0] ? `Your mods folder (${roots[0]})` : "Your mods folder" },
        ],
        confirm: "Move library",
      }))
    )
      return;
    archiveMove.startTracking();
    await archiveMove.drive(() => archiveUseDefault());
  }

  /** After a move, if the archive folder holds loose mods, offer to file them in. */
  async function offerAdopt() {
    const pv = await archiveAdoptPreview().catch(() => null);
    if (!pv) return;
    const n = pv.duplicates.length;
    const dupNote = n
      ? `${modsCount(n)} in that folder ${n === 1 ? "is" : "are"} already in your library and ${n === 1 ? "was" : "were"} left untouched: ${capList(pv.duplicates)}.`
      : null;
    if (pv.inputs.length === 0) {
      if (dupNote) info(dupNote, { duration: 10_000 });
      return;
    }
    organizePreview = {
      inputs: pv.inputs,
      techNames: pv.inputs.map((m) => m.techName),
      plan: pv.plan,
      loading: false,
      applying: false,
      adopt: { path: archiveMove.archive?.path ?? null, note: dupNote },
    };
  }

  function dismissOverrideNotice() {
    overrideNoticeOpen = false;
    localStorage.setItem("silo.overrideNoticeShown", "1");
  }

  // ── Notices: each condition raises and clears its own key (a rescan touches only `scan`) ──

  // The mods folder: unavailable / blocked / not created yet, FS25 reading another folder, and
  // the one-time "Silo now follows FS25's folder".
  $effect(() => {
    const n = modsFolderNotices(modsStatus, overrideNoticeOpen, {
      retryBlocked: fgLock.reason,
      retry: retryModsFolder,
      chooseAnother: changeModsFolder,
      useGameFolder,
      pointGameHere,
      createFolder: makeModsFolder,
      dismissFollowed: dismissOverrideNotice,
    });
    syncNotice(MODS_FOLDER_KEY, n.modsFolder);
    syncNotice(OVERRIDE_KEY, n.override);
    syncNotice(OVERRIDE_FOLLOWED_KEY, n.followed);
  });

  // A cloud-synced mods folder (copies instead of hardlinks). Dismissing it is remembered.
  $effect(() =>
    syncNotice(
      "cloud",
      cloudNoticeOpen
        ? {
            level: "warning",
            icon: "cloud",
            title:
              "Your mods folder is synced by a cloud service (OneDrive, Google Drive, or Proton Drive), which doesn't support hardlinks.",
            body:
              "Silo projects your active mods as file copies instead. That works, but uses more disk and makes bulk activate and deactivate slower. For the best performance, exclude …\\My Games\\FarmingSimulator2025 from your cloud sync.",
            dismissible: true,
            onDismiss: dismissCloudNotice,
          }
        : null,
    ),
  );

  // An interrupted guided bisection: the active set may be partly applied.
  $effect(() => {
    const lock = writeLock;
    const off = { disabled: lock.blocked, title: lock.reason || undefined };
    syncNotice(
      "bisect-recovery",
      bisectRecovery
        ? {
            level: "blocking",
            title: "A guided diagnosis was interrupted.",
            body: "Your active mods may be partially applied.",
            actions: [
              { label: "Restore my mod set", run: restoreBisect, primary: true, ...off },
              { label: "Discard", run: discardBisect, ...off },
              { label: "Later", run: () => void (bisectRecovery = null), ...off },
            ],
            dismissible: false,
          }
        : null,
    );
  });

  /** Scans skipped behind a foreground write collapse into one rescan when it ends. */
  const scanGate = rescanGate((auto) => void runScan(auto));

  /**
   * Rescan the library. While a foreground library write runs, one rescan is queued for when it
   * ends instead (writes rescan once they're done; the queued one covers startup, folder
   * switches and the like). A background op (the archive move) never holds a scan back. A scan
   * doesn't hold the write class itself, so the controls don't flicker off on every focus
   * rescan; the ones that must wait for a scan check `scanning`.
   * @param auto allow auto-filing afterwards
   * @param label names the scan in the status line (a plain rescan only shows the progress bar)
   * @param heldByCaller the caller holds the write class for this scan (rebuild, bisect
   *   restore): don't wait for it
   */
  async function runScan(auto = true, label?: string, heldByCaller = false) {
    if (scanning) return;
    if (!scanGate.admit(auto, heldByCaller)) return; // queued until the write ends
    const op = start(label ?? "Rescanning your library", { quiet: !label });
    scanOp = op;
    scanning = true;
    progress = { done: 0, total: 0 };
    try {
      const r = await scanMods(roots.length ? roots : undefined);
      result = r;
      mods = r.mods;
      libraryScanned = true;
      scanFailed = false;
      activeSet = new Set(r.mods.filter((m) => m.active).map((m) => m.techName));
      // Interrupted mod updates Silo finished or rolled back before scanning. The scan owns
      // only the `scan` notice: other producers' notices stay up across rescans.
      if (r.notices?.length) info(r.notices.join(" "), { duration: 10_000 });
      syncNotice(
        SCAN_KEY,
        r.problems?.length
          ? {
              level: "warning",
              title: "Silo couldn't finish an interrupted mod update.",
              body: r.problems.join(" "),
              dismissible: true,
            }
          : null,
      );
      // Filesystem-level check (Rust): files at a managed name that aren't Silo's projection.
      detectForeignFiles(roots[0])
        .then((f) => (foreignFiles = f))
        .catch(() => (foreignFiles = []));
      // Privacy: a scan never sends the library to the catalog. Catalog calls happen only
      // when the user asks (Browse, a mod's details, Updates, integrity check).
      // NOTE: the update check is NOT auto-run on launch — on a large library the burst of
      // scan + catalog-tags + a whole-library update pass all landing at once could jank the
      // window right after it opened. The "Needs update" toggle runs it on demand instead.
      // Re-read savegames too, so a Rescan picks up a save created since launch (they're
      // otherwise only loaded once on startup). Fire-and-forget; it handles its own errors.
      void loadSavegames();
    } catch (e) {
      // The library wasn't read: a standing condition until a scan works, so a notice.
      scanFailed = true;
      reportError(e, "Scanning your library", { quiet: true });
      const d = describeError(e, "Scanning your library");
      if (!d.silent)
        syncNotice(SCAN_KEY, {
          level: "warning",
          title: d.title,
          body: d.body,
          actions: [{ label: "Try again", run: () => runScan(false), primary: true }],
          dismissible: true,
        });
    } finally {
      scanning = false;
      lastScanAt = Date.now();
      op.end();
      scanOp = null;
    }
    // Auto-file any mods still loose in the flat root (e.g. freshly downloaded),
    // keeping them active so filing is transparent to the game.
    // Never auto-file mid-move: library writes are refused until the move finishes.
    if (auto && autoFileNew && !archiveMove.unfinished) {
      if (mods.some(isFileable)) {
        await autoFile();
      }
    }
  }

  onMount(() => {
    let unlisten: (() => void) | undefined;
    let dlUnlisten: (() => void) | undefined;
    let focusUnlisten: (() => void) | undefined;
    let moveUnlisten: (() => void) | undefined;
    let closeUnlisten: (() => void) | undefined;
    // Suppress the WebView's built-in right-click menu (reload/inspect junk) app-wide,
    // but keep the native cut/copy/paste menu inside editable fields. Our own row menu
    // (ModListRow / ModCard → Menu) still opens; it sets its own handler on the row.
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || t.closest("input, textarea, select"))) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
    (async () => {
      unlisten = await onScanProgress((p) => {
        progress = p;
        scanOp?.progress(p.done, p.total);
      });
      moveUnlisten = await archiveMove.listen();
      try {
        const rows = await getCuration();
        curationMap = Object.fromEntries(rows.map((r) => [r.techName, r]));
        const ovs = await getOverrides();
        overrideMap = Object.fromEntries(
          ovs.map((o) => [o.techName, { category: o.category, subcategory: o.subcategory }]),
        );
        await loadTags();
        await loadRepos();
        await loadSources();
        await loadLoadouts();
        await loadSavegames();
        gameInfo = await detectGame();
        userDir = await userDirPath();
        appVer = await appVersion();
        // Fire-and-forget: never let a slow/absent update endpoint delay startup.
        checkUpdate().then((u) => (update = u));
        settingsModsSet = new Set(await modsWithSettings());
      } catch (e) {
        reportError(e, "Starting Silo", { background: true });
      }
      try {
        modsStatus = await modsRootStatus();
        roots = rootsFor(modsStatus);
        if (
          showOverrideNotice(modsStatus, localStorage.getItem("silo.overrideNoticeShown") === "1")
        ) {
          overrideNoticeOpen = true;
        }
      } catch (e) {
        reportError(e, "Reading the mods folder setting", { retry: retryModsFolder, background: true });
      }
      // Where the library archive lives, and any unfinished move (paused before a restart).
      // Awaited when there is a folder to scan, so the first scan's auto-file guard sees a move
      // left paused before a restart. With nothing to scan (the mods-folder status failed or is
      // unavailable), deep links, bisect recovery and the focus listener don't wait on it.
      const archiveReady = archiveMove.load();
      if (roots.length) await archiveReady;
      try {
        // e.g. the library database was restored from a backup: worth reading, so it stays up
        // until dismissed.
        const notes = await takeStartupNotices();
        if (notes.length)
          setNotice({ key: STARTUP_KEY, level: "info", title: notes.join(" "), dismissible: true });
      } catch {
        /* non-critical */
      }
      if (roots.length) runScan();
      // Detect a cloud-synced mods folder (OneDrive / Google Drive / Proton Drive): those reject
      // hardlinks, so Silo projects the active set as copies. Warn once so the extra disk use and
      // slower bulk changes aren't a mystery — and so "Optimize links" can explain itself.
      if (roots.length) {
        hardlinkSupport()
          .then((ok) => {
            cloudSynced = !ok;
            if (cloudSynced && localStorage.getItem("silo.cloudNoticeDismissed") !== "1") {
              cloudNoticeOpen = true;
            }
          })
          .catch(() => {});
      }
      // A leftover snapshot means a bisection was interrupted — offer to restore.
      try {
        bisectRecovery = await bisectSnapshotGet();
      } catch {
        /* non-critical */
      }
      // Deep links: a silo://collection?url=… link (from a shared collection's web page)
      // opens the import panel pre-filled. getCurrent() catches a cold launch via link;
      // onOpenUrl fires while the app is already running.
      try {
        handleDeepLinks(await getCurrent());
      } catch {
        /* not a desktop deep-link context */
      }
      try {
        dlUnlisten = await onOpenUrl(handleDeepLinks);
      } catch {
        /* ignore */
      }
      // Auto-refresh on window focus: a mod downloaded in a browser lands while Silo is in the
      // background, so pick it up when the user returns instead of waiting for a manual rescan.
      // Throttled (skip during a library write or scan, or scanned <4s ago) so alt-tabbing doesn't thrash; the
      // scan cache (path+mtime+size) makes an unchanged rescan cheap anyway.
      try {
        focusUnlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (
            focused &&
            !scanning &&
            !writeLock.blocked &&
            !archiveMove.running &&
            !bisecting &&
            roots.length &&
            Date.now() - lastScanAt > 4000
          ) {
            void runScan(true);
          }
        });
      } catch {
        /* not a desktop window context */
      }
      // Closing while Silo is changing files is held by the backend; ask before quitting.
      try {
        // One question at a time: a second close while the dialog is up is ignored.
        let asking = false;
        closeUnlisten = await onCloseBlocked(async () => {
          if (asking) return;
          asking = true;
          try {
            const quit = await confirmDialog(
              "Silo is still changing files. Quitting now could leave a mod half-moved. Quit anyway?",
              { okLabel: "Quit anyway", cancelLabel: "Keep Silo open" },
            );
            if (quit) await forceQuit();
          } finally {
            asking = false;
          }
        });
      } catch {
        /* not a desktop window context */
      }
    })();
    return () => {
      unlisten?.();
      dlUnlisten?.();
      focusUnlisten?.();
      moveUnlisten?.();
      closeUnlisten?.();
      window.removeEventListener("contextmenu", onCtx);
    };
  });

  async function restoreBisect() {
    if (!bisectRecovery) return;
    const op = beginWrite("Restoring your mod set");
    if (!op) return;
    // Held through the rescan, so the library shows the restored set before anything else runs.
    try {
      let restored = false;
      try {
        // Clears even if the report lists errors (applyActive shows them): this banner is the
        // escape hatch, and keeping the snapshot would block every new bisection for good.
        await applyActive(bisectRecovery);
        await bisectSnapshotClear();
        bisectRecovery = null;
        restored = true;
      } catch (e) {
        reportError(e, "Restoring your mod set", { retry: restoreBisect });
      }
      if (restored) await runScan(false, undefined, true);
    } finally {
      op.end();
    }
  }

  /** Forget an interrupted bisection without restoring it (asked first: it can't be undone). */
  async function discardBisect() {
    if (!bisectRecovery) return;
    const ok = await ask({
      title: "Forget the interrupted diagnosis?",
      body: "Your active mods stay as they are now, and Silo won't offer to restore the earlier set again. This can't be undone.",
      confirm: "Discard",
      cancel: "Keep it",
      tone: "danger",
    });
    if (!ok) return;
    try {
      await bisectSnapshotClear();
      bisectRecovery = null;
    } catch (e) {
      reportError(e, "Discarding the interrupted diagnosis");
    }
  }

  // ── App shell: header search, status chip, Play, the Library strip and menu (#230) ──

  // A Silo update that's ready: a notice (it was a top-bar button) driven by the same state, with
  // the same action in Settings → About. Dismissed, it stays down until the next launch.
  let updateNoticeDismissed = $state(false);
  $effect(() => {
    syncNotice(
      "self-update",
      update && !updateNoticeDismissed
        ? {
            level: "info",
            icon: "update",
            // A ready update is never folded under "+N more".
            pinned: true,
            title: `Silo ${update.version} is ready`,
            body: updating ? (updatePct != null ? `Updating… ${updatePct}%` : "Updating…") : undefined,
            actions: [
              {
                label: "Restart to update",
                run: applyUpdate,
                primary: true,
                disabled: updating,
                title: updating ? "Silo is updating." : `Download and install Silo ${update.version}, then restart`,
              },
            ],
            dismissible: true,
            onDismiss: () => (updateNoticeDismissed = true),
          }
        : null,
    );
  });

  /** Browse's catalog search text (the header search while Browse is open). */
  let browseQuery = $state("");
  /** The header search shows the current view's search: the catalog on Browse, else the library's. */
  function getHeaderSearch(): string {
    return searchTargetFor(nav.view) === "browse" ? browseQuery : query;
  }
  function setHeaderSearch(v: string) {
    if (searchTargetFor(nav.view) === "browse") browseQuery = v;
    else query = v;
  }
  /** Enter away from Library and Browse takes the search to the Library (the text is applied). */
  function submitHeaderSearch(v: string) {
    if (searchTargetFor(nav.view) !== "library") return;
    query = v;
    go("library");
  }

  // Problems: critical conflicts and active mods that can't load. To review: warning-level
  // conflicts, files Silo left untouched and inactive mods that couldn't load. Info-level
  // findings never count.
  const statusCounts = $derived.by(() => {
    const on = (m: ModEntry) => activeSet.has(m.techName);
    const deps = health.missingDeps.map((d) => d.mod);
    const activeOf = (list: ModEntry[]) => list.filter(on).length;
    const all = deps.length + health.corrupt.length + health.ignored.length;
    const missingDeps = activeOf(deps);
    const corrupt = activeOf(health.corrupt);
    const ignored = activeOf(health.ignored);
    return libraryStatusCounts({
      criticalConflicts: criticalCount,
      warningConflicts: conflicts.filter((c) => c.severity === "warning").length,
      missingDeps,
      corrupt,
      ignored,
      inactiveFindings: all - missingDeps - corrupt - ignored,
      foreign: health.foreign.length,
    });
  });
  // The chip's counts only mean something once a scan of this folder has succeeded.
  const libraryRead = $derived(libraryReadState({ scanning, scanned: libraryScanned, failed: scanFailed }));

  const activeLoadoutName = $derived(
    activeLoadoutId !== null ? (loadouts.find((l) => l.id === activeLoadoutId)?.name ?? null) : null,
  );

  // Play keeps the old Launch button's guards: off during a foreground library write or while an
  // archive move is copying (launching would race it).
  const playOffReason = $derived(
    archiveMove.running ? "An archive move is running. Pause it to play." : fgLock.reason,
  );

  /** Saves (already loaded) that use mods the library doesn't have. */
  const savesMissingMods = $derived(saveRows.filter((r) => r.st.missing.length > 0));
  const playMenuItems = $derived.by((): PlayMenuItem[] => {
    const items: PlayMenuItem[] = [];
    if (gameInfo) items.push({ id: "game-folder", label: "Open the game folder", icon: "showFile" });
    savesMissingMods.slice(0, 3).forEach(({ s, st }, i) => {
      const n = st.missing.length;
      items.push({
        id: `save:${s.folder}`,
        label: `${s.name} uses ${modsCount(n)} that ${n === 1 ? "isn't" : "aren't"} in your library`,
        icon: "problem",
        separatorBefore: i === 0 && items.length > 0,
      });
    });
    items.push({
      id: "mod-sets",
      label: "Manage mod sets",
      icon: "modSets",
      separatorBefore: items.length > 0,
    });
    return items;
  });
  function playMenuSelect(id: string) {
    if (id === "game-folder")
      openKnownFolder("gameInstall").catch((e) => reportError(e, "Opening the game folder"));
    else if (id.startsWith("save:")) go("modSets", "savegames");
    else if (id === "mod-sets") go("modSets");
  }

  /** Why the old "Organize N" button was off: a library write, a scan, or an unfinished move. */
  const organizeOffReason = $derived(
    fgLock.reason ||
      (scanning ? "Wait for the scan to finish." : "") ||
      (archiveMove.unfinished ? "Finish or cancel the archive move first" : ""),
  );
  const libraryStripItems = $derived.by((): StatusStripItem[] => {
    const run: Record<string, () => void> = {
      critical: () => go("diagnose", "conflicts"),
      unfiled: () => (organizeOffReason ? refuse(organizeOffReason) : void organizeNew()),
      updates: () => go("updates"),
    };
    return stripItemSpecs({
      criticalConflicts: criticalCount,
      // Where the old "Organize N" button showed: loose mods, with auto-file off.
      unfiled: autoFileNew ? 0 : unorganizedCount,
      updates: availableUpdates.length,
    }).map((it) => ({ ...it, run: run[it.id] }));
  });
  const libraryStripLeft = $derived(
    stripLeft({
      scanning,
      scanned: libraryScanned,
      // "Ready to play" only when Play would work: the game is found, Play isn't off and no
      // critical conflict would make it ask first.
      ready: !!gameInfo && !playOffReason && criticalCount === 0,
      active: activeSet.size,
      total: mods.length,
      setName: activeLoadoutName,
    }),
  );

  // The Library's "More" menu: the old Rescan button and Settings' rebuild, with their guards.
  const libraryMenuItems = $derived<MenuItem[]>([
    {
      id: "rescan",
      label: scanning ? "Scanning…" : "Rescan",
      icon: "rescan",
      disabled: scanning || !!fgLock.reason,
      shortcut: "F5",
      keyshortcuts: "F5",
    },
    { id: "rebuild", label: "Re-read every mod", disabled: !!fgLock.reason || scanning },
  ]);
  function libraryMenuSelect(id: string) {
    if (id === "rescan") runScan();
    else if (id === "rebuild") rebuildLibrary();
  }

  // ── Keyboard shortcuts (lib/state/shortcuts): one window listener, each key runs the handler
  // its button would. F5 / Ctrl+R take the Library ⋯ Rescan path with the same guard as the menu
  // item (off while scanning or during a foreground change), and never reload the window.
  let appHeader = $state<AppHeader | null>(null);
  let shortcutsOpen = $state(false);

  function runShortcut(a: ShortcutAction) {
    switch (a.kind) {
      case "focusSearch":
        appHeader?.focusSearch();
        break;
      case "rescan": {
        const item = libraryMenuItems.find((i) => i.id === "rescan");
        if (item && !item.disabled) libraryMenuSelect("rescan");
        break;
      }
      case "settings":
        go("settings");
        break;
      case "view":
        go(a.view);
        break;
      case "back":
        back();
        break;
      case "region":
        cycleRegion(document, a.dir, document.activeElement);
        break;
      case "help":
        shortcutsOpen = true;
        break;
    }
  }

  // Shortcuts stand down behind a dialog and behind the overlays App draws itself (they aren't on
  // the overlay stack): the Organize preview, a settings editor, a mod's settings, the row
  // context menu and the category menu. Reload keys are still swallowed there, without a rescan.
  $effect(() =>
    installShortcuts(window, runShortcut, () =>
      shortcutsBlocked(get(modalOpen), [organizePreview, configEditor, settingsMod, rowMenu, editing]),
    ),
  );

  // The mod drawer hands focus back when it closes (✕, Esc, Open in Browse…): to the row of the
  // mod it showed (by key, scrolled into view), else to what had focus in the main area when it
  // opened, else to the main area, so focus never drops to <body>. Left alone when focus is still
  // somewhere real (a rail click that left the Library).
  let drawerWasOpen = false;
  let drawerOpener: HTMLElement | null = null;
  let drawerModKey: string | null = null;
  $effect(() => {
    const open = !!detailMod && nav.view === "library";
    const key = detailMod ? modKey(detailMod) : null;
    untrack(() => {
      if (open) drawerModKey = key; // follows the shown mod (Find in Library can swap it)
      if (open && !drawerWasOpen) {
        const a = document.activeElement;
        drawerOpener = a instanceof HTMLElement && regionOf(a) === "main" ? a : null;
      } else if (!open && drawerWasOpen) {
        const opener = drawerOpener;
        const back = drawerModKey;
        drawerOpener = null;
        drawerModKey = null;
        void tick().then(() => {
          const a = document.activeElement;
          if (a && a !== document.body && a.isConnected) return;
          if (back && libList?.focusKey(back)) return;
          (opener?.isConnected ? opener : document.getElementById("main"))?.focus();
        });
      }
      drawerWasOpen = open;
    });
  });
</script>

{#snippet modDrawer()}
  {#if detailMod}
    {@const dm = detailMod}
    {@const fgn = foreignFiles.find((f) => f.techName === dm.techName) ?? null}
    <ViewBoundary name="the mod details" onClose={() => (detailMod = null)}>
      <ModDetail
        mod={dm}
        curation={cur(dm.techName)}
        tags={tagsOf(dm.techName)}
        active={activeSet.has(dm.techName)}
        organized={dm.organized}
        hasSettings={settingsModsSet.has(dm.techName)}
        foreign={fgn}
        onAdoptForeign={async () => {
          if (fgn && (await resolveForeign(fgn.fileName, "adopt"))) detailMod = null;
        }}
        onRestoreForeign={async () => {
          if (fgn && (await resolveForeign(fgn.fileName, "restore"))) detailMod = null;
        }}
        {libraryTechNames}
        {conflicts}
        categories={CATEGORIES}
        isOverridden={overrideMap[dm.techName] != null}
        onSetCategory={(c, s) => setCategory(dm.techName, c, s)}
        onResetCategory={() => resetCategory(dm.techName)}
        onClose={() => (detailMod = null)}
        onToggle={(flag) => toggleCuration(dm.techName, flag)}
        onToggleActive={() => toggleActive(dm.techName)}
        onOpenSettings={() =>
          (settingsMod = { techName: dm.techName, title: dm.title ?? dm.techName })}
        onFindInBrowse={() => openInBrowse(dm)}
        onCurationChange={(row) => (curationMap = { ...curationMap, [dm.techName]: row })}
        onTagsChange={(t) => (tagMap = { ...tagMap, [dm.techName]: t })}
        onFilterTag={(t) => {
          selectedTag = t;
          detailMod = null;
        }}
        repo={repoMap[dm.techName] ?? null}
        onRepoChange={(r) => {
          const next = { ...repoMap };
          if (r) next[dm.techName] = r;
          else delete next[dm.techName];
          repoMap = next;
        }}
        source={sourceMap[dm.techName] ?? null}
        onSourceChange={(row) => setSource(dm.techName, row)}
        onInstalled={() => {
          detailMod = null;
          runScan(false);
        }}
      />
    </ViewBoundary>
  {/if}
{/snippet}

<AppShell
  badges={{ updates: availableUpdates.length, diagnose: bisecting ? BISECT_BADGE : undefined }}
  fill={nav.view === "library"}
  wide={nav.view === "library" || nav.view === "browse"}
  drawer={detailMod && nav.view === "library" ? modDrawer : undefined}
  drawerLabel="Mod details"
>
  {#snippet header()}
    <AppHeader
      bind:this={appHeader}
      bind:search={getHeaderSearch, setHeaderSearch}
      searchPlaceholder={searchPlaceholderFor(nav.view)}
      onsearchsubmit={submitHeaderSearch}
    >
      {#snippet status()}
        <StatusChip
          problems={statusCounts.problems}
          review={statusCounts.review}
          library={libraryRead}
          onclick={() => go("diagnose")}
        />
      {/snippet}
      {#snippet play()}
        {#if gameInfo}
          <PlayButton
            count={activeSet.size}
            disabled={!!playOffReason}
            disabledReason={playOffReason}
            onplay={launch}
            menuItems={playMenuItems}
            onmenuselect={playMenuSelect}
            activeSetLine={activeSetLine(activeLoadoutName)}
          />
        {/if}
      {/snippet}
    </AppHeader>
  {/snippet}

  {#snippet notices()}
    {#if scanning}
      <div class="progress">
        <div class="bar" style="width: {pct}%"></div>
        <span class="progress-text tnum">{progress.done} / {progress.total}</span>
      </div>
    {/if}
    {#if statusOp}
      <div class="busy" role="status">
        {statusOp.label}…{#if statusOp.progress && statusOp.progress.total > 0}
          <span class="tnum">{statusOp.progress.done}/{statusOp.progress.total}</span>{/if}
      </div>
    {/if}
    <NoticeArea />
  {/snippet}

  <ViewOutlet view={nav.view}>
    {#snippet library()}
      <LibraryView
        stripLeft={libraryStripLeft}
        stripItems={libraryStripItems}
        menuItems={libraryMenuItems}
        onmenuselect={libraryMenuSelect}
      >
        {#snippet filters()}
          <FilterBar
            catTree={libCatTree}
            bind:parentCategory={libCat}
            bind:childCategory={libSub}
            showSearch={false}
            bind:sort={sortBy}
            sortOptions={[
              { value: "added", label: "Recently added" },
              { value: "name", label: "Name" },
              { value: "category", label: "Category" },
              { value: "size", label: "Size" },
              { value: "version", label: "Version" },
              { value: "rating", label: "My rating" },
            ]}
            bind:sortDir
            dirEnabled={true}
            showYear={false}
            showClear={false}
            filterCount={libFilterCount}
            onClear={clearLibFilters}
          />

          <!-- Library-only filters (they have no Browse equivalent). -->
          <div class="lib-toggles">
            <button class="lib-tog" class:on={favoritesOnly} onclick={() => (favoritesOnly = !favoritesOnly)}>
              <Icon name="favorite" size={13} filled={favoritesOnly} /> Favorites
            </button>
            <button class="lib-tog" class:on={showHidden} onclick={() => (showHidden = !showHidden)}>
              Hidden
            </button>
            <button class="lib-tog" class:on={flaggedOnly} onclick={() => (flaggedOnly = !flaggedOnly)}>
              <Icon name="flagged" size={13} /> Flagged
            </button>
            <button
              class="lib-tog"
              class:on={conflictedOnly}
              disabled={conflictedSet.size === 0}
              onclick={() => (conflictedOnly = !conflictedOnly)}
            >
              <Icon name="problem" size={13} /> In conflict{conflictedSet.size > 0 ? ` (${conflictedSet.size})` : ""}
            </button>
            <button class="lib-tog" class:on={hasSettingsOnly} onclick={() => (hasSettingsOnly = !hasSettingsOnly)}>
              <Icon name="modSettings" size={13} /> Has settings
            </button>
            <button
              class="lib-tog lib-upd"
              class:on={needsUpdateOnly}
              disabled={updateChecking}
              title="Show only mods with a newer version available. Checks for updates the first time."
              onclick={toggleNeedsUpdate}
            >
              <Icon name="update" size={13} />
              {updateChecking ? "Checking…" : `Needs update${availableUpdates.length > 0 ? ` (${availableUpdates.length})` : ""}`}
            </button>
            {#if libFilterCount > 0}
              <button class="lib-clear" onclick={clearLibFilters}>Clear all <Icon name="close" size={13} /></button>
            {/if}
          </div>

          <LibraryToolbar
            {selectedTag}
            shownCount={filtered.length}
            allActive={allFilteredActive}
            activeInFilter={filteredActiveCount}
            disabled={writeLock.blocked || filtered.length === 0}
            disabledReason={writeLock.reason}
            bind:libView
            bind:statFilter
            statCounts={{
              maps: stats.maps,
              scripts: stats.scripts,
              unique: stats.unique,
              active: activeSet.size,
            }}
            onSelectAll={setActiveForFiltered}
            onClearTag={() => (selectedTag = null)}
          />
        {/snippet}
        {#snippet list()}
          <div class="list-body">
            {#if filtered.length === 0 && !scanning}
              <div class="empty">
                {mods.length === 0
                  ? "No mods found yet. Point Silo at your mods folder and rescan."
                  : "No mods match your filter."}
              </div>
            {:else if libView === "cards"}
              <VirtualGrid
                bind:this={libList}
                items={visible}
                getKey={modKey}
                itemHeight={CARD_HEIGHT}
                bottomPad={16}
                label="Mods"
                onopen={(mod) => (detailMod = mod)}
                ontoggle={(mod) => toggleActive(mod.techName)}
                onselect={(mod, intent) => selectMod(modKey(mod), intent)}
                onmenu={(mod, anchor) => openRowMenu(mod, { anchor })}
              >
                {#snippet item(mod)}
                  {@const c = cur(mod.techName)}
                  <ModCard
                    {mod}
                    active={activeSet.has(mod.techName)}
                    badges={badgesFor(mod)}
                    favorite={c.favorite}
                    flagged={c.broken}
                    rating={c.rating}
                    menuOpen={rowMenuOpen && rowMenu?.mod.path === mod.path}
                    onToggle={() => toggleActive(mod.techName)}
                    onOpen={() => (detailMod = mod)}
                    onMenu={(at) => openRowMenu(mod, at)}
                    onSelect={selectMod}
                  />
                {/snippet}
              </VirtualGrid>
            {:else}
              <VirtualList
                bind:this={libList}
                items={visible}
                getKey={modKey}
                rowHeight={ROW_HEIGHTS[libDensity]}
                bottomPad={16}
                label="Mods"
                onopen={(mod) => (detailMod = mod)}
                ontoggle={(mod) => toggleActive(mod.techName)}
                onselect={(mod, intent) => selectMod(modKey(mod), intent)}
                onmenu={(mod, anchor) => openRowMenu(mod, { anchor })}
              >
                {#snippet row(mod)}
                  {@const c = cur(mod.techName)}
                  <ModListRow
                    {mod}
                    active={activeSet.has(mod.techName)}
                    badges={badgesFor(mod)}
                    density={libDensity}
                    favorite={c.favorite}
                    flagged={c.broken}
                    menuOpen={rowMenuOpen && rowMenu?.mod.path === mod.path}
                    onToggle={() => toggleActive(mod.techName)}
                    onOpen={() => (detailMod = mod)}
                    onMenu={(at) => openRowMenu(mod, at)}
                    onSelect={selectMod}
                  />
                {/snippet}
              </VirtualList>
            {/if}
          </div>

        {/snippet}
      </LibraryView>
    {/snippet}
    {#snippet browse()}
      <BrowseView>
        <ModBrowser
          installed={libraryTechNames}
          onInstalled={() => runScan(true)}
          onNeedAuth={() => go("settings", "accounts")}
          onFindInLibrary={openInLibrary}
          seed={browseSeed}
          seedTech={browseSeedTech}
          onSeedConsumed={() => {
            browseSeed = null;
            browseSeedTech = null;
          }}
          showTitle={false}
          bind:query={browseQuery}
        />
      </BrowseView>
    {/snippet}
    {#snippet modSets()}
      <ModSetsView>
        {#snippet setsTab()}
          <ViewBoundary name="Loadouts">
            <LoadoutsPanel
              {loadouts}
              activeCount={activeSet.size}
              {activeLoadoutId}
              lockReason={fgLock.reason}
              onApply={applyLoadout}
              onOverwrite={overwriteLoadout}
              onExport={exportLoadout}
              onRemove={removeLoadout}
              onSaveCurrent={saveCurrentLoadout}
              onImport={importLoadout}
            />
          </ViewBoundary>
        {/snippet}
        {#snippet shareTab()}
          <ViewBoundary name="Collections">
            <div class="page-card">
              <Collections
                active={activeModRefs}
                library={mods}
                initialImportUrl={collImportUrl}
                onImported={() => runScan(false)}
              />
            </div>
          </ViewBoundary>
        {/snippet}
        {#snippet multiplayerTab()}
          <ViewBoundary name="Multiplayer">
            <div class="page-card">
              <MpSync active={activeModRefs} />
            </div>
          </ViewBoundary>
        {/snippet}
        {#snippet savegamesTab()}
          <ViewBoundary name="Savegames">
            <SavegamesPanel
              rows={saveRows}
              lockReason={fgLock.reason}
              onConfigure={openSaveConfig}
              onBackup={backupSave}
              onLoadout={loadoutFromSave}
            />
          </ViewBoundary>
        {/snippet}
      </ModSetsView>
    {/snippet}
    {#snippet diagnose()}
      <!-- Last run's content is the KeepMounted below, docked into this tab's slot. -->
      <DiagnoseView
        conflictsBadge={conflicts.filter((c) => c.severity !== "info").length}
        problemsBadge={healthCount}
        bind:lastRunSlot={diagnoseLastRunSlot}
      >
        {#snippet conflictsTab()}
          <ViewBoundary name="Conflicts" bodyText="Your mods weren't changed.">
            <ConflictsPanel
              {conflicts}
              {criticalCount}
              activeCount={activeSet.size}
            />
          </ViewBoundary>
        {/snippet}
        {#snippet problemsTab()}
          <ViewBoundary name="Library health">
            <HealthPanel {health} {healthCount} onResolveForeign={resolveForeign} />
          </ViewBoundary>
        {/snippet}
      </DiagnoseView>
    {/snippet}
    {#snippet updates()}
      <UpdatesView
        checking={updateChecking}
        checkDisabled={!!fgLock.reason || updateChecking || mods.length === 0}
        checkDisabledReason={fgLock.reason || (mods.length === 0 ? "No mods to check yet." : "")}
        oncheck={() => checkAllUpdates()}
      >
        <UpdatesPanel
          {updateResults}
          {availableUpdates}
          {updateChecking}
          lockReason={fgLock.reason}
          {checkYourself}
          onInstall={installFromRow}
          onOpenSource={(s) => openExternal(s.url)}
          onMarkChecked={markChecked}
        />
      </UpdatesView>
    {/snippet}
    {#snippet tools()}
      <ToolsView>
        {#snippet statsTab()}
          <ViewBoundary name="Library stats" bodyText="Your mods weren't changed.">
            <StatsPanel
              modCount={mods.length}
              {organizedCount}
              activeCount={activeSet.size}
              mapsCount={stats.maps}
              {libStats}
            />
          </ViewBoundary>
        {/snippet}
        {#snippet controlsTab()}
          <ViewBoundary name="Bindings" bodyText="Your mods weren't changed.">
            <div class="page-card"><BindingsView /></div>
          </ViewBoundary>
        {/snippet}
        {#snippet bridgeTab()}
          <ViewBoundary name="the filltype bridge">
            <div class="page-card"><BridgeTool onGenerated={() => runScan(true)} /></div>
          </ViewBoundary>
        {/snippet}
        {#snippet graphicsTab()}
          <div class="tool-note">
            <p>
              Tune Farming Simulator 25's graphics settings (presets: Performance / Balanced /
              Quality) without launching the game. Changes take effect the next time you launch it.
            </p>
            {#if userDir}
              <Button icon="modSettings" onclick={openGameGraphics}>Edit game.xml</Button>
            {:else}
              <p class="muted">Silo couldn't find your Farming Simulator 25 user folder.</p>
            {/if}
          </div>
        {/snippet}
      </ToolsView>
    {/snippet}
    {#snippet settings()}
      <SettingsView>
        {#snippet generalTab()}
          <GeneralSettings
            {theme}
            {uiScale}
            {userDir}
            onSetTheme={setTheme}
            onSetUiScale={setUiScale}
            onOpenGameGraphics={openGameGraphics}
          />
        {/snippet}
        {#snippet modsFolderTab()}
          <ModsFolderSettings
            {roots}
            {modsStatus}
            {gameInfo}
            lockReason={fgLock.reason}
            {scanning}
            moveUnfinished={archiveMove.unfinished}
            onChangeModsFolder={changeModsFolder}
            onUseGameFolder={useGameFolder}
            onSwitchModsFolder={switchModsFolder}
          />
        {/snippet}
        {#snippet libraryTab()}
          <LibrarySettings
            {autoFileNew}
            {organizedCount}
            {unorganizedCount}
            lockReason={fgLock.reason}
            {scanning}
            archive={archiveMove.archive}
            moveUnfinished={archiveMove.unfinished}
            archiveLoadFailed={archiveMove.loadFailed}
            onSetAutoFile={setAutoFile}
            onOrganize={organizeNew}
            onChangeArchive={changeArchive}
            onArchiveDefault={archiveDefault}
          />
        {/snippet}
        {#snippet accountsTab()}
          <AccountSettings />
        {/snippet}
        {#snippet diagnosticsTab()}
          <DiagnosticsSettings onCopyDiagnostics={copyDiagnostics} onExportReport={exportReport} />
        {/snippet}
        {#snippet maintenanceTab()}
          <MaintenanceSettings
            {organizedCount}
            {cloudSynced}
            lockReason={fgLock.reason}
            {scanning}
            moveUnfinished={archiveMove.unfinished}
            onRebuild={rebuildLibrary}
            onOptimizeLinks={optimizeLinks}
            onRestoreVanilla={restoreVanilla}
          />
        {/snippet}
        {#snippet aboutTab()}
          <AboutSettings
            {appVer}
            scanMs={result ? result.tookMs : null}
            {update}
            {updating}
            {updatePct}
            onApplyUpdate={applyUpdate}
          />
        {/snippet}
      </SettingsView>
    {/snippet}
    {#snippet help()}
      <HelpView
        onopen={(url) => openExternal(url).catch((e) => reportError(e, "Opening the link"))}
        oncopydiagnostics={copyDiagnostics}
        onshowshortcuts={() => (shortcutsOpen = true)}
      />
    {/snippet}
  </ViewOutlet>
  <!-- Diagnose → Last run (log triage and guided bisection). Mounted with its tab; while a
       bisection runs it stays mounted (hidden) when you switch tabs or views, so the run isn't
       cut short, and comes back where it was. -->
  <KeepMounted target={diagnoseLastRunSlot} keep={bisecting}>
    <ViewBoundary
      name="Crash & log triage"
      bodyText="Your mods are as they were before this view failed. If a diagnosis was running, Silo keeps your place."
    >
      <div class="page-card">
        <LogTriage
          activeMods={[...activeSet]}
          activeMaps={mods.filter((m) => activeSet.has(m.techName) && m.isMap).map((m) => m.techName)}
          onApply={bisectRound}
          onBisecting={(v) => void bisectingChanged(v)}
          recoveryPending={bisectRecovery !== null}
        />
      </div>
    </ViewBoundary>
  </KeepMounted>
</AppShell>

  {#if settingsMod}
    <ViewBoundary name="Mod settings" overlay onClose={() => (settingsMod = null)}>
      <ModSettings
        modName={settingsMod.techName}
        title={settingsMod.title}
        onClose={() => (settingsMod = null)}
      />
    </ViewBoundary>
  {/if}

  {#if configEditor}
    <ViewBoundary name="the settings editor" overlay onClose={() => (configEditor = null)}>
      <ConfigEditor
        title={configEditor.title}
        path={configEditor.path}
        fields={configEditor.fields}
        presets={configEditor.presets}
        footnote={configEditor.footnote}
        onClose={() => (configEditor = null)}
      />
    </ViewBoundary>
  {/if}

  {#if organizePreview}
    <ViewBoundary name="the Organize preview" overlay onClose={() => (organizePreview = null)}>
      <OrganizePreview
        plan={organizePreview.plan}
        loading={organizePreview.loading}
        applying={organizePreview.applying}
        adopt={organizePreview.adopt ?? null}
        onConfirm={confirmOrganize}
        onCancel={() => (organizePreview = null)}
      />
    </ViewBoundary>
  {/if}

  {#if editing}
    <CategoryMenu
      categories={CATEGORIES}
      x={editing.x}
      y={editing.y}
      onSelect={(c) => setCategory(editing!.techName, c)}
      onReset={() => resetCategory(editing!.techName)}
      onClose={() => (editing = null)}
    />
  {/if}

  {#if rowMenu && rowMenuSpec}
    <Menu
      bind:open={rowMenuOpen}
      anchor={rowMenu?.at.anchor ?? null}
      point={rowMenu?.at.point ?? null}
      placement="bottom-end"
      items={rowMenuSpec?.items ?? []}
      label="Actions for {rowMenu?.mod.title ?? rowMenu?.mod.techName}"
      onselect={(id) => rowMenuRun?.(id)}
    />
  {/if}

<ViewBoundary name="the dialog" overlay>
  <DialogHost />
</ViewBoundary>
<ShortcutsDialog bind:open={shortcutsOpen} />
<ToastRegion />

<style>
  .busy {
    padding: 8px 20px;
    background: color-mix(in srgb, var(--accent) 14%, var(--surface));
    color: var(--accent-text);
    font-size: var(--fs-sm);
    font-weight: 600;
  }
  .progress {
    position: relative;
    height: 4px;
    background: var(--border);
  }
  .bar {
    height: 100%;
    background: var(--accent);
    transition: width 0.1s linear;
  }
  .progress-text {
    position: absolute;
    right: 12px;
    top: 6px;
    font-size: var(--fs-xs);
    color: var(--text-muted);
  }
  /* A tool or panel that was a floating card (log triage, bindings, MP sync, collections, the
     bridge) shown as a page section. Each owns its inner padding. */
  .page-card {
    max-width: 56rem;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
  }
  .tool-note {
    max-width: 48rem;
  }
  .tool-note p {
    margin: 0 0 var(--sp-3);
    max-width: 65ch;
  }
  .tool-note .muted {
    color: var(--muted);
  }
  /* ── Library view (LibraryView lays out the column) ── */
  .lib-toggles {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 7px;
    margin: 0 0 8px;
  }
  .lib-tog {
    border: 1px solid var(--border);
    background: var(--bg);
    color: var(--text-muted);
    padding: 8px 12px;
    border-radius: var(--radius);
    font-size: var(--fs-xs);
    font-weight: 600;
    cursor: pointer;
  }
  .lib-tog:hover {
    color: var(--text);
  }
  .lib-tog:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .lib-tog.on {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    color: var(--accent-text);
  }
  .lib-upd.on {
    background: color-mix(in srgb, var(--accent) 15%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, var(--border));
    color: var(--accent-text);
  }
  .lib-clear {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: var(--fs-sm);
    font-weight: 600;
    cursor: pointer;
    padding: 6px;
  }
  .lib-clear:hover {
    color: var(--primary-text);
    text-decoration: underline;
  }
  .list-body {
    height: 100%;
    min-height: 0;
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-muted);
    font-size: var(--fs-md);
    padding: 40px;
    text-align: center;
  }
</style>
