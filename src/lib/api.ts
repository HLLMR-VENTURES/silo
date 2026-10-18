// Typed wrappers over the Tauri command surface. The frontend never touches a
// filesystem or zip — it asks the Rust core and listens for progress events.

import { invoke as rawInvoke, type InvokeArgs } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { save, open, confirm as pluginConfirm } from "@tauri-apps/plugin-dialog";
import { revealItemInDir, openUrl } from "@tauri-apps/plugin-opener";
import { toAppError } from "./errors";
import { serial } from "./serial";
export { serial } from "./serial";
import type {
  ScanResult,
  UpdateOutcome,
  ScanProgress,
  CurationRow,
  CategoryOverride,
  TagRow,
  ModInput,
  OrganizeReport,
  PlannedMove,
  VerifyResult,
  Loadout,
  Savegame,
  Conflict,
  ConflictInput,
  GameInfo,
  SettingsFile,
  SettingsEdit,
  RepoRow,
  SourceRow,
  UpdateInfo,
  GhStatus,
  RepoStats,
  DeviceCode,
  PollResult,
  BrowseMod,
  ModTag,
  BrowsePage,
  SiloStats,
  InstallProgress,
  CatalogUpdate,
  CatalogModDetail,
  CategoryCount,
  LogReport,
  BisectStep,
  BindingReport,
  MpModRef,
  MpVerifyReport,
  CollectionExportResult,
  ImportPlan,
  ApplyReport,
  CollectionProgress,
  BridgeSpec,
} from "./types";

/** Every command goes through here so a rejection is always an AppError (see errors.ts). */
async function invoke<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  try {
    return await rawInvoke<T>(cmd, args);
  } catch (e) {
    throw toAppError(e);
  }
}

/**
 * Ask the user to confirm with the OS dialog. Views use `ask()` from `$lib/ui` (the in-app
 * dialog) instead; this stays for the quit guard, which must work whatever state the window is
 * in, and as the fallback `askOrOs()` (src/lib/confirm.ts) uses before the dialog host mounts.
 * ALWAYS `await` this. tauri-plugin-dialog replaces the webview's
 * `window.confirm` with an async function, so calling it bare returns a Promise that is
 * always truthy and the action runs no matter what the user clicks (#177). Never call the
 * global confirm directly; `scripts/check-confirm.mjs` fails the build if one appears.
 */
export async function confirmDialog(
  message: string,
  opts: { okLabel?: string; cancelLabel?: string; kind?: "info" | "warning" | "error" } = {},
): Promise<boolean> {
  return await pluginConfirm(message, { title: "Silo", kind: opts.kind ?? "warning", ...opts });
}

export type ModsRootSource = "user" | "gameOverride" | "default";

export interface KnownRoot {
  path: string;
  modCount: number;
  current: boolean;
}

/** The live mods folder (#162): where it came from, whether FS25 reads it, what else is known. */
export interface ModsRootStatus {
  path: string | null;
  source: ModsRootSource;
  gameReads: string | null;
  mismatch: boolean;
  available: boolean;
  modsFolderMissing: boolean;
  known: KnownRoot[];
  canWriteGameSetting: boolean;
  gameSettingProblem: string | null;
  unresolvedOverride: string | null;
  /** Why Silo won't change files in this folder (it shares files with another layout's archive). */
  layoutProblem: string | null;
  /** What causes it: an overlap, only FS25's own setting, or a database read that failed. */
  layoutProblemCause: "overlap" | "gameSetting" | "readError" | null;
  /** The folder it runs into, when known. */
  layoutConflict: string | null;
}

export function modsRootStatus(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("mods_root_status");
}
export function setModsRoot(path: string): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("set_mods_root", { path });
}
export function clearModsRoot(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("clear_mods_root");
}
export function pointGameAtModsRoot(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("point_game_at_mods_root");
}
export function createModsFolder(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("create_mods_folder");
}
// ── Library archive location (#166) ──

/** An unfinished archive move. `from` is where it started (home), `to` where it's going;
 *  cancelling flips `direction` to "back". Running vs paused comes from `ArchiveStatus.running`. */
export interface MoveMarker {
  from: string;
  to: string;
  direction: "forward" | "back";
  paused: boolean;
  pauseReason: string | null;
  errors: string[];
  /** techNames placed in `to` and not yet moved back. */
  moved: string[];
  /** Category folders Silo created in `to`; a cancel removes the ones left empty. Absent on a
   * marker from before Silo recorded them. */
  created?: string[];
}

