import { describe, expect, it } from "vitest";
import type { ArchiveStatus, MoveMarker, MoveProgress } from "./api";
import {
  cancelWasLost,
  capList,
  isArchiveMissing,
  isMoveCancelling,
  isMoveRunning,
  isMoveUnfinished,
  joinProblems,
  lostCancelText,
  modsCount,
  pollShouldSettle,
  problemLines,
  sentence,
  settleMayOfferAdopt,
  unDot,
} from "./archiveMoveLogic";

const idle: ArchiveStatus = {
  path: "D:/archive",
  isDefault: false,
  available: true,
  moving: null,
  running: false,
};

function moving(over: Partial<MoveMarker> = {}): ArchiveStatus {
  return {
    ...idle,
    moving: {
      from: "C:/mods/archive",
      to: "D:/archive",
      direction: "forward",
      paused: true,
      pauseReason: null,
      errors: [],
      moved: [],
      ...over,
    },
  };
}

const progress = (cancelling: boolean): MoveProgress => ({
  done: 1,
  total: 2,
  bytesDone: 10,
  bytesTotal: 20,
  current: "FS25_A.zip",
  cancelling,
});

describe("move state", () => {
  it("is running while the backend copies or this window waits on an invoke", () => {
    expect(isMoveRunning(null, 0)).toBe(false);
    expect(isMoveRunning(idle, 0)).toBe(false);
    expect(isMoveRunning({ ...idle, running: true }, 0)).toBe(true);
    expect(isMoveRunning(idle, 1)).toBe(true);
  });

  it("is unfinished while running or paused with a marker", () => {
    expect(isMoveUnfinished(idle, false)).toBe(false);
    expect(isMoveUnfinished(idle, true)).toBe(true);
    expect(isMoveUnfinished(moving(), false)).toBe(true);
  });

  it("is cancelling when heading back, a cancel is in flight, or running progress says so", () => {
    expect(isMoveCancelling(moving(), false, false, null)).toBe(false);
    expect(isMoveCancelling(moving({ direction: "back" }), false, false, null)).toBe(true);
    expect(isMoveCancelling(idle, true, false, null)).toBe(true);
    expect(isMoveCancelling(idle, false, true, progress(true))).toBe(true);
    // A stale "cancelling" progress event doesn't count once nothing runs.
    expect(isMoveCancelling(idle, false, false, progress(true))).toBe(false);
    expect(isMoveCancelling(idle, false, true, progress(false))).toBe(false);
  });

  it("calls only an unavailable custom archive missing", () => {
    expect(isArchiveMissing(null)).toBe(false);
    expect(isArchiveMissing(idle)).toBe(false);
    expect(isArchiveMissing({ ...idle, available: false })).toBe(true);
    expect(isArchiveMissing({ ...idle, available: false, isDefault: true })).toBe(false);
    expect(isArchiveMissing({ ...idle, path: null, available: false })).toBe(false);
  });
});

describe("poll and settle gates", () => {
  it("settles from the poll only when a move it wasn't waiting on stopped", () => {
    expect(pollShouldSettle(true, idle, 0)).toBe(true);
    expect(pollShouldSettle(true, idle, 1)).toBe(false); // the waiting invoke settles it
    expect(pollShouldSettle(true, { ...idle, running: true }, 0)).toBe(false);
    expect(pollShouldSettle(false, idle, 0)).toBe(false);
  });

  it("offers adopt only after a finished, clean move", () => {
    expect(settleMayOfferAdopt(idle, null)).toBe(true);
    expect(settleMayOfferAdopt(null, null)).toBe(true);
    expect(settleMayOfferAdopt(idle, "Copy failed")).toBe(false);
    expect(settleMayOfferAdopt(moving(), null)).toBe(false);
    expect(settleMayOfferAdopt({ ...idle, running: true }, null)).toBe(false);
  });
});

describe("lost cancel", () => {
  const base = {
    lastProgressCancelling: false,
    sawTurnAround: false,
    cancelHome: "C:/mods/archive",
    archivePath: "D:/archive",
  };

  it("is lost when nothing turned around and the library landed elsewhere", () => {
    expect(cancelWasLost(base)).toBe(true);
    expect(cancelWasLost({ ...base, cancelHome: null })).toBe(true);
  });

  it("isn't lost when the move turned around or came home", () => {
    expect(cancelWasLost({ ...base, lastProgressCancelling: true })).toBe(false);
    expect(cancelWasLost({ ...base, sawTurnAround: true })).toBe(false);
    expect(cancelWasLost({ ...base, archivePath: "C:/mods/archive" })).toBe(false);
  });

  it("explains where the library is and how to move it", () => {
    expect(lostCancelText("D:/archive", false)).toBe(
      "The move finished before it could be cancelled. Your library is now in D:/archive. " +
        "Use Change… in Settings to move it again.",
    );
    expect(lostCancelText(null, true)).toBe(
      "The move finished before it could be cancelled. Your library is now in its new folder. " +
        "Use 'Use default' in Settings to move it back.",
    );
  });
});

describe("message helpers", () => {
  it("joins only the messages that are set", () => {
    expect(joinProblems(null, undefined, "")).toBeNull();
    expect(joinProblems("A.", null, "B.")).toBe("A. B.");
  });

  it("ends messages with exactly one period", () => {
    expect(unDot("Done.. ")).toBe("Done");
    expect(sentence("Done")).toBe("Done.");
    expect(sentence("Done.")).toBe("Done.");
    expect(problemLines(["One.", "Two"])).toBe("One.\nTwo.");
  });

  it("counts and caps lists", () => {
    expect(modsCount(1)).toBe("1 mod");
    expect(modsCount(3)).toBe("3 mods");
    expect(capList(["a", "b"])).toBe("a, b");
    expect(capList(["a", "b", "c", "d", "e"])).toBe("a, b, c +2 more");
    expect(capList(["a", "b", "c", "d"], 3, "; ")).toBe("a; b; c +1 more");
  });
});
