// Running an update check and "Update all", with the app's calls passed in so the orchestration
// unit-tests without Tauri.
//   const { rows, catalogError } = await runUpdateCheck(mods, repoMap, deps);
//   const res = await updateAll(sections.ready, { begin: beginWrite, install, stops, ... });
//   const toastText = updateAllSummary(res);
// Nothing here installs on its own: updateAll runs only when the user presses Update all.

import type { CatalogUpdate, UpdateInfo, UpdateOutcome, UpdateRow } from "../types";
import { plural } from "../plural";
import { displayVersion } from "../format";

/** What the check reads from a library mod. */
export interface CheckMod {
  techName: string;
  title?: string | null;
  path: string;
  version?: string | null;
  /** The file's size and modified time when the scan read `version` (#402). */
  size?: number;
  mtimeMs?: number;
}

/** A mod as the catalog check asks about it. */
export type CatalogAsk = {
  techName: string;
  version?: string;
  path?: string;
  size?: number;
  mtimeMs?: number;
};

/** A linked GitHub repo (the drawer's "Source & updates"). */
export interface LinkedRepo {
  owner: string;
  repo: string;
}

/** A failure as the view shows it: the sentence, and the text behind "Copy details". */
export interface ExplainedError {
  message: string;
  detail: string;
  /** No network or a timeout: the catalog may be fine, Silo just couldn't reach it. */
  offline?: boolean;
}

export interface CheckDeps {
  catalog: (mods: CatalogAsk[]) => Promise<CatalogUpdate[]>;
  checkRepo: (owner: string, repo: string, current: string) => Promise<UpdateInfo>;
  /** Log the failure and word it (inlineError + formatDetail). */
  explain: (e: unknown, op: string) => ExplainedError;
  /** The rows so far, as they come in (the view fills while GitHub repos are checked). */
  onRows?: (rows: UpdateRow[]) => void;
  /** Linked repos checked so far, of how many. */
  onProgress?: (done: number, total: number) => void;
}

export interface CheckResult {
  rows: UpdateRow[];
  /** The catalog request failed (offline, or the catalog is down); null when it answered. */
  catalogError: ExplainedError | null;
}

/** The activity-log and "Copy details" line for one mod's check: names the mod (#333). */
export const checkOp = (mod: Pick<CheckMod, "title" | "techName">) =>
  `Checking ${mod.title || mod.techName} for an update`;

/**
 * Check one linked repo and fold the answer into `row` (the repo is authoritative for the source
 * the user chose). A failure marks the row only when the catalog had no answer for it.
 * @returns the updated row (a copy)
 */
async function checkLinked(
  row: UpdateRow,
  mod: CheckMod,
  repo: LinkedRepo,
  hadCatalogAnswer: boolean,
  deps: Pick<CheckDeps, "checkRepo" | "explain">,
): Promise<UpdateRow> {
  try {
    const info = await deps.checkRepo(repo.owner, repo.repo, mod.version ?? "0");
    return {
      ...row,
      current: info.current,
      latest: info.release.tag,
      hasUpdate: info.hasUpdate,
      assetUrl: info.release.assetUrl,
      // No .zip on the release: its page is where the user gets it (not the catalog's ModHub page).
      pageUrl: info.release.assetUrl ? row.pageUrl : (info.release.htmlUrl ?? row.pageUrl),
      source: "github",
      error: undefined,
      errorDetail: undefined,
    };
  } catch (e) {
    const why = deps.explain(e, checkOp(mod));
    if (hadCatalogAnswer) return row;
    return { ...row, hasUpdate: undefined, error: why.message || "Silo couldn't check this mod.", errorDetail: why.detail };
  }
}

/** Mods Silo linked to their catalog entries (#402) whose rows are owed a refresh, in the
 *  folder (generation) the check that asked about them was of. */
export interface HeldRefresh {
  gen: number;
  names: string[];
}

/**
 * Which of the mods Silo just linked in the background (`linked`, techNames; #402) the Updates
 * list refreshes, and when: now, or once nothing keeps a check from starting (a check running,
 * an update installing). Only mods a check the user allowed asked about in this folder: the
 * running one (its snapshot, and only while it's of this folder) or else the last one (failed
 * or not). Mods added since, of another folder, or never asked about aren't sent anywhere.
 */
export function linkRefresh(s: {
  linked: string[];
  gen: number;
  running: { gen: number; asked: ReadonlySet<string> } | null;
  lastAsked: ReadonlySet<string> | undefined;
  blocked: boolean;
}): { when: "now" | "after" | "no"; names: string[] } {
  const pool = s.running
    ? s.running.gen === s.gen
      ? s.running.asked
      : undefined
    : s.lastAsked;
  const names = pool ? [...new Set(s.linked.filter((n) => pool.has(n)))] : [];
  if (names.length === 0) return { when: "no", names };
  return { when: s.running || s.blocked ? "after" : "now", names };
}