export interface ArchiveStatus {
  /** The active archive — during an unfinished move, where the move started. */
  path: string | null;
  isDefault: boolean;
  /** False when a custom archive's folder is missing (its drive unplugged). */
  available: boolean;
  moving: MoveMarker | null;
  /** A move is copying right now (Pause applies, Launch is blocked). */
  running: boolean;
}

export interface ArchivePreflight {
  modCount: number;
  bytes: number;
  freeBytes: number | null;
  sameDrive: boolean;
  symlinksAvailable: boolean;
  looseMods: number;
  problems: string[];
  /** The current archive holds no mods and the folder passes every path check, so it can be
   *  used as the archive as it is (`archiveRepoint`, moves nothing) even if a move is refused. */
  canRepoint: boolean;
}

export interface MoveProgress {
  done: number;
  total: number;
  bytesDone: number;
  bytesTotal: number;
  current: string;
  cancelling: boolean;
}

export interface AdoptPreview {
  plan: PlannedMove[];
  inputs: ModInput[];
  /** File names left untouched (techName already in the library, or repeated). */
  duplicates: string[];
}

/** Rejects with code `STILL_LOADING` until the layout is resolved at startup. */
export const archiveStatus = () => invoke<ArchiveStatus>("archive_status");
export const archiveMovePreflight = (path: string) =>
  invoke<ArchivePreflight>("archive_move_preflight", { path });
/** Stays pending for the whole move; resolves null (finished) or the marker (paused). */
export const archiveMoveStart = (path: string) =>
  invoke<MoveMarker | null>("archive_move_start", { path });
export const archiveMoveResume = () => invoke<MoveMarker | null>("archive_move_resume");
/** Just a request — the running move pauses at the next mod boundary. */
export const archiveMovePause = () => invoke<void>("archive_move_pause");
/** A running move is asked to turn around (resolves null at once); a paused one is run back
 *  home here (pending until it lands). */
export const archiveMoveCancel = () => invoke<MoveMarker | null>("archive_move_cancel");
export const archiveUseDefault = () => invoke<MoveMarker | null>("archive_use_default");
/** Point Silo at another archive folder (null = the default) WITHOUT moving anything. Only
 *  while the current archive is unavailable or holds no mods, and no move is unfinished;
 *  rescan afterwards. */
export const archiveRepoint = (path: string | null) =>
  invoke<void>("archive_repoint", { path });
export const archiveAdoptPreview = () => invoke<AdoptPreview>("archive_adopt_preview");
/** File the picked loose mods (by file name) into the archive's category folders. */
export const archiveAdoptApply = (fileNames: string[]) =>
  invoke<OrganizeReport>("archive_adopt_apply", { fileNames });
export function onMoveProgress(cb: (p: MoveProgress) => void): Promise<UnlistenFn> {
  return listen<MoveProgress>("archive-move-progress", (e) => cb(e.payload));
}

export function folderLooksEmpty(path: string): Promise<boolean> {
  return invoke<boolean>("folder_looks_empty", { path });
}
/** Ask for a folder with the picker Silo opens itself. Silo only moves the mods folder or the
 *  library archive to a folder chosen here (or one it already knows). */
export function pickFolderFor(purpose: "modsFolder" | "archive"): Promise<string | null> {
  return invoke<string | null>("pick_folder", { purpose });
}

/** App version (from tauri.conf/Cargo), e.g. "0.2.0" — the single source the UI shows. */
export function appVersion(): Promise<string> {
  return invoke<string>("app_version");
}

/** One-time notices from startup work (a database restored, an archive found again). */
export function takeStartupNotices(): Promise<string[]> {
  return invoke<string[]>("take_startup_notices");
}

/** True when the OS keychain can hold account tokens; false → they'd fall back to the DB. */
export function secretStorageSecure(): Promise<boolean> {
  return invoke<boolean>("secret_storage_secure");
}

/** A file in the flat mods root at a managed name that isn't Silo's projection. */
export interface ForeignFile {
  techName: string;
  fileName: string;
  kind: string;
  /** Version of the file in the mods folder (what "adopt" keeps). */
  flatVersion: string | null;
  /** Version of Silo's managed copy (what "restore" puts back). */
  managedVersion: string | null;
}

