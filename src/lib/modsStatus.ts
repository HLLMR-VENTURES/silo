import type { Notice, NoticeAction } from "./state/notify.svelte";
import type { ModsRootStatus } from "./api";

/**
 * The folders to scan for this status: the live mods folder, unless it's unavailable or
 * blocked (it shares files with another layout, so Silo neither scans nor writes it).
 */
export function rootsFor(st: ModsRootStatus): string[] {
  return st.available && st.path && !st.layoutProblem ? [st.path] : [];
}

/**
 * Whether to say, once, that Silo now follows FS25's own folder — not while that folder is
 * blocked: its banner explains what's wrong instead.
 */
export function showOverrideNotice(st: ModsRootStatus, alreadyShown: boolean): boolean {
  return st.source === "gameOverride" && !alreadyShown && !st.layoutProblem;
}

/** What switching mods folders has to wait for. */
export interface SwitchState {
  moveUnfinished: boolean;
  scanning: boolean;
  busy: string | null;
  /** The current folder is blocked (`layoutProblem`): Silo changes nothing there. */
  blocked: boolean;
}

/**
 * Why switching folders isn't safe right now, or null. An interrupted guided bisection keeps
 * its folder until it's restored or discarded (`hasBisectSnapshot`) — except when the current
 * folder is blocked: nothing can be restored there, and switching away is the way out, so the
 * snapshot is kept and not asked about.
 */
export async function switchBlockReason(
  s: SwitchState,
  hasBisectSnapshot: () => Promise<boolean>,
): Promise<string | null> {
  if (s.moveUnfinished) return "Finish or cancel the library move first.";
  if (s.scanning) return "Wait for the scan to finish.";
  if (s.busy) return `Wait for “${s.busy}” to finish.`;
  if (s.blocked) return null;
  if (await hasBisectSnapshot())
    return "Finish or cancel Find the problem mod first. It belongs to this folder.";
  return null;
}

// ── Notices (#228) ──
// The mods-folder conditions, in the order the old banners checked them: at most one of
// unavailable / blocked / not created yet is up (key `mods-folder`); FS25 reading another folder
// (key `override`) only when none of those is; "Silo now follows FS25's folder" (key
// `override-followed`) on its own.

export const MODS_FOLDER_KEY = "mods-folder";
export const OVERRIDE_KEY = "override";
export const OVERRIDE_FOLLOWED_KEY = "override-followed";

export interface ModsFolderActions {
  retry: () => void | Promise<void>;
  chooseAnother: () => void | Promise<void>;
  useGameFolder: () => void | Promise<void>;
  pointGameHere: () => void | Promise<void>;
  createFolder: () => void | Promise<void>;
  /** The "Silo now follows FS25's folder" notice was dismissed. */
  dismissFollowed: () => void;
  /** Why "Try again" is off right now (a library write is running); empty or absent when it's on. */
  retryBlocked?: string;
}

/** "Try again" for the mods folder, off (titled with the reason) while a write runs. */
function retryAction(a: ModsFolderActions): NoticeAction {
  const action: NoticeAction = { label: "Try again", run: a.retry, primary: true };
  return a.retryBlocked ? { ...action, disabled: true, title: a.retryBlocked } : action;
}

export interface ModsFolderNotices {
  modsFolder: Notice | null;
  override: Notice | null;
  followed: Notice | null;
}

/**
 * The mods-folder notices for a status.
 * @param st the status, or null before the first read
 * @param followedOpen the one-time "Silo now uses FS25's folder" notice hasn't been dismissed
 */
export function modsFolderNotices(
  st: ModsRootStatus | null,
  followedOpen: boolean,
  a: ModsFolderActions,
): ModsFolderNotices {
  const out: ModsFolderNotices = { modsFolder: null, override: null, followed: null };
  if (st && st.source === "user" && !st.available) {
    out.modsFolder = {
      key: MODS_FOLDER_KEY,
      level: "blocking",
      title: `Your mods folder isn't available: ${st.path}.`,
      body: "Nothing was scanned. Reconnect the drive, or pick another folder.",
      actions: [
        retryAction(a),
        { label: "Choose another", run: a.chooseAnother },
        { label: "Use the game's folder", run: a.useGameFolder },
      ],
      dismissible: false,
    };
  } else if (st?.layoutProblem) {
    out.modsFolder = {
      key: MODS_FOLDER_KEY,
      level: "blocking",
      title: `Silo won't change files in this mods folder: ${st.path}.`,
      body: st.layoutProblem + (st.layoutConflict ? ` (${st.layoutConflict})` : ""),
      actions: [
        ...(st.layoutProblemCause === "readError"
          ? [retryAction(a)]
          : []),
        ...(st.mismatch && st.canWriteGameSetting
          ? [{ label: "Point FS25 here", run: a.pointGameHere }]
          : []),
        { label: "Choose another", run: a.chooseAnother },
      ],
      dismissible: false,
    };
  } else if (st?.modsFolderMissing) {
    out.modsFolder = {
      key: MODS_FOLDER_KEY,
      level: "warning",
      icon: "info",
      title: "Farming Simulator 25 hasn't created its mods folder yet.",
      actions: [
        { label: "Create it", run: a.createFolder, primary: true },
        { label: "Choose a folder", run: a.chooseAnother },
      ],
      dismissible: false,
    };
  } else if (st?.mismatch && st.gameProfile?.enabled) {
    // The folder's own FS25 profile (#387) points the game at another mods folder. "Use the
    // game's folder" would leave the profile for the main one, so it isn't offered.
    out.override = {
      key: OVERRIDE_KEY,
      level: "warning",
      title: `This folder's FS25 profile loads mods from a different folder${st.gameReads ? ` (${st.gameReads})` : ""}.`,
      body: "Its own game settings point there, so mods you turn on here won't load in-game. Change that setting in the game, or choose another folder.",
      actions: [{ label: "Choose another", run: a.chooseAnother }],
      dismissible: false,
    };
  } else if (st?.mismatch) {
    out.override = {
      key: OVERRIDE_KEY,
      level: "warning",
      title: `Farming Simulator 25 is reading a different mods folder${st.gameReads ? ` (${st.gameReads})` : ""}.`,
      body: "Mods you turn on here won't load in-game.",
      actions: [
        ...(st.canWriteGameSetting
          ? [{ label: "Point FS25 here", run: a.pointGameHere, primary: true }]
          : []),
        { label: "Use the game's folder", run: a.useGameFolder },
      ],
      dismissible: false,
    };
  }
  if (followedOpen && st?.path && !st.layoutProblem) {
    out.followed = {
      key: OVERRIDE_FOLLOWED_KEY,
      level: "info",
      title: `Silo now uses the mods folder FS25 is set to: ${st.path}.`,
      dismissible: true,
      onDismiss: a.dismissFollowed,
    };
  }
  return out;
}
