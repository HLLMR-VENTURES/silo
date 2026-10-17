<script lang="ts">
  import { onMount } from "svelte";
  import {
    modsRootStatus,
    setModsRoot,
    clearModsRoot,
    pointGameAtModsRoot,
    createModsFolder,
    folderLooksEmpty,
    pickFolder,
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
    checkModUpdate,
    catalogCheckUpdates,
    catalogLibraryTags,
    downloadUpdate,
    saveTextFile,
    userDirPath,
    bisectSnapshotGet,
    bisectSnapshotClear,
    appVersion,
    detectForeignFiles,
    adoptForeignFile,
    restoreForeignFile,
    relinkProjections,
    onRelinkProgress,
    hardlinkSupport,
    type ForeignFile,
    confirmDialog,
    archiveStatus,
    archiveMovePreflight,
    archiveMoveStart,
    archiveMoveResume,
    archiveMovePause,
    archiveMoveCancel,
    archiveUseDefault,
    archiveRepoint,
    archiveAdoptPreview,
    archiveAdoptApply,
    onMoveProgress,
    type ArchiveStatus,
    type ArchivePreflight,
    type MoveProgress,
  } from "./lib/api";
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
    ModTag,
    ScanResult,
    CurationRow,
    ModInput,
    PlannedMove,
    Loadout,
    Savegame,
    Conflict,
    GameInfo,
    UpdateRow,
  } from "./lib/types";

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
  import VirtualList from "./lib/components/VirtualList.svelte";
  import ModRow from "./lib/components/ModRow.svelte";
  import ModTile from "./lib/components/ModTile.svelte";
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
  import SettingsPanel from "./lib/components/panels/SettingsPanel.svelte";
  import CategoryMenu from "./lib/components/panels/CategoryMenu.svelte";
  import ContextMenu, { type ContextMenuItem } from "./lib/components/ContextMenu.svelte";
  import Topbar from "./lib/components/panels/Topbar.svelte";
  import StatBar from "./lib/components/panels/StatBar.svelte";
  import LibraryToolbar from "./lib/components/panels/LibraryToolbar.svelte";
  import OrganizePreview from "./lib/components/OrganizePreview.svelte";
  import { fmtSize } from "./lib/format";

  let roots = $state<string[]>([]);
  let modsStatus = $state<ModsRootStatus | null>(null);
  let overrideNoticeOpen = $state(false);
  let mods = $state<ModEntry[]>([]);
  let foreignFiles = $state<ForeignFile[]>([]);
  let scanning = $state(false);
  // Epoch-ms of the last scan attempt — throttles the auto-rescan-on-focus so alt-tabbing
  // doesn't re-scan on every focus.
  let lastScanAt = 0;
  let progress = $state({ done: 0, total: 0 });
  let result = $state<ScanResult | null>(null);
  let query = $state("");
  let errorMsg = $state<string | null>(null);
  // Library category filter, driven by the shared FilterBar's parent/child dropdowns
  // ("" = all). Replaces the old category rail + `selected`.
  let libCat = $state("");
  let libSub = $state("");
  // Catalog facet filter for the Library (silo-api#9): "namespace:value" tags, ANDed, matched
  // against each installed mod's catalog tags.
  let libActiveTags = $state<string[]>([]);
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
  // A single categorical facet from the stat bar (maps/scripts/uniqueType/active) —
  // click a count to scope the list to it; click again (or another) to switch/clear.
  let statFilter = $state<"" | "maps" | "scripts" | "unique" | "active">("");
  let editing = $state<{ techName: string; x: number; y: number } | null>(null);
  let activeSet = $state<Set<string>>(new Set());
  let busy = $state<string | null>(null);
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

  // Silo only organizes .zip mods (the commercial distribution format). Unpacked
  // dir mods are left in place — that's how dev/work-in-progress mods live until
  // they're officially packaged.
  const isFileable = (m: ModEntry) => !m.organized && m.kind === "zip";
  const organizedCount = $derived(mods.filter((m) => m.organized).length);
  const unorganizedCount = $derived(mods.filter(isFileable).length);

  let loadouts = $state<Loadout[]>([]);
  let loadoutsOpen = $state(false);
  let savegames = $state<Savegame[]>([]);
  let savesOpen = $state(false);
  let conflicts = $state<Conflict[]>([]);
  let conflictsOpen = $state(false);
  let conflictTimer: ReturnType<typeof setTimeout> | undefined;
  let settingsOpen = $state(false);
  let healthOpen = $state(false);
  let statsOpen = $state(false);
  let logOpen = $state(false);
  let bindingsOpen = $state(false);
  let mpOpen = $state(false);
  let collectionsOpen = $state(false);
  // Set when a silo://collection?url=… deep link arrives — opens Collections, pre-filled.
  let collImportUrl = $state<string | null>(null);

  function handleDeepLinks(urls: string[] | null | undefined) {
    if (!urls) return;
    for (const raw of urls) {
      const target = parseCollectionDeepLink(raw);
      if (target) {
        collImportUrl = target;
        collectionsOpen = true;
        break;
      }
    }
  }
  let bridgeOpen = $state(false);
  // Active set as manifest refs for MP sync (techName, path, kind, version).
  const activeModRefs = $derived(
    mods
      .filter((m) => activeSet.has(m.techName))
      .map((m) => ({ techName: m.techName, path: m.path, kind: m.kind, version: m.version })),
  );
  // Set when a previous bisection didn't finish (app closed mid-run) — the user's mod
  // set may be partially applied, so offer to restore it.
  let bisectRecovery = $state<string[] | null>(null);
  // Top-level view: the local library, or the remote catalog browser.
  let view = $state<"library" | "browse">("library");
  // Measured so the fixed detail drawers can sit *below* the header instead of over it
  // (the topbar wraps, so its height isn't constant).
  let topbarH = $state(56);

  /** Switch views and close the Library drawer so it doesn't hang over Browse. The
   *  Browse drawer lives inside ModBrowser, which unmounts on switch, so it self-closes. */
  function switchView(v: "library" | "browse") {
    if (view === v) return;
    view = v;
    detailMod = null;
  }
  let userDir = $state<string | null>(null);
  let appVer = $state<string | null>(null);
  let update = $state<AvailableUpdate | null>(null);
  let updating = $state(false);
  let updatePct = $state<number | null>(null);

  async function applyUpdate() {
    if (updating) return;
    updating = true;
    try {
      await installUpdate((p) => (updatePct = p));
      // installUpdate relaunches on success; if we're still here, it was a no-op.
    } catch (e) {
      errorMsg = `Update failed: ${String(e)}`;
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
    settingsOpen = false;
    configEditor = {
      title: "Graphics settings — game.xml",
      path: `${userDir}/game.xml`,
      fields: GAME_GRAPHICS_FIELDS,
      presets: GAME_GRAPHICS_PRESETS,
      footnote: "Changes take effect the next time you launch the game.",
    };
  }

  function openSaveConfig(s: Savegame) {
    if (!userDir) return;
    savesOpen = false;
    configEditor = {
      title: `Configure — ${s.name}`,
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
  // Catalog tags per installed mod (batch-fetched from the catalog), for the Library facets.
  let catalogTagsMap = $state<Record<string, ModTag[]>>({});
  async function loadCatalogTags(techNames: string[]) {
    try {
      const rows = await catalogLibraryTags(techNames);
      const map: Record<string, ModTag[]> = {};
      for (const row of rows) map[row.techName] = row.tags;
      catalogTagsMap = map;
    } catch {
      // Best-effort — an older/offline catalog just leaves the facet dropdowns empty.
    }
  }
  let settingsMod = $state<{ techName: string; title: string } | null>(null);

  async function launch() {
    if (
      criticalCount > 0 &&
      !(await confirmDialog(
        `Your active set has ${criticalCount} critical conflict${criticalCount === 1 ? "" : "s"}. Launch anyway?`,
      ))
    )
      return;
    try {
      await launchGame();
    } catch (e) {
      errorMsg = String(e);
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
      errorMsg = String(e);
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
      errorMsg = String(e);
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
    L.push(`## Active set${al ? ` — loadout “${al.name}”` : ""} (${activeSet.size} mods)`, "");
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
    for (const m of health.corrupt) L.push(`- Corrupt: ${m.techName} — ${m.error}`);
    for (const f of health.foreign)
      L.push(`- Foreign file at a managed name: ${f.fileName} (isn't Silo's — left untouched)`);
    return L.join("\n");
  }

  async function exportReport() {
    try {
      await saveTextFile("silo-report.md", buildReport());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function backupSave(s: Savegame) {
    busy = `Backing up “${s.name}”…`;
    try {
      await backupSavegame(s.folder);
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
  }

  async function loadoutFromSave(s: Savegame) {
    const { present, missing } = saveStats(s);
    if (present.length === 0) {
      errorMsg = `None of “${s.name}”'s mods are in your library yet.`;
      return;
    }
    const note =
      missing.length > 0
        ? `\n\nNote: ${missing.length} mod(s) the save used aren't in your library and will be left out.`
        : "";
    if (
      !(await confirmDialog(
        `Create a loadout “${s.name}” with ${present.length} mod(s) from this save?${note}`,
      ))
    )
      return;
    try {
      await saveLoadout(null, s.name, present.map((m) => m.modName));
      await loadLoadouts();
      savesOpen = false;
      // Apply it immediately.
      const created = loadouts.find((l) => l.name === s.name);
      if (created) await applyLoadout(created);
    } catch (e) {
      errorMsg = String(e);
    }
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
      errorMsg = String(e);
    }
  }

  async function applyLoadout(l: Loadout) {
    loadoutsOpen = false;
    busy = `Applying loadout “${l.name}”…`;
    activeSet = new Set(l.mods);
    try {
      await applyActive(l.mods);
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
  }

  async function saveCurrentLoadout() {
    const name = prompt("Name this loadout:", "");
    if (!name || !name.trim()) return;
    try {
      await saveLoadout(null, name.trim(), [...activeSet]);
      await loadLoadouts();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function overwriteLoadout(l: Loadout) {
    try {
      await saveLoadout(l.id, l.name, [...activeSet]);
      await loadLoadouts();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function removeLoadout(l: Loadout) {
    if (!(await confirmDialog(`Delete loadout “${l.name}”? (Your mods aren't affected.)`))) return;
    try {
      await deleteLoadout(l.id);
      await loadLoadouts();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function exportLoadout(l: Loadout) {
    try {
      await exportLoadoutFile(l.id, l.name);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function importLoadout() {
    try {
      const id = await importLoadoutFile();
      if (id !== null) {
        await loadLoadouts();
        loadoutsOpen = true;
      }
    } catch (e) {
      errorMsg = String(e);
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
    return rep;
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
    const next = new Set(activeSet);
    if (next.has(techName)) next.delete(techName);
    else next.add(techName);
    activeSet = next;
    try {
      await applyActive([...next]);
    } catch (e) {
      errorMsg = String(e);
    }
  }

  // Resolve a foreign file (a mod-folder file that isn't Silo's projection — typically a newer
  // build the user dropped in to update). Adopt = promote it to the managed copy; restore = put
  // Silo's copy back. Shared by the health panel and the mod drawer's "you dropped an update" banner.
  async function resolveForeign(fileName: string, action: "adopt" | "restore") {
    busy = action === "adopt" ? "Adopting the dropped build…" : "Restoring Silo's copy…";
    try {
      if (action === "adopt") await adoptForeignFile(fileName);
      else await restoreForeignFile(fileName);
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
    await runScan(false);
    foreignFiles = await detectForeignFiles(roots[0]).catch(() => []);
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
  // loaded state is preserved (used by both the auto-filer and the manual button).
  async function applyLoose(inputs: ModInput[], techNames: string[], keepActive: boolean) {
    if (inputs.length === 0) return;
    busy = `Filing ${inputs.length} mod${inputs.length > 1 ? "s" : ""} into the library…`;
    try {
      const rep = await applyOrganize(inputs);
      if (rep.errors.length) {
        errorMsg = rep.errors.slice(0, 3).join("; ");
      } else if (keepActive) {
        const next = new Set([...activeSet, ...techNames]);
        activeSet = next;
        await applyActive([...next]);
      }
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
    await runScan(false);
  }

  // Auto-filer (opt-in): file new mods and keep them active — no prompt, they chose it.
  async function autoFile() {
    const { inputs, techNames } = looseInputs();
    await applyLoose(inputs, techNames, true);
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
      errorMsg = String(e);
      organizePreview = null;
    }
  }

  async function confirmOrganize(skipNext: boolean) {
    if (!organizePreview) return;
    if (organizePreview.adopt) {
      const fileNames = organizePreview.inputs.map((m) => m.fileName);
      organizePreview = { ...organizePreview, applying: true };
      let problem: string | null = null;
      try {
        const rep = await archiveAdoptApply(fileNames);
        if (rep.errors.length) problem = rep.errors.slice(0, 3).join("; ");
      } catch (e) {
        problem = String(e);
      }
      organizePreview = null;
      await runScan(false);
      if (problem) errorMsg = problem; // runScan clears errorMsg
      return;
    }
    if (skipNext) localStorage.setItem(PREVIEW_SKIP_KEY, "true");
    const { inputs, techNames } = organizePreview;
    organizePreview = { ...organizePreview, applying: true };
    await applyLoose(inputs, techNames, true);
    organizePreview = null;
  }

  async function rebuildLibrary() {
    busy = "Rebuilding library (re-scanning every mod)…";
    errorMsg = null;
    try {
      await clearScanCache();
      await runScan(false);
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
  }

  async function restoreVanilla() {
    if (
      !(await confirmDialog(
        "Restore a vanilla flat mods/ folder?\n\nThis moves every mod back out of archive/ and removes all links. Your mods are not deleted — this just undoes Silo's organization.",
      ))
    )
      return;
    busy = "Restoring vanilla layout…";
    try {
      const rep = await flatten();
      if (rep.errors.length) errorMsg = rep.errors.slice(0, 3).join("; ");
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
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

  // ── Library row right-click menu ──
  let rowMenu = $state<{ mod: ModEntry; ev: MouseEvent } | null>(null);
  // Seeds ModBrowser's catalog search when "Find in Browse" is used. ModBrowser remounts
  // on every view switch, so setting this before switching views seeds the fresh mount.
  let browseSeed = $state<string | null>(null);
  let browseSeedTech = $state<string | null>(null);

  function openInBrowse(mod: ModEntry) {
    browseSeed = mod.title ?? mod.techName;
    browseSeedTech = mod.techName; // so Browse can open this exact mod's drawer, not just filter to it
    switchView("browse");
  }

  // The reverse of openInBrowse: jump from a Browse card to the installed mod in the Library,
  // filtering to it and opening its drawer.
  function openInLibrary(techName: string) {
    const mod = mods.find((m) => m.techName === techName);
    if (!mod) return;
    query = mod.title ?? mod.techName; // seed the library search so the tile surfaces behind the drawer
    switchView("library"); // clears detailMod…
    detailMod = mod; // …then open this mod's drawer
  }

  function rowMenuItems(mod: ModEntry, ev: MouseEvent): ContextMenuItem[] {
    const c = cur(mod.techName);
    const isActive = activeSet.has(mod.techName);
    const items: ContextMenuItem[] = [
      {
        label: isActive ? "Park (deactivate)" : "Activate",
        icon: isActive ? "○" : "●",
        onClick: () => toggleActive(mod.techName),
      },
      { label: "Open details", icon: "⊞", onClick: () => (detailMod = mod) },
      { label: "Find in Browse", icon: "⌕", onClick: () => openInBrowse(mod) },
      { separator: true },
      { label: "Change category…", icon: "🏷", onClick: () => openEditor(mod.techName, ev) },
    ];
    if (settingsModsSet.has(mod.techName)) {
      items.push({
        label: "Edit settings…",
        icon: "⚙",
        onClick: () =>
          (settingsMod = { techName: mod.techName, title: mod.title ?? mod.techName }),
      });
    }
    items.push(
      { separator: true },
      {
        label: c.favorite ? "Remove favorite" : "Favorite",
        icon: c.favorite ? "★" : "☆",
        onClick: () => toggleCuration(mod.techName, "favorite"),
      },
      {
        label: c.hidden ? "Unhide" : "Hide",
        icon: "⊘",
        onClick: () => toggleCuration(mod.techName, "hidden"),
      },
      {
        label: c.broken ? "Clear broken flag" : "Mark broken",
        icon: "⚠",
        danger: !c.broken,
        onClick: () => toggleCuration(mod.techName, "broken"),
      },
    );
    return items;
  }

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
      errorMsg = String(e);
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
      errorMsg = String(e);
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
      errorMsg = String(e);
    }
  }

  // GitHub repo links: techName -> {owner, repo}
  let repoMap = $state<Record<string, { owner: string; repo: string }>>({});
  async function loadRepos() {
    try {
      const rows = await getModRepos();
      repoMap = Object.fromEntries(rows.map((r) => [r.techName, { owner: r.owner, repo: r.repo }]));
    } catch (e) {
      errorMsg = String(e);
    }
  }

  let updatesOpen = $state(false);
  let updateChecking = $state(false);
  let updateResults = $state<UpdateRow[]>([]);
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
      busy = "All mods are up to date";
      setTimeout(() => (busy = null), 1600);
    }
  }

  async function checkAllUpdates(quiet = false) {
    if (!quiet) updatesOpen = true;
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
        if (!rows.has(techName)) row.error = String(e);
      }
      rows.set(techName, row);
      updateResults = [...rows.values()];
    }

    updateChecking = false;
  }

  async function installFromRow(row: UpdateRow) {
    if (!row.assetUrl) return;
    busy = `Installing ${row.title}…`;
    try {
      await downloadUpdate(row.path, row.assetUrl);
      updateResults = updateResults.map((r) =>
        r.techName === row.techName ? { ...r, hasUpdate: false, current: r.latest } : r,
      );
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
    await runScan(false);
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
      errorMsg = String(e);
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
  // Aggregate the installed set's catalog tags into the same facet dropdowns as Browse.
  const LIB_FACET_ORDER = ["region", "era", "theme", "brand", "realism"];
  const LIB_FACET_LABEL: Record<string, string> = {
    theme: "Theme",
    brand: "Brand",
    region: "Region",
    realism: "Realism",
    era: "Era",
  };
  const libFacets = $derived.by(() => {
    const acc: Record<string, Map<string, number>> = {};
    for (const m of effectiveMods) {
      for (const t of catalogTagsMap[m.techName] ?? []) {
        (acc[t.namespace] ??= new Map()).set(
          t.value,
          (acc[t.namespace]?.get(t.value) ?? 0) + 1,
        );
      }
    }
    const out: Record<string, { value: string; count: number }[]> = {};
    for (const ns of Object.keys(acc)) {
      out[ns] = [...acc[ns].entries()]
        .sort((a, b) => b[1] - a[1])
        .map(([value, count]) => ({ value, count }));
    }
    return out;
  });
  const libFilterCount = $derived(
    (libCat ? 1 : 0) +
      libActiveTags.length +
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
    libActiveTags = [];
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
    if (libActiveTags.length > 0) {
      // Each active tag is "namespace:value"; a mod matches when its catalog tags include it.
      // Split only on the first colon so values with colons survive.
      list = list.filter((m) => {
        const mt = catalogTagsMap[m.techName] ?? [];
        return libActiveTags.every((t) => {
          const i = t.indexOf(":");
          const ns = t.slice(0, i);
          const val = t.slice(i + 1);
          return mt.some((x) => x.namespace === ns && x.value === val);
        });
      });
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
  // Library view: landscape tiles (default) or compact rows.
  let libView = $state<"tiles" | "rows">("tiles");
  const TILE_ROW_H = 216;

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
  // Two tiles per virtualized row for the landscape grid.
  const visiblePairs = $derived.by(() => {
    const out: ModEntry[][] = [];
    for (let i = 0; i < visible.length; i += 2) out.push(visible.slice(i, i + 2));
    return out;
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
        ? " Your mods folder is cloud-synced, so each one is copied/removed individually — this can take a while."
        : "";
      if (!(await confirmDialog(`${verb} all ${changing.length} mods in your library?${tail}`))) return;
    }
    const next = new Set(activeSet);
    for (const m of filtered) {
      if (active) next.add(m.techName);
      else next.delete(m.techName);
    }
    activeSet = next;
    busy = active ? "Activating…" : "Deactivating…";
    let problem: string | null = null;
    try {
      const rep = await applyActive([...next]);
      if (rep.errors.length) problem = rep.errors.slice(0, 3).join("; ");
    } catch (e) {
      problem = String(e);
    }
    busy = null;
    // Reconcile the optimistic set against what's actually on disk — a partial/failed projection
    // (e.g. a file Silo couldn't prove was its own) leaves the two out of sync otherwise.
    await runScan(false);
    if (problem) errorMsg = problem; // runScan clears errorMsg — restore what the projection said
  }

  // Repair copy-projections into hardlinks: reclaims the duplicated space and makes future
  // activate/deactivate O(1) instead of a full-library content hash. Progress-reported so a
  // large one-time pass shows a bar instead of looking frozen.
  async function optimizeLinks() {
    settingsOpen = false;
    let unlisten: (() => void) | null = null;
    let done = "";
    let problem: string | null = null;
    try {
      unlisten = await onRelinkProgress((d, total) => {
        busy = `Optimizing links… ${d}/${total}`;
      });
      busy = "Optimizing links…";
      const rep = await relinkProjections();
      if (rep.crossDriveCopies) {
        done =
          "Your library archive is on another drive and Windows can't link across drives without Developer Mode — active mods stay as copies.";
      } else if (rep.hardlinksUnsupported) {
        cloudSynced = true;
        done = "Mods folder is cloud-synced — hardlinks aren't available, so there's nothing to optimize";
      } else {
        if (rep.errors.length) problem = rep.errors.slice(0, 3).join("; ");
        const gb = rep.reclaimedBytes / 1024 ** 3;
        const freed =
          gb >= 1 ? `${gb.toFixed(1)} GB` : `${Math.round(rep.reclaimedBytes / 1024 ** 2)} MB`;
        done =
          rep.upgraded > 0
            ? `Optimized ${rep.upgraded} mod${rep.upgraded === 1 ? "" : "s"} — reclaimed ${freed}`
            : "Links already optimized — nothing to reclaim";
      }
    } catch (e) {
      problem = String(e);
    } finally {
      unlisten?.();
    }
    await runScan(false); // reconcile + refresh sizes; also clears errorMsg
    if (problem) errorMsg = problem;
    if (done) {
      busy = done;
      // Long enough to read: the cross-drive explanation is a full sentence.
      setTimeout(() => (busy = null), Math.max(2600, done.length * 50));
    } else {
      busy = null;
    }
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
  async function switchBlockReason(): Promise<string | null> {
    if (moveUnfinished) return "Finish or cancel the archive move first.";
    if (scanning) return "Wait for the scan to finish.";
    if (busy) return `Wait for “${busy}” to finish.`;
    const snap = await bisectSnapshotGet().catch(() => null);
    if (snap) return "Finish or cancel the guided bisection first — it belongs to this folder.";
    return null;
  }

  /** Adopt a new status; if the live folder changed, reload everything folder-specific. */
  async function applyModsStatus(st: ModsRootStatus) {
    const prev = roots[0] ?? null;
    modsStatus = st;
    await refreshArchive(); // the archive belongs to the mods folder
    roots = st.available && st.path ? [st.path] : [];
    if ((roots[0] ?? null) === prev) return;
    result = null;
    mods = [];
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
      errorMsg = why;
      return;
    }
    try {
      if (
        (await folderLooksEmpty(path)) &&
        !(await confirmDialog("This folder has no mods yet — use it anyway?", {
          okLabel: "Use this folder",
          cancelLabel: "Cancel",
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
        (await confirmDialog(
          `Point Farming Simulator at this folder too?\n\nFS25 currently reads:\n${st.gameReads ?? "(unknown)"}\n\n` +
            "Silo keeps a backup of gameSettings.xml. Close FS25 first — the game rewrites that " +
            "file when it exits.",
          { okLabel: "Update FS25's setting", cancelLabel: "Only in Silo" },
        ))
      ) {
        await pointGameHere();
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function changeModsFolder() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    const path = await pickFolder();
    if (path) await switchModsFolder(path);
  }

  async function useGameFolder() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    try {
      await applyModsStatus(await clearModsRoot());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function pointGameHere() {
    try {
      await applyModsStatus(await pointGameAtModsRoot());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function retryModsFolder() {
    try {
      await applyModsStatus(await modsRootStatus());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function makeModsFolder() {
    try {
      await applyModsStatus(await createModsFolder());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  // ── Library archive location (#166) ──────────────────────────────────────────
  // A move can run for hours: its invoke stays pending the whole time, so the UI is driven by
  // `archive_status` (polled while running) and `archive-move-progress` events, not the return.
  let archive = $state<ArchiveStatus | null>(null);
  /** The startup read gave up — Settings says so and retries once when opened. */
  let archiveLoadFailed = $state(false);
  let moveProgress = $state<MoveProgress | null>(null);
  /** Move invokes (start / resume / cancel / use default) this window is still waiting on. */
  let movesInFlight = $state(0);
  /** Pause was clicked; the move pauses at the next mod boundary. */
  let pauseRequested = $state(false);
  /** Cancel was confirmed; shows "moving back" before the backend reports the turn-around. */
  let cancelInFlight = $state(false);
  /** A neutral, dismissible notice (not an error). */
  let infoMsg = $state<string | null>(null);
  /** A move is copying right now — Pause applies, Launch is blocked. */
  const moveRunning = $derived(!!archive?.running || movesInFlight > 0);
  /** A move is running or paused — library writes are refused until it finishes. */
  const moveUnfinished = $derived(moveRunning || !!archive?.moving);
  const moveCancelling = $derived(
    archive?.moving?.direction === "back" ||
      cancelInFlight ||
      (moveRunning && !!moveProgress?.cancelling),
  );
  // Lost-cancel detection: Cancel was clicked on a running move, but it may finish first.
  let cancelRequested = false;
  /** Where the move started when Cancel was clicked (the archive path during a move). */
  let cancelHome: string | null = null;
  let cancelHomeDefault = false;
  /** The cancel invoke's own error, held until we know whether the cancel was lost. */
  let cancelProblem: string | null = null;
  let sawTurnAround = false;
  let lastProgressCancelling = false;
  let settlingMove = false;
  /** A settle was asked for while one ran: the running one loops once more with this. */
  let resettle: { problem: string | null } | null = null;
  /** Orders archive_status responses so a stale one can't overwrite newer state. */
  let archiveSeq = 0;

  const isStillLoading = (e: unknown) => /still loading/i.test(String(e));
  const joinProblems = (...ps: (string | null | undefined)[]) =>
    ps.filter(Boolean).join(" ") || null;
  /** Drop a trailing period (and spaces) so joined messages don't end up with "..". */
  const unDot = (s: string) => s.replace(/[.\s]+$/, "");
  /** End a message with exactly one period. */
  const sentence = (s: string) => unDot(s) + ".";
  /** "a, b, c +N more" */
  const capList = (xs: string[], n = 3, sep = ", ") =>
    xs.slice(0, n).join(sep) + (xs.length > n ? ` +${xs.length - n} more` : "");

  /** Re-read the archive status; keeps the last one on error ("still loading" is expected
   *  until the layout resolves at startup). False when it failed or a newer read superseded it. */
  async function refreshArchive(): Promise<boolean> {
    const seq = ++archiveSeq;
    try {
      const st = await archiveStatus();
      if (seq !== archiveSeq) return false;
      archive = st;
      archiveLoadFailed = false;
      if (st.moving?.direction === "back") sawTurnAround = true;
      if (st.moving && !st.running) pauseRequested = false;
      return true;
    } catch (e) {
      if (!isStillLoading(e)) console.warn("[Silo] archive_status:", e);
      return false;
    }
  }

  /** Startup: retry while the backend is still resolving the layout. */
  async function loadArchive() {
    for (let i = 0; i < 60; i++) {
      const seq = ++archiveSeq;
      try {
        const st = await archiveStatus();
        if (seq === archiveSeq) archive = st;
        archiveLoadFailed = false;
        return;
      } catch (e) {
        if (!isStillLoading(e)) {
          console.warn("[Silo] archive_status at startup:", e);
          break;
        }
        await new Promise((r) => setTimeout(r, 500));
      }
    }
    if (!archive) {
      console.warn("[Silo] gave up reading the library archive status");
      archiveLoadFailed = true;
    }
  }

  // The startup read gave up: try once more whenever Settings opens.
  $effect(() => {
    if (settingsOpen && archiveLoadFailed) void refreshArchive();
  });

  // While a move runs, poll so Pause → paused and the turn-around show even if an event is missed.
  $effect(() => {
    if (!moveRunning) return;
    const t = setInterval(() => void pollArchive(), 2000);
    return () => clearInterval(t);
  });

  async function pollArchive() {
    const wasRunning = !!archive?.running;
    if (!(await refreshArchive())) return;
    // A move this window isn't waiting on (e.g. after a webview reload) stopped: settle it here.
    if (wasRunning && !archive?.running && movesInFlight === 0) await settleMove(null);
  }

  /** After a move stops (finished, paused, or back home): refresh everything that depends on
   *  where the library lives, explain a lost cancel, and offer to file in loose mods. A settle
   *  asked for while one runs is folded into it (one more pass), never dropped. */
  async function settleMove(problem: string | null) {
    if (settlingMove) {
      if (problem) errorMsg = problem;
      resettle = { problem: joinProblems(resettle?.problem, problem) };
      return;
    }
    settlingMove = true;
    try {
      // Messages accumulate across passes: each pass's runScan clears errorMsg, so every pass
      // re-shows everything said so far.
      let said: { problem: string | null; lost: string | null } = { problem, lost: null };
      for (;;) {
        said = await settleOnce(said.problem, said.lost);
        if (!resettle) break;
        said = { problem: joinProblems(said.problem, resettle.problem), lost: said.lost };
        resettle = null;
      }
    } finally {
      settlingMove = false;
    }
  }

  /** One settle pass. Returns the messages it showed, for the next pass to carry. */
  async function settleOnce(
    problem: string | null,
    lost: string | null,
  ): Promise<{ problem: string | null; lost: string | null }> {
    moveProgress = null;
    pauseRequested = false;
    cancelInFlight = false;
    await refreshArchive();
    modsStatus = await modsRootStatus().catch(() => modsStatus);
    if (cancelRequested && !archive?.moving && !archive?.running) {
      const landedElsewhere = !cancelHome || archive?.path !== cancelHome;
      if (!lastProgressCancelling && !sawTurnAround && landedElsewhere) {
        // Lost: the cancel's own "nothing to cancel" error is noise — drop it.
        lost = lostCancelMessage();
      } else {
        problem = joinProblems(problem, cancelProblem);
      }
      cancelRequested = false;
      cancelHome = null;
      cancelProblem = null;
      sawTurnAround = false;
    }
    await runScan();
    // runScan clears errorMsg — show what the move said afterwards.
    if (problem || lost) errorMsg = joinProblems(problem, lost);
    // One adopt offer at a time (a second pass mustn't replace an open preview).
    if (!archive?.moving && !archive?.running && !problem && !organizePreview) await offerAdopt();
    return { problem, lost };
  }

  function lostCancelMessage(): string {
    return (
      `The move finished before it could be cancelled — your library is now in ${archive?.path ?? "its new folder"}. ` +
      (cancelHomeDefault
        ? "Use 'Use default' in Settings to move it back."
        : "Use Change… in Settings to move it again.")
    );
  }

  /** Run a move-ish invoke (pending until the move finishes or pauses), then settle. */
  async function driveMove(call: () => Promise<unknown>, isCancel = false) {
    movesInFlight++;
    let problem: string | null = null;
    try {
      await call();
    } catch (e) {
      // A cancel of a running move can race its finish; hold that error until the settle
      // knows whether the cancel was lost.
      if (isCancel && cancelRequested) cancelProblem = String(e);
      else problem = String(e);
      // A rejected cancel didn't turn anything around.
      if (isCancel) cancelInFlight = false;
    } finally {
      movesInFlight--;
    }
    if (movesInFlight > 0) {
      if (problem) errorMsg = problem;
      return;
    }
    const fresh = await refreshArchive();
    if (fresh && archive?.running) {
      // Not this window's move to settle (e.g. after a webview reload) — pollArchive will.
      if (problem) errorMsg = problem;
      return;
    }
    await settleMove(problem);
  }

  /** Fresh move: forget the previous one's progress and cancel bookkeeping. */
  function startTracking() {
    moveProgress = null;
    lastProgressCancelling = false;
    pauseRequested = false;
    cancelInFlight = false;
    cancelRequested = false;
    cancelHome = null;
    cancelProblem = null;
    sawTurnAround = false;
  }

  /** A custom archive whose folder is missing (drive unplugged): nothing can be moved out of
   *  it, so Change… / Use default re-point Silo instead. */
  const archiveMissing = $derived(!!archive?.path && !archive.available && !archive.isDefault);

  /** Point Silo at another archive folder (or the default) without moving anything, then
   *  rescan so the library reconciles against it. */
  async function repointArchive(toDefault: boolean) {
    const old = archive?.path ?? "its old folder";
    let path: string | null = null;
    if (!toDefault) {
      path = await pickFolder();
      if (!path) return;
    }
    const msg = toDefault
      ? `Your library archive at ${old} isn't available. Start fresh with the default archive inside your mods folder?\n\n` +
        "Nothing is moved or copied. Mods still on the old drive stay there, untouched — if you restored your archive somewhere, use Choose another and pick that folder instead."
      : `Your library archive at ${old} isn't available. Use ${path} as your library archive instead?\n\n` +
        "Nothing is moved or copied — use this if you moved or restored your archive there yourself. Mods still on the old drive stay there, untouched.";
    const ok = await confirmDialog(msg, {
      okLabel: toDefault ? "Use default" : "Use this folder",
      cancelLabel: "Cancel",
    });
    if (!ok) return;
    try {
      busy = "Pointing Silo at the archive…";
      try {
        await archiveRepoint(path);
      } finally {
        busy = null;
      }
      settingsOpen = false;
      await refreshArchive();
      modsStatus = await modsRootStatus().catch(() => modsStatus);
      await runScan();
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function changeArchive() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    if (archiveMissing) {
      await repointArchive(false);
      return;
    }
    const path = await pickFolder();
    if (!path) return;
    try {
      busy = "Checking the folder…";
      let pf: ArchivePreflight;
      try {
        pf = await archiveMovePreflight(path);
      } finally {
        busy = null;
      }
      if (pf.problems.length) {
        errorMsg = pf.problems.join(" ");
        return;
      }
      let msg =
        `Move your library (${pf.modCount} mods, ${fmtSize(pf.bytes)}) to ${path}?\n\n` +
        "Active mods stay playable — Silo links them into your mods folder" +
        (pf.sameDrive ? "." : " (or copies them if links aren't available).");
      if (!pf.sameDrive && pf.freeBytes != null)
        msg += `\n\n${fmtSize(pf.freeBytes)} free on that drive.`;
      if (!pf.sameDrive && !pf.symlinksAvailable)
        msg += "\n\nTurn on Windows Developer Mode to link active mods instead of copying them.";
      if (pf.looseMods > 0)
        msg += `\n\nThat folder already holds ${pf.looseMods} loose mod(s) — Silo will offer to file them in afterwards.`;
      msg += "\n\nClose Farming Simulator 25 first. You can pause the move to play and resume later.";
      if (!(await confirmDialog(msg, { okLabel: "Move library", cancelLabel: "Cancel" }))) return;
      settingsOpen = false;
      startTracking();
      await driveMove(() => archiveMoveStart(path));
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function archiveDefault() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    if (archiveMissing) {
      await repointArchive(true);
      return;
    }
    if (
      !(await confirmDialog("Move your library back into your mods folder?", {
        okLabel: "Move library",
        cancelLabel: "Cancel",
      }))
    )
      return;
    settingsOpen = false;
    startTracking();
    await driveMove(() => archiveUseDefault());
  }

  async function pauseMove() {
    pauseRequested = true;
    try {
      await archiveMovePause();
    } catch (e) {
      pauseRequested = false;
      errorMsg = String(e);
    }
    void refreshArchive();
  }

  async function resumeMove() {
    startTracking();
    await driveMove(() => archiveMoveResume());
  }

  async function cancelMove() {
    // Captured before the dialog: the move may finish while it's open.
    const home = archive?.path ?? null;
    const homeDefault = !!archive?.isDefault;
    if (
      !(await confirmDialog("Cancel the move? Mods already moved go back to where they were.", {
        okLabel: "Cancel move",
        cancelLabel: "Keep going",
      }))
    )
      return;
    if (!moveUnfinished) {
      // It finished while the dialog was open — nothing left to cancel.
      if (archive?.path && archive.path !== home) {
        cancelHomeDefault = homeDefault;
        errorMsg = lostCancelMessage();
      }
      return;
    }
    // Only a running move can finish before it sees the cancel; a paused one is run home here.
    if (moveRunning) {
      cancelRequested = true;
      cancelHome = home;
      cancelHomeDefault = homeDefault;
    }
    cancelInFlight = true;
    // The backend's cancel replaces a pending pause — don't leave "Pausing…" up for the trip home.
    pauseRequested = false;
    // A running move is asked to turn around (resolves at once; the original invoke settles
    // when it lands); a paused one stays pending until it's back home.
    await driveMove(() => archiveMoveCancel(), true);
    void refreshArchive();
  }

  /** After a move, if the archive folder holds loose mods, offer to file them in. */
  async function offerAdopt() {
    const pv = await archiveAdoptPreview().catch(() => null);
    if (!pv) return;
    const dupNote = pv.duplicates.length
      ? `${pv.duplicates.length} mod(s) in that folder are already in your library and were left untouched: ${capList(pv.duplicates)}.`
      : null;
    if (pv.inputs.length === 0) {
      if (dupNote) infoMsg = dupNote;
      return;
    }
    organizePreview = {
      inputs: pv.inputs,
      techNames: pv.inputs.map((m) => m.techName),
      plan: pv.plan,
      loading: false,
      applying: false,
      adopt: { path: archive?.path ?? null, note: dupNote },
    };
  }

  function dismissOverrideNotice() {
    overrideNoticeOpen = false;
    localStorage.setItem("silo.overrideNoticeShown", "1");
  }

  async function runScan(auto = true) {
    if (scanning) return;
    scanning = true;
    errorMsg = null;
    progress = { done: 0, total: 0 };
    try {
      const r = await scanMods(roots.length ? roots : undefined);
      result = r;
      mods = r.mods;
      activeSet = new Set(r.mods.filter((m) => m.active).map((m) => m.techName));
      // Filesystem-level check (Rust): files at a managed name that aren't Silo's projection.
      detectForeignFiles(roots[0])
        .then((f) => (foreignFiles = f))
        .catch(() => (foreignFiles = []));
      // Catalog facet tags for the library (silo-api#9) — powers the facet dropdowns.
      // Fire-and-forget; the bar just shows no facets until it lands.
      void loadCatalogTags(r.mods.map((m) => m.techName));
      // NOTE: the update check is NOT auto-run on launch — on a large library the burst of
      // scan + catalog-tags + a whole-library update pass all landing at once could jank the
      // window right after it opened. The "Needs update" toggle runs it on demand instead.
      // Re-read savegames too, so a Rescan picks up a save created since launch (they're
      // otherwise only loaded once on startup). Fire-and-forget; it handles its own errors.
      void loadSavegames();
    } catch (e) {
      errorMsg = String(e);
    } finally {
      scanning = false;
      lastScanAt = Date.now();
    }
    // Auto-file any mods still loose in the flat root (e.g. freshly downloaded),
    // keeping them active so filing is transparent to the game.
    // Never auto-file mid-move: library writes are refused until the move finishes.
    if (auto && autoFileNew && !moveUnfinished) {
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
    // Suppress the WebView's built-in right-click menu (reload/inspect junk) app-wide,
    // but keep the native cut/copy/paste menu inside editable fields. Our own row menu
    // (ModRow → ContextMenu) still opens; it sets its own handler on the row.
    const onCtx = (e: MouseEvent) => {
      const t = e.target as HTMLElement | null;
      if (t && (t.isContentEditable || t.closest("input, textarea, select"))) return;
      e.preventDefault();
    };
    window.addEventListener("contextmenu", onCtx);
    (async () => {
      unlisten = await onScanProgress((p) => (progress = p));
      moveUnlisten = await onMoveProgress((p) => {
        moveProgress = p;
        lastProgressCancelling = p.cancelling;
      });
      try {
        const rows = await getCuration();
        curationMap = Object.fromEntries(rows.map((r) => [r.techName, r]));
        const ovs = await getOverrides();
        overrideMap = Object.fromEntries(
          ovs.map((o) => [o.techName, { category: o.category, subcategory: o.subcategory }]),
        );
        await loadTags();
        await loadRepos();
        await loadLoadouts();
        await loadSavegames();
        gameInfo = await detectGame();
        userDir = await userDirPath();
        appVer = await appVersion();
        // Fire-and-forget: never let a slow/absent update endpoint delay startup.
        checkUpdate().then((u) => (update = u));
        settingsModsSet = new Set(await modsWithSettings());
      } catch (e) {
        errorMsg = String(e);
      }
      try {
        modsStatus = await modsRootStatus();
        roots = modsStatus.available && modsStatus.path ? [modsStatus.path] : [];
        if (
          modsStatus.source === "gameOverride" &&
          localStorage.getItem("silo.overrideNoticeShown") !== "1"
        ) {
          overrideNoticeOpen = true;
        }
      } catch (e) {
        errorMsg = String(e);
      }
      // Where the library archive lives, and any unfinished move (paused before a restart).
      // Awaited so the first scan's auto-file guard sees a move left paused before a restart.
      await loadArchive();
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
      // Throttled (skip if busy/scanning or scanned <4s ago) so alt-tabbing doesn't thrash; the
      // scan cache (path+mtime+size) makes an unchanged rescan cheap anyway.
      try {
        focusUnlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (
            focused &&
            !scanning &&
            !busy &&
            !moveRunning &&
            roots.length &&
            Date.now() - lastScanAt > 4000
          ) {
            void runScan(true);
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
      window.removeEventListener("contextmenu", onCtx);
    };
  });

  async function restoreBisect() {
    if (!bisectRecovery) return;
    busy = "Restoring your mod set…";
    try {
      await applyActive(bisectRecovery);
      await bisectSnapshotClear();
      bisectRecovery = null;
      await runScan(false);
    } catch (e) {
      errorMsg = String(e);
    }
    busy = null;
  }
</script>

<div class="app" style="--topbar-h:{topbarH}px">
  <Topbar
    bind:topbarH
    {view}
    {appVer}
    {update}
    {updating}
    {updatePct}
    onApplyUpdate={applyUpdate}
    hasSavegames={savegames.length > 0}
    {savesOpen}
    {loadoutsOpen}
    activeLoadoutName={activeLoadoutId !== null
      ? loadouts.find((l) => l.id === activeLoadoutId)?.name ?? null
      : null}
    {mpOpen}
    {collectionsOpen}
    showOrganize={unorganizedCount > 0 && !autoFileNew}
    {unorganizedCount}
    hasMods={mods.length > 0}
    {updateChecking}
    {scanning}
    {busy}
    launchBlocked={moveRunning}
    organizeBlocked={moveUnfinished}
    hasGame={!!gameInfo}
    activeCount={activeSet.size}
    {settingsOpen}
    onSwitchView={switchView}
    onToggleSaves={() => (savesOpen = !savesOpen)}
    onToggleLoadouts={() => (loadoutsOpen = !loadoutsOpen)}
    onToggleMp={() => (mpOpen = !mpOpen)}
    onToggleCollections={() => (collectionsOpen = !collectionsOpen)}
    onOrganize={organizeNew}
    onCheckUpdates={checkAllUpdates}
    onRescan={() => runScan()}
    onLaunch={launch}
    onToggleSettings={() => (settingsOpen = !settingsOpen)}
  />

  {#if bisectRecovery}
    <div class="recover-banner">
      <span>
        A guided diagnosis was interrupted — your active mods may be partially applied.
      </span>
      <div class="recover-actions">
        <button class="btn primary" onclick={restoreBisect} disabled={!!busy}>
          Restore my mod set
        </button>
        <button class="btn" onclick={() => (bisectRecovery = null)} disabled={!!busy}>
          Later
        </button>
      </div>
    </div>
  {/if}

  {#if loadoutsOpen}
    <LoadoutsPanel
      {loadouts}
      activeCount={activeSet.size}
      {activeLoadoutId}
      onApply={applyLoadout}
      onOverwrite={overwriteLoadout}
      onExport={exportLoadout}
      onRemove={removeLoadout}
      onSaveCurrent={saveCurrentLoadout}
      onImport={importLoadout}
      onClose={() => (loadoutsOpen = false)}
    />
  {/if}

  {#if savesOpen}
    <SavegamesPanel
      rows={saveRows}
      {busy}
      onConfigure={openSaveConfig}
      onBackup={backupSave}
      onLoadout={loadoutFromSave}
      onClose={() => (savesOpen = false)}
    />
  {/if}

  {#if conflictsOpen}
    <ConflictsPanel
      {conflicts}
      {criticalCount}
      activeCount={activeSet.size}
      onClose={() => (conflictsOpen = false)}
    />
  {/if}

  {#if settingsOpen}
    <SettingsPanel
      {theme}
      {roots}
      {gameInfo}
      {userDir}
      {appVer}
      {autoFileNew}
      {organizedCount}
      {unorganizedCount}
      {cloudSynced}
      {busy}
      {scanning}
      onSetTheme={setTheme}
      onOpenGameGraphics={openGameGraphics}
      onExportReport={exportReport}
      onSetAutoFile={setAutoFile}
      onOrganize={() => {
        settingsOpen = false;
        organizeNew();
      }}
      onRebuild={rebuildLibrary}
      onRestoreVanilla={() => {
        settingsOpen = false;
        restoreVanilla();
      }}
      onOptimizeLinks={optimizeLinks}
      onClose={() => (settingsOpen = false)}
      {modsStatus}
      onChangeModsFolder={changeModsFolder}
      onUseGameFolder={useGameFolder}
      onSwitchModsFolder={switchModsFolder}
      {archive}
      {moveUnfinished}
      {archiveLoadFailed}
      onChangeArchive={changeArchive}
      onArchiveDefault={archiveDefault}
    />
  {/if}

  {#if settingsMod}
    <ModSettings
      modName={settingsMod.techName}
      title={settingsMod.title}
      onClose={() => (settingsMod = null)}
    />
  {/if}

  {#if configEditor}
    <ConfigEditor
      title={configEditor.title}
      path={configEditor.path}
      fields={configEditor.fields}
      presets={configEditor.presets}
      footnote={configEditor.footnote}
      onClose={() => (configEditor = null)}
    />
  {/if}

  {#if detailMod}
    {@const dm = detailMod}
    {@const fgn = foreignFiles.find((f) => f.techName === dm.techName) ?? null}
    <ModDetail
      mod={dm}
      curation={cur(dm.techName)}
      tags={tagsOf(dm.techName)}
      active={activeSet.has(dm.techName)}
      organized={dm.organized}
      hasSettings={settingsModsSet.has(dm.techName)}
      foreign={fgn}
      onAdoptForeign={async () => {
        if (fgn) {
          await resolveForeign(fgn.fileName, "adopt");
          detailMod = null;
        }
      }}
      onRestoreForeign={async () => {
        if (fgn) {
          await resolveForeign(fgn.fileName, "restore");
          detailMod = null;
        }
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
      onInstalled={() => {
        detailMod = null;
        runScan(false);
      }}
    />
  {/if}

  {#if updatesOpen}
    <UpdatesPanel
      {updateResults}
      {availableUpdates}
      {updateChecking}
      {busy}
      onInstall={installFromRow}
      onClose={() => (updatesOpen = false)}
    />
  {/if}

  {#if statsOpen}
    <StatsPanel
      modCount={mods.length}
      {organizedCount}
      activeCount={activeSet.size}
      mapsCount={stats.maps}
      {libStats}
      onClose={() => (statsOpen = false)}
    />
  {/if}

  {#if healthOpen}
    <HealthPanel
      {health}
      {healthCount}
      onResolveForeign={resolveForeign}
      onClose={() => (healthOpen = false)}
    />
  {/if}

  {#if logOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (logOpen = false)}></div>
    <div class="log-panel">
      <LogTriage
        onClose={() => (logOpen = false)}
        activeMods={[...activeSet]}
        activeMaps={mods.filter((m) => activeSet.has(m.techName) && m.isMap).map((m) => m.techName)}
      />
    </div>
  {/if}

  {#if bindingsOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (bindingsOpen = false)}></div>
    <div class="log-panel">
      <BindingsView onClose={() => (bindingsOpen = false)} />
    </div>
  {/if}

  {#if mpOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (mpOpen = false)}></div>
    <div class="log-panel">
      <MpSync active={activeModRefs} onClose={() => (mpOpen = false)} />
    </div>
  {/if}

  {#if collectionsOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (collectionsOpen = false)}></div>
    <div class="log-panel">
      <Collections
        active={activeModRefs}
        library={mods}
        initialImportUrl={collImportUrl}
        onImported={() => runScan(false)}
        onClose={() => {
          collectionsOpen = false;
          collImportUrl = null;
        }}
      />
    </div>
  {/if}

  {#if bridgeOpen}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="backdrop" onclick={() => (bridgeOpen = false)}></div>
    <div class="log-panel">
      <BridgeTool
        onGenerated={() => runScan(true)}
        onClose={() => (bridgeOpen = false)}
      />
    </div>
  {/if}

  {#if scanning}
    <div class="progress">
      <div class="bar" style="width: {pct}%"></div>
      <span class="progress-text tnum">{progress.done} / {progress.total}</span>
    </div>
  {/if}

  {#if organizePreview}
    <OrganizePreview
      plan={organizePreview.plan}
      loading={organizePreview.loading}
      applying={organizePreview.applying}
      adopt={organizePreview.adopt ?? null}
      onConfirm={confirmOrganize}
      onCancel={() => (organizePreview = null)}
    />
  {/if}

  {#if busy}
    <div class="busy">{busy}</div>
  {/if}

  {#if errorMsg}
    <div class="error">{errorMsg}</div>
  {/if}

  {#if infoMsg}
    <div class="cloud-notice">
      <span class="cn-ico">ⓘ</span>
      <div class="cn-text">{infoMsg}</div>
      <button class="cn-x" onclick={() => (infoMsg = null)} title="Dismiss">✕</button>
    </div>
  {/if}

  {#if cloudNoticeOpen}
    <div class="cloud-notice">
      <span class="cn-ico">☁</span>
      <div class="cn-text">
        <b>Your mods folder is synced by a cloud service</b> (OneDrive, Google Drive, or Proton
        Drive), which doesn't support hardlinks. Silo will project your active mods as
        <b>file copies</b> instead — this works fine, but uses more disk and makes bulk activate /
        deactivate slower. For the best performance, exclude
        <span class="tnum">…\My Games\FarmingSimulator2025</span> from your cloud sync.
      </div>
      <button class="cn-x" onclick={dismissCloudNotice} title="Dismiss">✕</button>
    </div>
  {/if}

  {#if modsStatus && modsStatus.source === "user" && !modsStatus.available}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>Your mods folder isn't available:</b> <span class="tnum">{modsStatus.path}</span>.
        Nothing was scanned. Reconnect the drive, or pick another folder.
      </div>
      <button class="cn-act" onclick={retryModsFolder}>Retry</button>
      <button class="cn-act" onclick={changeModsFolder}>Choose another</button>
      <button class="cn-act" onclick={useGameFolder}>Use the game's folder</button>
    </div>
  {:else if modsStatus?.modsFolderMissing}
    <div class="cloud-notice">
      <span class="cn-ico">ⓘ</span>
      <div class="cn-text">Farming Simulator hasn't created its <b>mods</b> folder yet.</div>
      <button class="cn-act" onclick={makeModsFolder}>Create it</button>
      <button class="cn-act" onclick={changeModsFolder}>Choose a folder</button>
    </div>
  {:else if modsStatus?.mismatch}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>FS25 is reading a different mods folder</b>
        {#if modsStatus.gameReads}(<span class="tnum">{modsStatus.gameReads}</span>){/if} — mods
        you activate here won't load in-game.
      </div>
      {#if modsStatus.canWriteGameSetting}
        <button class="cn-act" onclick={pointGameHere}>Point FS25 here</button>
      {/if}
      <button class="cn-act" onclick={useGameFolder}>Switch Silo to the game's folder</button>
    </div>
  {/if}

  {#if moveUnfinished}
    <div class="cloud-notice">
      <span class="cn-ico">⇄</span>
      <div class="cn-text">
        {#if moveRunning}
          <b>{moveCancelling ? "Moving your library back" : "Moving your library"}</b>
          {#if archive?.moving}
            to <span class="tnum">{moveCancelling ? archive.moving.from : archive.moving.to}</span>
          {/if}
          —
          {#if moveProgress && moveProgress.total > 0}
            <span class="tnum">
              {moveProgress.done}/{moveProgress.total} mods · {fmtSize(moveProgress.bytesDone)} of
              {fmtSize(moveProgress.bytesTotal)}</span
            >.
          {:else}
            starting.
          {/if}
          Launch is blocked until it pauses or finishes.
        {:else if archive?.moving?.errors.length}
          <b>{archive.moving.errors.length} mod(s) couldn't be moved:</b>
          {sentence(capList(archive.moving.errors.map(unDot), 3, "; "))}
          {archive.moving.pauseReason ? `${sentence(archive.moving.pauseReason)} ` : ""}The move is
          paused — Retry tries them again{moveCancelling ? "." : ", or Cancel moves everything back."}
        {:else if archive?.moving}
          <b>{moveCancelling ? "Moving your library back is paused." : "Archive move paused."}</b>
          {archive.moving.pauseReason ? `${sentence(archive.moving.pauseReason)} ` : ""}You can play —
          resume when you're done.
        {/if}
      </div>
      {#if moveRunning}
        <button class="cn-act" onclick={pauseMove} disabled={pauseRequested}>
          {pauseRequested ? "Pausing…" : "Pause"}
        </button>
      {:else if archive?.moving}
        <button class="cn-act" onclick={resumeMove}>
          {archive.moving.errors.length ? "Retry" : "Resume"}
        </button>
      {/if}
      {#if !moveCancelling}
        <button class="cn-act" onclick={cancelMove}>Cancel move</button>
      {/if}
    </div>
  {:else if archive?.path && !archive.available}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>Your library archive isn't available:</b> <span class="tnum">{archive.path}</span>. Parked
        mods are hidden until it's back; active mods linked from it won't load in-game.
      </div>
      <button
        class="cn-act"
        onclick={async () => {
          await refreshArchive();
          await runScan();
        }}>Retry</button
      >
      <button class="cn-act" onclick={changeArchive}>Choose another</button>
    </div>
  {/if}

  {#if overrideNoticeOpen && modsStatus?.path}
    <div class="cloud-notice">
      <span class="cn-ico">ⓘ</span>
      <div class="cn-text">
        Silo now uses the mods folder FS25 is set to: <span class="tnum">{modsStatus.path}</span>.
      </div>
      <button class="cn-x" onclick={dismissOverrideNotice} title="Dismiss">✕</button>
    </div>
  {/if}

  {#if view === "browse"}
    <!-- Own scroll region: the library scrolls via its inner VirtualList, but Browse
         is plain flow content, so it needs a flex-fill scroll container of its own. -->
    <div class="browse-scroll">
      <ModBrowser
        installed={libraryTechNames}
        onInstalled={() => runScan(true)}
        onNeedAuth={() => (settingsOpen = true)}
        onFindInLibrary={openInLibrary}
        seed={browseSeed}
        seedTech={browseSeedTech}
      />
    </div>
  {:else}
  <div class="lib">
    <div class="lib-inner">
      <FilterBar
        catTree={libCatTree}
        bind:parentCategory={libCat}
        bind:childCategory={libSub}
        bind:query
        searchPlaceholder="Filter by title, author, or tech name…"
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
        facetOrder={LIB_FACET_ORDER}
        facetData={libFacets}
        facetLabels={LIB_FACET_LABEL}
        bind:activeTags={libActiveTags}
        showYear={false}
        showClear={false}
        filterCount={libFilterCount}
        onClear={clearLibFilters}
      />

      <!-- Library-only filters (they have no Browse equivalent). -->
      <div class="lib-toggles">
        <button class="lib-tog" class:on={favoritesOnly} onclick={() => (favoritesOnly = !favoritesOnly)}>
          {favoritesOnly ? "★" : "☆"} Favorites
        </button>
        <button class="lib-tog" class:on={showHidden} onclick={() => (showHidden = !showHidden)}>
          Hidden
        </button>
        <button class="lib-tog" class:on={flaggedOnly} onclick={() => (flaggedOnly = !flaggedOnly)}>
          ⚑ Flagged
        </button>
        <button
          class="lib-tog"
          class:on={conflictedOnly}
          disabled={conflictedSet.size === 0}
          onclick={() => (conflictedOnly = !conflictedOnly)}
        >
          ⚠ In conflict{conflictedSet.size > 0 ? ` (${conflictedSet.size})` : ""}
        </button>
        <button class="lib-tog" class:on={hasSettingsOnly} onclick={() => (hasSettingsOnly = !hasSettingsOnly)}>
          ⚙ Has settings
        </button>
        <button
          class="lib-tog lib-upd"
          class:on={needsUpdateOnly}
          disabled={updateChecking}
          title="Show only mods with a newer version available — checks for updates the first time"
          onclick={toggleNeedsUpdate}
        >
          {updateChecking ? "⬆ Checking…" : `⬆ Needs update${availableUpdates.length > 0 ? ` (${availableUpdates.length})` : ""}`}
        </button>
        {#if libFilterCount > 0}
          <button class="lib-clear" onclick={clearLibFilters}>Clear all ✕</button>
        {/if}
      </div>

      <LibraryToolbar
        {selectedTag}
        shownCount={filtered.length}
        allActive={allFilteredActive}
        activeInFilter={filteredActiveCount}
        disabled={!!busy || filtered.length === 0}
        bind:libView
        onSelectAll={setActiveForFiltered}
        onClearTag={() => (selectedTag = null)}
      />

      <div class="list-wrap">
        <div class="list-body">
          {#if filtered.length === 0 && !scanning}
            <div class="empty">
              {mods.length === 0
                ? "No mods found yet. Point Silo at your mods folder and rescan."
                : "No mods match your filter."}
            </div>
          {:else if libView === "tiles"}
            <VirtualList items={visiblePairs} rowHeight={TILE_ROW_H} bottomPad={62}>
              {#snippet row(pair)}
                <div class="tile-pair">
                  {#each pair as mod (mod.techName)}
                    <ModTile
                      {mod}
                      curation={cur(mod.techName)}
                      overridden={!!overrideMap[mod.techName]}
                      organized={mod.organized}
                      active={activeSet.has(mod.techName)}
                      hasSettings={settingsModsSet.has(mod.techName)}
                      hasUpdate={updatesSet.has(mod.techName)}
                      tags={tagsOf(mod.techName)}
                      onToggle={(flag) => toggleCuration(mod.techName, flag)}
                      onToggleActive={() => toggleActive(mod.techName)}
                      onEditCategory={(ev) => openEditor(mod.techName, ev)}
                      onOpenSettings={() =>
                        (settingsMod = { techName: mod.techName, title: mod.title ?? mod.techName })}
                      onOpenDetail={() => (detailMod = mod)}
                      onFindInBrowse={() => openInBrowse(mod)}
                      onContextMenu={(ev) => {
                        ev.preventDefault();
                        rowMenu = { mod, ev };
                      }}
                    />
                  {/each}
                </div>
              {/snippet}
            </VirtualList>
          {:else}
            <VirtualList items={visible} rowHeight={76} bottomPad={62}>
              {#snippet row(mod)}
                <ModRow
                  {mod}
                  curation={cur(mod.techName)}
                  overridden={!!overrideMap[mod.techName]}
                  organized={mod.organized}
                  active={activeSet.has(mod.techName)}
                  hasSettings={settingsModsSet.has(mod.techName)}
                  tags={tagsOf(mod.techName)}
                  onToggle={(flag) => toggleCuration(mod.techName, flag)}
                  onToggleActive={() => toggleActive(mod.techName)}
                  onEditCategory={(ev) => openEditor(mod.techName, ev)}
                  onOpenSettings={() =>
                    (settingsMod = { techName: mod.techName, title: mod.title ?? mod.techName })}
                  onOpenDetail={() => (detailMod = mod)}
                  onContextMenu={(ev) => {
                    ev.preventDefault();
                    rowMenu = { mod, ev };
                  }}
                />
              {/snippet}
            </VirtualList>
          {/if}
        </div>

        <!-- Stats + tools as a content-width footer that floats over the scrolling list. -->
        <div class="statbar-footer">
          <StatBar
            modCount={mods.length}
            mapsCount={stats.maps}
            scriptsCount={stats.scripts}
            uniqueCount={stats.unique}
            activeCount={activeSet.size}
            conflictCount={conflicts.length}
            {criticalCount}
            {healthCount}
            tookMs={result ? result.tookMs : null}
            bind:statFilter
            onOpenStats={() => (statsOpen = !statsOpen)}
            onOpenConflicts={() => (conflictsOpen = !conflictsOpen)}
            onOpenHealth={() => (healthOpen = !healthOpen)}
            onOpenLog={() => (logOpen = true)}
            onOpenBindings={() => (bindingsOpen = true)}
            onOpenBridge={() => (bridgeOpen = true)}
          />
        </div>
      </div>
    </div>
  </div>
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

  {#if rowMenu}
    <ContextMenu
      x={rowMenu.ev.clientX}
      y={rowMenu.ev.clientY}
      items={rowMenuItems(rowMenu.mod, rowMenu.ev)}
      onClose={() => (rowMenu = null)}
    />
  {/if}
</div>

<style>
  .app {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .btn {
    border: 1px solid var(--border);
    background: var(--surface-raised);
    color: var(--text);
    padding: 9px 18px;
    border-radius: var(--radius);
    font-weight: 600;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .btn.primary {
    background: var(--primary);
    border-color: var(--primary);
    color: var(--on-primary);
  }
  .btn.primary:hover:not(:disabled) {
    background: var(--primary-hover);
  }
  .btn:hover:not(:disabled):not(.primary) {
    color: var(--text);
    border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
  }
  .btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .busy {
    padding: 8px 20px;
    background: color-mix(in srgb, var(--accent) 14%, var(--surface));
    color: var(--gold-700);
    font-size: 13px;
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
    font-size: 11px;
    color: var(--text-muted);
  }
  .error {
    padding: 10px 20px;
    background: color-mix(in srgb, var(--danger) 12%, var(--surface));
    color: var(--danger);
    font-size: 13px;
  }
  .cloud-notice {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 11px 16px 11px 20px;
    background: color-mix(in srgb, var(--info) 12%, var(--surface));
    border-bottom: 1px solid color-mix(in srgb, var(--info) 30%, var(--border));
    font-size: 12.5px;
    line-height: 1.5;
    color: var(--text);
  }
  .cn-ico {
    font-size: 16px;
    line-height: 1.4;
    color: var(--info);
    flex: 0 0 auto;
  }
  .cn-text {
    flex: 1 1 auto;
  }
  .cn-act {
    flex: 0 0 auto;
    border: 1px solid color-mix(in srgb, var(--info) 40%, transparent);
    background: transparent;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
  }
  .cn-act:hover {
    background: color-mix(in srgb, var(--info) 15%, transparent);
  }
  .cn-x {
    flex: 0 0 auto;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font-size: 13px;
    cursor: pointer;
    padding: 2px 6px;
    border-radius: var(--radius-sm);
  }
  .cn-x:hover {
    background: color-mix(in srgb, var(--info) 15%, transparent);
    color: var(--text);
  }
  /* Log-triage panel owns its own padding (LogTriage.svelte), so no inner padding here. */
  .recover-banner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin: 0 0 10px;
    padding: 10px 14px;
    background: color-mix(in srgb, var(--warn) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--warn) 40%, transparent);
    border-radius: var(--radius-sm);
    color: var(--text);
    font-size: 0.88rem;
  }
  .recover-actions {
    display: flex;
    gap: 8px;
  }
  .log-panel {
    position: fixed;
    z-index: 50;
    top: 90px;
    left: 50%;
    transform: translateX(-50%);
    width: 620px;
    max-width: calc(100vw - 40px);
    max-height: 80vh;
    overflow-y: auto;
    background: var(--surface);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    box-shadow: var(--shadow-2);
    scrollbar-width: thin;
  }
  /* ── Library view: content-width column, a floating footer stat bar over the list ── */
  .lib {
    flex: 1 1 auto;
    min-height: 0;
    display: flex;
    /* Reserve room for an open detail drawer so content tucks left of it. */
    padding-right: var(--drawer-w, 0px);
  }
  .lib-inner {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 0;
    width: 100%;
    max-width: 1200px;
    margin: 0 auto;
    padding: 0 20px;
    display: flex;
    flex-direction: column;
  }
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
    font-size: 12.5px;
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
    color: var(--accent);
  }
  .lib-upd.on {
    background: color-mix(in srgb, var(--gold-700) 15%, transparent);
    border-color: color-mix(in srgb, var(--gold-700) 45%, var(--border));
    color: var(--gold-700);
  }
  .lib-clear {
    margin-left: auto;
    border: none;
    background: transparent;
    color: var(--text-muted);
    font: inherit;
    font-size: 0.8rem;
    font-weight: 600;
    cursor: pointer;
    padding: 6px;
  }
  .lib-clear:hover {
    color: var(--primary);
    text-decoration: underline;
  }
  .list-wrap {
    position: relative;
    flex: 1 1 auto;
    min-height: 0;
  }
  .tile-pair {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
    height: 100%;
    padding-bottom: 16px;
  }
  @media (max-width: 720px) {
    .tile-pair {
      grid-template-columns: 1fr;
    }
  }
  .statbar-footer {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 5;
    border-radius: var(--radius) var(--radius) 0 0;
    overflow: hidden;
    box-shadow: 0 -6px 20px rgba(0, 0, 0, 0.14);
  }
  .browse-scroll {
    flex: 1 1 auto;
    min-height: 0;
    overflow-y: auto;
    /* Reserve room for an open detail drawer (Browse content is centered, so this
       shifts it left of the drawer instead of hiding under it). */
    padding-right: var(--drawer-w, 0px);
  }
  .list-body {
    height: 100%;
    min-height: 0;
  }
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
  }
  .empty {
    display: grid;
    place-items: center;
    height: 100%;
    color: var(--text-muted);
    font-size: 14px;
    padding: 40px;
    text-align: center;
  }
</style>