/** Detect files squatting at an organized mod's name that Silo didn't create. */
export function detectForeignFiles(root?: string): Promise<ForeignFile[]> {
  return invoke<ForeignFile[]>("detect_foreign_files", { root: root ?? null });
}

/** Adopt the file the user swapped into the mods folder as the mod's new canonical version
 *  (old copy kept under backups/). Rescan afterward. */
export function adoptForeignFile(fileName: string, root?: string): Promise<void> {
  return invoke("adopt_foreign_file", { fileName, root: root ?? null });
}

/** Restore Silo's managed copy over a swapped-in file (their file kept under backups/). */
export function restoreForeignFile(fileName: string, root?: string): Promise<void> {
  return invoke("restore_foreign_file", { fileName, root: root ?? null });
}

/** Outcome of a link-repair pass. */
export interface RelinkReport {
  /** Copy-projections collapsed into hardlinks. */
  upgraded: number;
  /** Already a hardlink — nothing to do. */
  alreadyLinked: number;
  /** Left untouched (no archive twin, cross-volume, or the user's own build). */
  skipped: number;
  /** Bytes reclaimed by deduping copies into shared hardlinks. */
  reclaimedBytes: number;
  /** The mods folder is cloud-synced (no hardlinks) — Silo uses copies; nothing to optimize. */
  hardlinksUnsupported: boolean;
  /** The library archive is on another drive and symlinks aren't available (Windows without
   *  Developer Mode) — active mods stay copies; nothing was done. */
  crossDriveCopies: boolean;
  errors: string[];
}

/** Whether the mods folder supports hardlinks. False on cloud-synced folders (OneDrive /
 *  Google Drive / Proton Drive), where Silo projects the active set as copies. */
export function hardlinkSupport(root?: string): Promise<boolean> {
  return invoke<boolean>("hardlink_support", { root: root ?? null });
}

/** Repair active copy-projections into hardlinks — reclaims duplicated space and makes future
 *  activate/deactivate instant. Emits `relink-progress` `[done, total]` events while it runs. */
export function relinkProjections(root?: string): Promise<RelinkReport> {
  return invoke<RelinkReport>("relink_projections", { root: root ?? null });
}

/** Subscribe to link-repair progress. Returns an unlisten fn. */
export function onRelinkProgress(cb: (done: number, total: number) => void): Promise<UnlistenFn> {
  return listen<[number, number]>("relink-progress", (e) => cb(e.payload[0], e.payload[1]));
}

export function scanMods(roots?: string[]): Promise<ScanResult> {
  return invoke<ScanResult>("scan_mods", { roots: roots ?? null });
}

/** Drop the scan cache so the next scan re-parses & re-categorizes every mod. */
export function clearScanCache(): Promise<void> {
  return invoke("clear_scan_cache");
}

export function getModIcon(
  path: string,
  kind: string,
  iconFilename: string | null,
): Promise<string | null> {
  return invoke<string | null>("get_mod_icon", { path, kind, iconFilename });
}

export function onScanProgress(
  handler: (p: ScanProgress) => void,
): Promise<UnlistenFn> {
  return listen<ScanProgress>("scan:progress", (e) => handler(e.payload));
}

export function getCuration(): Promise<CurationRow[]> {
  return invoke<CurationRow[]>("get_curation");
}

// Whole-row writes: queued per mod so a quick second click can't be overtaken by the first
// (commands run on a worker pool). The value is copied now, so it's the one sent later.
export function setCuration(row: CurationRow): Promise<void> {
  const snapshot = { ...row };
  return serial(`curation:${row.techName}`, () => invoke<void>("set_curation", { row: snapshot }));
}

export function getTags(): Promise<TagRow[]> {
  return invoke<TagRow[]>("get_tags");
}

export function setTags(techName: string, tags: string[]): Promise<void> {
  const snapshot = [...tags];
  return serial(`tags:${techName}`, () => invoke<void>("set_tags", { techName, tags: snapshot }));
}

export function getOverrides(): Promise<CategoryOverride[]> {
  return invoke<CategoryOverride[]>("get_overrides");
}

export function setOverride(row: CategoryOverride): Promise<void> {
  return invoke("set_override", { row });
}

// ── Organize / projection ──
export function planOrganize(mods: ModInput[]): Promise<PlannedMove[]> {
  return invoke<PlannedMove[]>("plan_organize", { root: null, mods });
}

export function applyOrganize(mods: ModInput[]): Promise<OrganizeReport> {
  return invoke<OrganizeReport>("apply_organize", { root: null, mods });
}

