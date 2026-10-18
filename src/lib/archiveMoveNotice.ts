// The library archive's notice (#166, #228): an unfinished move (running or paused, with
// Pause / Resume / Retry / Cancel), or else a custom archive whose folder isn't available.
// Pure: App feeds it the move's state and the actions, and syncs the result into the notice area.

import type { ArchiveStatus, MoveProgress } from "./api";
import { capList, modsCount, sentence, unDot } from "./archiveMoveLogic";
import { fmtSize } from "./format";
import type { Notice } from "./state/notify.svelte";

/** The notice keys this module raises. At most one is up at a time. */
export const ARCHIVE_MOVE_KEY = "archive-move";
export const ARCHIVE_UNAVAILABLE_KEY = "archive-unavailable";

export interface ArchiveNoticeState {
  archive: ArchiveStatus | null;
  progress: MoveProgress | null;
  /** A move is copying right now. */
  running: boolean;
  /** A move is running or paused. */
  unfinished: boolean;
  /** The move is heading back home. */
  cancelling: boolean;
  /** Pause was clicked and hasn't taken effect yet. */
  pauseRequested: boolean;
}

export interface ArchiveNoticeActions {
  pause: () => void | Promise<void>;
  resume: () => void | Promise<void>;
  cancel: () => void | Promise<void>;
  /** The archive is unavailable: look again (and rescan). */
  retry: () => void | Promise<void>;
  /** The archive is unavailable: pick another folder. */
  chooseAnother: () => void | Promise<void>;
}

/**
 * The archive notice for this state, or null when there's nothing to say.
 * @returns the notice (its `key` says which of the two it is), or null
 */
export function archiveNotice(s: ArchiveNoticeState, a: ArchiveNoticeActions): Notice | null {
  const moving = s.archive?.moving ?? null;
  if (s.unfinished) {
    const cancel = s.cancelling ? [] : [{ label: "Cancel move", run: a.cancel }];
    if (s.running) {
      const where = moving ? ` to ${s.cancelling ? moving.from : moving.to}` : "";
      const p = s.progress;
      const counted = p && p.total > 0;
      return {
        key: ARCHIVE_MOVE_KEY,
        level: "warning",
        icon: "archive",
        title: `${s.cancelling ? "Moving your library back" : "Moving your library"}${where}.`,
        body:
          (counted
            ? `${p.done}/${p.total} mods · ${fmtSize(p.bytesDone)} of ${fmtSize(p.bytesTotal)}. `
            : "Starting. ") + "Launch is blocked until it pauses or finishes.",
        progress: counted ? { done: p.done, total: p.total, label: "Mods moved" } : undefined,
        actions: [
          {
            label: s.pauseRequested ? "Pausing…" : "Pause",
            run: a.pause,
            primary: true,
            disabled: s.pauseRequested,
          },
          ...cancel,
        ],
        dismissible: false,
      };
    }
    if (moving?.errors.length) {
      const reason = moving.pauseReason ? `${sentence(moving.pauseReason)} ` : "";
      return {
        key: ARCHIVE_MOVE_KEY,
        level: "blocking",
        icon: "archive",
        title: `${modsCount(moving.errors.length)} couldn't be moved.`,
        body:
          `${sentence(capList(moving.errors.map(unDot), 3, "; "))} ${reason}The move is paused. ` +
          `Retry tries them again${s.cancelling ? "." : ", or Cancel moves everything back."}`,
        actions: [{ label: "Retry", run: a.resume, primary: true }, ...cancel],
        dismissible: false,
      };
    }
    if (moving) {
      const reason = moving.pauseReason ? `${sentence(moving.pauseReason)} ` : "";
      return {
        key: ARCHIVE_MOVE_KEY,
        level: "warning",
        icon: "archive",
        title: s.cancelling ? "Moving your library back is paused." : "Archive move paused.",
        body: `${reason}You can play — resume when you're done.`,
        actions: [{ label: "Resume", run: a.resume, primary: true }, ...cancel],
        dismissible: false,
      };
    }
    return null;
  }
  if (s.archive?.path && !s.archive.available) {
    return {
      key: ARCHIVE_UNAVAILABLE_KEY,
      level: "blocking",
      title: `Your library archive isn't available: ${s.archive.path}.`,
      body: "Parked mods are hidden until it's back; active mods linked from it won't load in-game.",
      actions: [
        { label: "Try again", run: a.retry, primary: true },
        { label: "Choose another", run: a.chooseAnother },
      ],
      dismissible: false,
    };
  }
  return null;
}