/** `held` with `names` for folder `gen` added (a refresh held for another folder is replaced). */
export function holdRefresh(
  held: HeldRefresh | null,
  gen: number,
  names: string[],
): HeldRefresh {
  const before = held && held.gen === gen ? held.names : [];
  return { gen, names: [...new Set([...before, ...names])] };
}

/**
 * The held refresh that may run now (`run`), and what stays held. One held for another folder
 * is dropped, never run in this one.
 */
export function heldRefresh(
  held: HeldRefresh | null,
  gen: number,
  busy: boolean,
): { run: string[] | null; held: HeldRefresh | null } {
  if (!held || held.gen !== gen) return { run: null, held: null };
  return busy ? { run: null, held } : { run: held.names, held: null };
}

/**
 * The library entries a link refresh asks about: those of `names` not `leftOut` (left out of
 * update checks, or updated since the last scan that worked). The same rule picks the entries
 * its answer is compared with, so a mod left out meanwhile never makes the two differ.
 */
export function refreshTargets<M extends { techName: string }>(
  names: string[],
  mods: M[],
  leftOut: (techName: string) => boolean,
): M[] {
  const want = new Set(names);
  return mods.filter((m) => want.has(m.techName) && !leftOut(m.techName));
}

/** Which file each of `mods` is (path, size, modified time, version), by techName. */
export function fileStamps(
  mods: {
    techName: string;
    path: string;
    size?: number;
    mtimeMs?: number;
    version?: string | null;
  }[],
): Map<string, string> {
  return new Map(
    mods.map((m) => [
      m.techName,
      JSON.stringify([m.path, m.size, m.mtimeMs, m.version ?? null]),
    ]),
  );
}

/** Whether two `fileStamps` name the same files. */
export function sameStamps(
  a: Map<string, string>,
  b: Map<string, string>,
): boolean {
  return a.size === b.size && [...a].every(([k, v]) => b.get(k) === v);
}

/** The Updates rows with those of `names` replaced by `fresh` (a refresh of just those mods). */
export function mergeRows(
  rows: UpdateRow[],
  fresh: UpdateRow[],
  names: ReadonlySet<string>,
): UpdateRow[] {
  return [...rows.filter((r) => !names.has(r.techName)), ...fresh];
}

/**
 * Check the library: one catalog request for every mod (GitHub and ModHub alike), then each
 * linked GitHub repo directly.
 * @param mods the library
 * @param repos linked repos by tech name
 * @param deps the app's calls
 */
export async function runUpdateCheck(
  mods: CheckMod[],
  repos: Record<string, LinkedRepo>,
  deps: CheckDeps,
): Promise<CheckResult> {
  const byTech = new Map(mods.map((m) => [m.techName, m]));
  const rows = new Map<string, UpdateRow>();
  let catalogError: ExplainedError | null = null;

  try {
    const catalog = await deps.catalog(
      // The file, so a mod Silo installed under another name finds its catalog entry (#386),
      // and the size and time the version was read at, for linking older such installs (#402).
      mods.map((m) => ({
        techName: m.techName,
        version: m.version ?? undefined,
        path: m.path,
        size: m.size,
        mtimeMs: m.mtimeMs,
      })),
    );
    for (const c of catalog) {
      const m = byTech.get(c.techName);
      if (!m) continue;
      if (c.error) {
        // Couldn't be checked (#386): shown as such, never as up to date.
        rows.set(c.techName, { techName: c.techName, title: m.title || c.techName, path: m.path, error: c.error, errorDetail: c.error });
        continue;
      }
      rows.set(c.techName, {
        techName: c.techName,
        title: m.title || c.techName,
        path: m.path,
        current: m.version ?? undefined,
        latest: c.latest ?? undefined,
        hasUpdate: c.hasUpdate,
        assetUrl: c.downloadUrl,
        source: c.source ?? undefined,
        pageUrl: c.pageUrl ?? null,
      });
    }
  } catch (e) {
    catalogError = deps.explain(e, "Checking Silo's catalog for updates");
  }
  deps.onRows?.([...rows.values()]);

  const linked = Object.entries(repos).filter(([t]) => byTech.has(t));
  let done = 0;
  deps.onProgress?.(0, linked.length);
  for (const [techName, repo] of linked) {
    const m = byTech.get(techName)!;
    const had = rows.get(techName);
    const base: UpdateRow = had ?? { techName, title: m.title || techName, path: m.path };
    rows.set(techName, await checkLinked(base, m, repo, !!had, deps));
    deps.onProgress?.(++done, linked.length);
    deps.onRows?.([...rows.values()]);
  }
  return { rows: [...rows.values()], catalogError };
}

/**
 * Try one failed row again ("Couldn't check" → Try again): its linked repo only.
 * @returns the new row
 */
export async function recheckRow(
  row: UpdateRow,
  mod: CheckMod,
  repo: LinkedRepo,
  deps: Pick<CheckDeps, "checkRepo" | "explain">,
): Promise<UpdateRow> {
  return checkLinked(row, mod, repo, false, deps);
}