// ── Provenance (tamper verification) ──
export function verifyMod(
  techName: string,
  version: string | null,
  path: string,
): Promise<VerifyResult> {
  return invoke<VerifyResult>("verify_mod", { techName, version, path });
}

export function setActive(active: string[]): Promise<OrganizeReport> {
  return invoke<OrganizeReport>("set_active", { root: null, active });
}

export function flatten(): Promise<OrganizeReport> {
  return invoke<OrganizeReport>("flatten", { root: null });
}

// ── Loadouts ──
export function getLoadouts(): Promise<Loadout[]> {
  return invoke<Loadout[]>("get_loadouts");
}

export function saveLoadout(
  id: number | null,
  name: string,
  mods: string[],
): Promise<number> {
  return invoke<number>("save_loadout", { id, name, mods });
}

export function deleteLoadout(id: number): Promise<void> {
  return invoke("delete_loadout", { id });
}

/** Prompt for a path and export the loadout as a .silo file. Returns false if cancelled. */
export async function exportLoadoutFile(id: number, name: string): Promise<boolean> {
  const path = await save({
    defaultPath: `${name}.silo`,
    filters: [{ name: "Silo loadout", extensions: ["silo"] }],
  });
  if (!path) return false;
  await invoke("export_loadout", { id, path });
  return true;
}

/** Prompt for a .silo file and import it as a new loadout. Returns the new id or null. */
export async function importLoadoutFile(): Promise<number | null> {
  const path = await open({
    multiple: false,
    filters: [{ name: "Silo loadout", extensions: ["silo", "json"] }],
  });
  if (!path || Array.isArray(path)) return null;
  return await invoke<number>("import_loadout", { path });
}

export function getSavegames(): Promise<Savegame[]> {
  return invoke<Savegame[]>("get_savegames");
}

export function backupSavegame(folder: string): Promise<string> {
  return invoke<string>("backup_savegame", { folder });
}

export function detectConflicts(mods: ConflictInput[]): Promise<Conflict[]> {
  return invoke<Conflict[]>("detect_conflicts", { mods });
}

export function revealInFolder(path: string): Promise<void> {
  return revealItemInDir(path);
}

/** Open one of Silo's folders in the file manager. Rust resolves the path from the name. */
export function openKnownFolder(which: "modsFolder" | "archive" | "gameInstall"): Promise<void> {
  return invoke("open_known_folder", { which });
}

export function openExternal(url: string): Promise<void> {
  return openUrl(url);
}

/** Saved source pages (where each mod came from). Silo stores them and never requests them. */
export function getModSources(): Promise<SourceRow[]> {
  return invoke<SourceRow[]>("get_mod_sources");
}

/** Save a mod's source page (null or "" removes it). Resolves to the GitHub repo link this
 *  also made, when the page is a GitHub repository and the mod had none. */
export function setModSource(techName: string, url: string | null): Promise<RepoRow | null> {
  return invoke<RepoRow | null>("set_mod_source", { techName, url });
}

/** Record that the user checked a mod's source page now. Resolves to that time (unix secs). */
export function markSourceChecked(techName: string): Promise<number> {
  return invoke<number>("mark_source_checked", { techName });
}

export function getModRepos(): Promise<RepoRow[]> {
  return invoke<RepoRow[]>("get_mod_repos");
}

export function setModRepo(
  techName: string,
  owner: string,
  repo: string,
): Promise<void> {
  return invoke("set_mod_repo", { techName, owner, repo });
}

export function guessRepo(
  path: string,
  kind: string,
): Promise<RepoRow | null> {
  return invoke<RepoRow | null>("guess_repo", { path, kind });
}

export function checkModUpdate(
  owner: string,
  repo: string,
  current: string,
): Promise<UpdateInfo> {
  return invoke<UpdateInfo>("check_mod_update", { owner, repo, current });
}

// ── GitHub OAuth (device flow) ──
export function ghStatus(): Promise<GhStatus> {
  return invoke<GhStatus>("gh_status");
}
export function ghSetClientId(clientId: string): Promise<void> {
  return invoke("gh_set_client_id", { clientId });
}
export function ghDeviceStart(write = false, gist = false): Promise<DeviceCode> {
  return invoke<DeviceCode>("gh_device_start", { write, gist });
}
// Poll takes no intent — capability is read from the scopes GitHub grants on the token.
export function ghDevicePoll(deviceCode: string): Promise<PollResult> {
  return invoke<PollResult>("gh_device_poll", { deviceCode });
}
export function ghLogout(): Promise<void> {
  return invoke("gh_logout");
}
/** PAT fallback: store + verify a personal access token. Returns the login name. */
export function ghSetPat(pat: string): Promise<string> {
  return invoke<string>("gh_set_pat", { pat });
}

// ── GitHub source card (live reads + user-owned actions) ──
export function ghRepoStats(owner: string, repo: string): Promise<RepoStats> {
  return invoke<RepoStats>("gh_repo_stats", { owner, repo });
}
/** Star (on=true) / unstar the repo through the user's own GitHub account. */
export function ghStar(owner: string, repo: string, on: boolean): Promise<boolean> {
  return invoke<boolean>("gh_star", { owner, repo, on });
}
/** Watch (on=true) / unwatch the repo through the user's own GitHub account. */
export function ghWatch(owner: string, repo: string, on: boolean): Promise<boolean> {
  return invoke<boolean>("gh_watch", { owner, repo, on });
}

/**
 * Download a release .zip beside the mod and swap it in, keeping the previous version in the
 * mods folder's backups. Resolves to where that backup is and anything that went wrong after
 * the update landed (warnings: the update itself stands).
 */
export function downloadUpdate(path: string, assetUrl: string): Promise<UpdateOutcome> {
  return invoke<UpdateOutcome>("download_update", { path, assetUrl });
}

/** True while Silo is changing files (a library write or an archive move is running). */
export const appBusy = () => invoke<boolean>("app_busy");
/** Quit even though Silo is busy (after the user confirmed it). */
export const forceQuit = () => invoke<void>("force_quit");
/** While on, library writes and archive moves refuse ("Silo is installing an update."). */
export const setSelfUpdating = (on: boolean) => invoke<void>("set_self_updating", { on });
/** Fires when the window was asked to close while Silo is changing files (the close was held). */
export function onCloseBlocked(cb: () => void): Promise<UnlistenFn> {
  return listen("close-blocked", () => cb());
}

// ── SiloAPI (mod browser / discovery) ──
export function siloapiStatus(): Promise<string> {
  return invoke<string>("siloapi_status");
}

export function browseMods(opts: {
  query?: string;
  category?: string;
  sort?: string;
  /** Facet tags as "namespace:value", ANDed server-side (e.g. ["brand:fendt","region:europe"]). */
  tags?: string[];
  /** Period-correct: keep only mods whose model existed by this year (year_from ≤ Y). */
  availableBy?: number | null;
  limit?: number;
  offset?: number;
}): Promise<BrowsePage> {
  return invoke<BrowsePage>("browse_mods", {
    query: opts.query ?? null,
    category: opts.category ?? null,
    sort: opts.sort ?? null,
    tags: opts.tags && opts.tags.length ? opts.tags : null,
    availableBy: opts.availableBy ?? null,
    limit: opts.limit ?? null,
    offset: opts.offset ?? null,
  });
}

/** The available Browse facets (brand/theme/region/realism/era) + counts, for the filter chips. */
export interface Facets {
  facets: Record<string, { value: string; count: number }[]>;
}
export function siloapiFacets(): Promise<Facets> {
  return invoke<Facets>("siloapi_facets");
}

export function siloapiStats(): Promise<SiloStats> {
  return invoke<SiloStats>("siloapi_stats");
}

/** One mod's full catalog record + every source it was seen on. */
export function siloapiModDetail(id: string): Promise<CatalogModDetail> {
  return invoke<CatalogModDetail>("siloapi_mod_detail", { id });
}

/** Resolve a library mod (by tech name) to its catalog record, or null if not catalogued.
 *  Lets the library detail drawer show the same summary/sources/latest-version as Browse. */
export function catalogDetailByTech(techName: string): Promise<CatalogModDetail | null> {
  return invoke<CatalogModDetail | null>("catalog_detail_by_tech", { techName });
}

/** Fetch a catalog thumbnail as a data: URL (Rust adds the referer the Giants CDN now
 *  requires; cached on disk). Returns "" if it can't be fetched. */
export function catalogImage(url: string): Promise<string> {
  return invoke<string>("catalog_image", { url });
}

/** Catalog categories with counts, for the Browse filter. */
export function siloapiCategories(): Promise<CategoryCount[]> {
  return invoke<CategoryCount[]>("siloapi_categories");
}