// ── Update all ──

/** The op token App's beginWrite hands out (ops.svelte.ts). */
export interface WriteToken {
  progress(done: number, total: number, label?: string): void;
  setLabel(label: string): void;
  end(): void;
}

export interface UpdateAllDeps {
  /** Take the exclusive library write; null when refused (it says why itself). */
  begin: (label: string) => WriteToken | null;
  install: (row: UpdateRow) => Promise<UpdateOutcome>;
  /** A failure that would fail every remaining mod too (the game is running, the library is
   *  busy, the archive drive is gone, the disk is full, no internet): stop there. */
  stops: (e: unknown) => boolean;
  onRowStart?: (row: UpdateRow) => void;
  onRowDone?: (row: UpdateRow, outcome: UpdateOutcome) => void;
  onRowFailed?: (row: UpdateRow, e: unknown) => void;
}

export interface UpdateAllResult {
  /** The library write was refused before anything ran. */
  refused: boolean;
  updated: { row: UpdateRow; outcome: UpdateOutcome }[];
  failed: { row: UpdateRow; error: unknown }[];
  /** Left alone after a stopping failure. */
  notTried: UpdateRow[];
  /** The failure that stopped the run, if one did. */
  stoppedBy: unknown;
}

/**
 * Update the given mods one at a time under one library write, reporting "Updating 2 of 4:
 * AutoDrive" as it goes. A failed mod doesn't stop the rest, unless the failure would fail them
 * all (see `stops`).
 * @param rows the Ready to install rows, in the order shown
 * @param deps the app's calls
 */
export async function updateAll(rows: UpdateRow[], deps: UpdateAllDeps): Promise<UpdateAllResult> {
  const res: UpdateAllResult = { refused: false, updated: [], failed: [], notTried: [], stoppedBy: null };
  if (rows.length === 0) return res;
  const n = rows.length;
  const op = deps.begin(`Updating ${n} ${plural(n, { one: "mod", other: "mods" })}`);
  if (!op) return { ...res, refused: true };
  try {
    for (let i = 0; i < n; i++) {
      const row = rows[i];
      op.setLabel(`Updating ${i + 1} of ${n}: ${row.title}`);
      op.progress(i, n);
      deps.onRowStart?.(row);
      try {
        const outcome = await deps.install(row);
        res.updated.push({ row, outcome });
        deps.onRowDone?.(row, outcome);
      } catch (e) {
        res.failed.push({ row, error: e });
        deps.onRowFailed?.(row, e);
        if (deps.stops(e)) {
          res.stoppedBy = e;
          res.notTried = rows.slice(i + 1);
          break;
        }
      }
    }
    op.progress(n - res.notTried.length, n);
  } finally {
    op.end();
  }
  return res;
}

export interface Summary {
  tone: "success" | "warn" | "danger";
  message: string;
  body?: string;
}

const mods = (n: number) => `${n} ${plural(n, { one: "mod", other: "mods" })}`;

/**
 * The one toast at the end of Update all.
 * @param res what happened
 * @param why words the stopping failure (describeError's title)
 * @returns null when the write was refused (beginWrite already said why) or nothing ran
 */
export function updateAllSummary(res: UpdateAllResult, why: (e: unknown) => string): Summary | null {
  if (res.refused) return null;
  const ok = res.updated.length;
  const bad = res.failed.length;
  const skipped = res.notTried.length;
  if (ok + bad === 0) return null;
  const problems = res.updated.flatMap((u) => u.outcome.problems);
  if (bad === 0) {
    const message =
      ok === 1
        ? `Updated ${res.updated[0].row.title}${res.updated[0].row.latest ? ` to ${displayVersion(res.updated[0].row.latest)}` : ""}. The previous version is in backups.`
        : `Updated ${mods(ok)}. The previous versions are in backups.`;
    return problems.length
      ? { tone: "warn", message, body: problems.join(" ") }
      : { tone: "success", message };
  }
  let message = `${ok} updated, ${bad} failed.`;
  if (skipped) message += ` ${skipped} not tried.`;
  const body = [res.stoppedBy !== null ? why(res.stoppedBy) : "Each failed mod shows why in its row.", ...problems]
    .filter(Boolean)
    .join(" ");
  return { tone: ok > 0 ? "warn" : "danger", message, body };
}

/** What the check/install guards read: Update all and each row's state. */
export interface InstallActivity {
  updatingAll: boolean;
  rowStates: Record<string, { phase: string }>;
}

/**
 * Why a new check can't start now, or "": a check replaces every row, so it waits while an
 * update is installing (its row would lose its state and its "updated" mark).
 */
export function checkBlockedReason(a: InstallActivity): string {
  const installing = a.updatingAll || Object.values(a.rowStates).some((s) => s.phase === "installing");
  return installing ? "Wait for the updates to finish." : "";
}

/** Why Update / Update all can't run while a check is running, or "". */
export function installBlockedReason(checking: boolean): string {
  return checking ? "Wait for the check to finish." : "";
}