/** Download a browsed mod's .zip into the library. Returns the installed filename.
 *  `source` picks which source to fetch from (the button the user clicked). */
export function installRemoteMod(id: string, source?: string): Promise<string> {
  return invoke<string>("install_remote_mod", { id, source: source ?? null, root: null });
}

/** Per-mod download progress during install. */
export function onInstallProgress(
  handler: (p: InstallProgress) => void,
): Promise<UnlistenFn> {
  return listen<InstallProgress>("install:progress", (e) => handler(e.payload));
}

/** Check the whole library against the catalog in one request (by tech name). */
export function catalogCheckUpdates(
  mods: { techName: string; version?: string }[],
): Promise<CatalogUpdate[]> {
  return invoke<CatalogUpdate[]>("catalog_check_updates", {
    mods: mods.map((m) => ({ techName: m.techName, version: m.version ?? null })),
  });
}

/** Parse FS25's log.txt: did the last run crash, and which mods are at fault. */
export function scanLog(): Promise<LogReport> {
  return invoke<LogReport>("scan_log");
}

/** Parse inputBinding.xml into a per-device binding map. */
export function scanBindings(): Promise<BindingReport> {
  return invoke<BindingReport>("scan_bindings");
}

/** Generate a filltype-compatibility bridge companion mod. Returns the created filename. */
export function generateBridge(spec: BridgeSpec): Promise<string> {
  return invoke<string>("generate_bridge", { spec, root: null });
}

// ── Multiplayer mod-set sync ──
/** Prompt for a path and export a manifest of the active set. Returns count, or null if cancelled. */
export async function mpExport(mods: MpModRef[]): Promise<number | null> {
  const path = await save({
    defaultPath: "my-modset.silomp",
    filters: [{ name: "Silo mod-set", extensions: ["silomp", "json"] }],
  });
  if (!path) return null;
  return await invoke<number>("mp_export", { mods, path });
}
/** Prompt for a manifest file and verify the active set against it. Null if cancelled. */
export async function mpVerify(local: MpModRef[]): Promise<MpVerifyReport | null> {
  const path = await open({
    multiple: false,
    filters: [{ name: "Silo mod-set", extensions: ["silomp", "json"] }],
  });
  if (!path || Array.isArray(path)) return null;
  return await invoke<MpVerifyReport>("mp_verify_file", { path, local });
}

// ── Collections (share a mod set as a link) ──
/**
 * Export a mod set as a Collection and publish it to the user's GitHub as a secret gist.
 * Returns the shareable URL + how many mods went in + any dev-mod folders left out.
 * Requires the `gist` scope (Settings → GitHub → Enable collection sharing).
 */
export function collectionExport(
  name: string,
  description: string | null,
  isPublic: boolean,
  mods: MpModRef[],
): Promise<CollectionExportResult> {
  return invoke<CollectionExportResult>("collection_export", {
    name,
    description,
    createdAt: new Date().toISOString(),
    public: isPublic,
    mods,
  });
}

/**
 * Re-pin an existing collection to the given (current active) set and write it back to the
 * same gist/repo — the share link stays the same. Keeps the collection's name/description.
 */
export function collectionUpdate(
  reference: string,
  mods: MpModRef[],
): Promise<CollectionExportResult> {
  return invoke<CollectionExportResult>("collection_update", { reference, mods });
}

/** A collection the user has published, for the "Your collections" management list. */
export interface CollectionSummary {
  kind: "gist" | "repo";
  /** Gist id, or "owner/repo" — what collectionDelete takes. */
  reference: string;
  name: string;
  modCount: number;
  createdAt: string | null;
  /** The silo.hllmr.com/c/ share link. */
  pageUrl: string;
  /** The raw gist/repo URL on GitHub. */
  sourceUrl: string;
  /** Whether Silo can delete it in-app (gists yes; repos → delete on GitHub). */
  canDelete: boolean;
}

/** List the collections you've published to your GitHub (secret gists + silo- repos). */
export function collectionsList(): Promise<CollectionSummary[]> {
  return invoke<CollectionSummary[]>("collections_list");
}

/** Delete a published collection. Gists delete in-app; repos must be removed on GitHub. */
export function collectionDelete(kind: string, reference: string): Promise<void> {
  return invoke("collection_delete", { kind, reference });
}

/**
 * Preview importing a shared collection from a gist link/id, bucketed against the
 * installed library + catalog. Read-only — touches no files.
 */
export function collectionImportPreview(
  urlOrId: string,
  installed: { techName: string; version: string | null }[],
): Promise<ImportPlan> {
  return invoke<ImportPlan>("collection_import_preview", { urlOrId, installed });
}

/**
 * Import a shared collection: download the installable mods, verify each against the
 * pinned build, and save the whole set as a loadout. Returns a per-mod report.
 */
export function collectionApply(
  urlOrId: string,
  installed: { techName: string; version: string | null }[],
  fingerprint: string,
): Promise<ApplyReport> {
  return invoke<ApplyReport>("collection_apply", { urlOrId, installed, fingerprint, root: null });
}

/**
 * Parse a `silo://collection?url=<link>` deep link → the collection URL to import, or null
 * if it isn't a collection deep link. (The web-side "Open in Silo" button on a shared
 * collection points here.)
 */
export function parseCollectionDeepLink(raw: string): string | null {
  try {
    const u = new URL(raw);
    if (u.protocol !== "silo:") return null;
    const kind = u.hostname || u.pathname.replace(/^\/+/, "");
    if (kind !== "collection") return null;
    const target = u.searchParams.get("url");
    return target && target.trim() ? target.trim() : null;
  } catch {
    return null;
  }
}

/** Per-mod progress during a collection import. */
export function onCollectionProgress(
  handler: (p: CollectionProgress) => void,
): Promise<UnlistenFn> {
  return listen<CollectionProgress>("collection:progress", (e) => handler(e.payload));
}

// ── Guided bisection ──
export function bisectPlan(pool: string[]): Promise<BisectStep> {
  return invoke<BisectStep>("bisect_plan", { pool });
}
export function bisectNarrow(
  test: string[],
  rest: string[],
  stillBroken: boolean,
): Promise<string[]> {
  return invoke<string[]>("bisect_narrow", { test, rest, stillBroken });
}
/** Persist the real active set before bisection perturbs it (crash-safe restore). */
export function bisectSnapshotSave(active: string[]): Promise<void> {
  return invoke("bisect_snapshot_save", { active });
}
export function bisectSnapshotGet(): Promise<string[] | null> {
  return invoke<string[] | null>("bisect_snapshot_get");
}
export function bisectSnapshotClear(): Promise<void> {
  return invoke("bisect_snapshot_clear");
}

export function detectGame(): Promise<GameInfo | null> {
  return invoke<GameInfo | null>("detect_game");
}

export function launchGame(): Promise<void> {
  return invoke("launch_game");
}

/** Prompt for a path and save text (used by the diagnostics report). */
export async function saveTextFile(
  defaultName: string,
  content: string,
): Promise<boolean> {
  const path = await save({
    defaultPath: defaultName,
    filters: [{ name: "Report", extensions: ["md", "txt"] }],
  });
  if (!path) return false;
  await invoke("save_text", { path, content });
  return true;
}

/** The diagnostics report plus (optionally) Silo's recent log, with the home folder and user
 *  name removed. */
export function diagnosticsBundle(report: string, includeLog: boolean): Promise<string> {
  return invoke<string>("diagnostics_bundle", { report, includeLog });
}

/** Put `text` on the clipboard. Done in Rust: WebKit (macOS, Linux) refuses
 *  navigator.clipboard after an awaited call, and older WebKitGTK has none. */
export function copyText(text: string): Promise<void> {
  return invoke("copy_text", { text });
}

// ── Mod settings form ──
export function userDirPath(): Promise<string | null> {
  return invoke<string | null>("user_dir_path");
}

export function getConfig(
  path: string,
  paths: string[],
): Promise<Record<string, string>> {
  return invoke<Record<string, string>>("get_config", { path, paths });
}

export function setConfig(
  path: string,
  edits: { path: string; value: string }[],
): Promise<void> {
  return invoke("set_config", { path, edits });
}

export function modsWithSettings(): Promise<string[]> {
  return invoke<string[]>("mods_with_settings");
}

export function getModSettings(modName: string): Promise<SettingsFile[]> {
  return invoke<SettingsFile[]>("get_mod_settings", { modName });
}

export function saveModSettings(
  path: string,
  edits: SettingsEdit[],
): Promise<void> {
  return invoke("save_mod_settings", { path, edits });
}

export function saveModSettingsRaw(path: string, content: string): Promise<void> {
  return invoke("save_mod_settings_raw", { path, content });
}
