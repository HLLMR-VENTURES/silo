import { describe, expect, it, vi } from "vitest";
import {
  modsFolderNotices,
  rootsFor,
  showOverrideNotice,
  switchBlockReason,
  type SwitchState,
} from "./modsStatus";
import type { ModsRootStatus } from "./api";

function status(over: Partial<ModsRootStatus> = {}): ModsRootStatus {
  return {
    path: "C:/mods",
    source: "user",
    gameReads: "C:/mods",
    mismatch: false,
    available: true,
    modsFolderMissing: false,
    known: [],
    canWriteGameSetting: true,
    gameSettingProblem: null,
    unresolvedOverride: null,
    layoutProblem: null,
    layoutProblemCause: null,
    layoutConflict: null,
    ...over,
  };
}

describe("switchBlockReason", () => {
  const idle: SwitchState = { moveUnfinished: false, scanning: false, busy: null, blocked: false };

  it("waits for a move, a scan or a busy action first", async () => {
    const none = async () => false;
    expect(await switchBlockReason({ ...idle, moveUnfinished: true }, none)).toBe(
      "Finish or cancel the library move first.",
    );
    expect(await switchBlockReason({ ...idle, scanning: true }, none)).toBe(
      "Wait for the scan to finish.",
    );
    expect(await switchBlockReason({ ...idle, busy: "Organizing" }, none)).toBe(
      "Wait for “Organizing” to finish.",
    );
    expect(await switchBlockReason(idle, none)).toBeNull();
  });

  it("keeps an interrupted bisection's folder until it's dealt with", async () => {
    expect(await switchBlockReason(idle, async () => true)).toMatch(/Find the problem mod/);
  });

  it("never gates leaving a blocked folder on a bisection snapshot", async () => {
    let asked = false;
    const snap = async () => {
      asked = true;
      return true;
    };
    expect(await switchBlockReason({ ...idle, blocked: true }, snap)).toBeNull();
    expect(asked).toBe(false);
    // A move still comes first, even then.
    expect(await switchBlockReason({ ...idle, blocked: true, moveUnfinished: true }, snap)).toBe(
      "Finish or cancel the library move first.",
    );
  });
});

describe("showOverrideNotice", () => {
  it("tells once that Silo follows FS25's folder", () => {
    expect(showOverrideNotice(status({ source: "gameOverride" }), false)).toBe(true);
    expect(showOverrideNotice(status({ source: "gameOverride" }), true)).toBe(false);
    expect(showOverrideNotice(status({ source: "user" }), false)).toBe(false);
  });

  it("stays quiet while that folder is blocked (its own banner explains)", () => {
    expect(
      showOverrideNotice(status({ source: "gameOverride", layoutProblem: "overlaps" }), false),
    ).toBe(false);
  });
});

describe("rootsFor", () => {
  it("scans the live folder", () => {
    expect(rootsFor(status())).toEqual(["C:/mods"]);
  });

  it("scans nothing for a missing or unavailable folder", () => {
    expect(rootsFor(status({ path: null }))).toEqual([]);
    expect(rootsFor(status({ available: false }))).toEqual([]);
  });

  it("scans nothing while the folder is blocked", () => {
    expect(
      rootsFor(status({ layoutProblem: "That folder overlaps another mods folder's archive." })),
    ).toEqual([]);
  });
});

describe("modsFolderNotices (#228)", () => {
  const acts = () => ({
    retry: vi.fn(),
    chooseAnother: vi.fn(),
    useGameFolder: vi.fn(),
    pointGameHere: vi.fn(),
    createFolder: vi.fn(),
    dismissFollowed: vi.fn(),
  });
  const labels = (n: { actions?: { label: string }[] } | null) => n?.actions?.map((a) => a.label);

  it("Try again is off, saying why, while a library write runs", () => {
    const busy = { ...acts(), retryBlocked: "Silo is filing 3 mods into the library." };
    const retry = modsFolderNotices(status({ available: false }), false, busy).modsFolder?.actions?.[0];
    expect(retry).toMatchObject({
      label: "Try again",
      disabled: true,
      title: "Silo is filing 3 mods into the library.",
    });
    const idle = modsFolderNotices(status({ available: false }), false, { ...acts(), retryBlocked: "" });
    expect(idle.modsFolder?.actions?.[0].disabled).toBeUndefined();
  });

  it("says nothing for a healthy folder, or before the first status", () => {
    expect(modsFolderNotices(status(), false, acts())).toEqual({
      modsFolder: null,
      override: null,
      followed: null,
    });
    expect(modsFolderNotices(null, true, acts()).modsFolder).toBeNull();
  });

  it("an unavailable chosen folder blocks, with the banner's three ways out", () => {
    const n = modsFolderNotices(status({ available: false }), false, acts());
    expect(n.modsFolder).toMatchObject({
      key: "mods-folder",
      level: "blocking",
      title: "Your mods folder isn't available: C:/mods.",
      dismissible: false,
    });
    expect(labels(n.modsFolder)).toEqual(["Try again", "Choose another", "Use the game's folder"]);
    expect(n.override).toBeNull();
  });

  it("a blocked folder explains why; Try again only after a failed read", () => {
    const blocked = status({
      layoutProblem: "It overlaps another folder's archive.",
      layoutProblemCause: "overlap",
      layoutConflict: "D:/other",
      mismatch: true,
    });
    const n = modsFolderNotices(blocked, true, acts());
    expect(n.modsFolder?.body).toBe("It overlaps another folder's archive. (D:/other)");
    expect(labels(n.modsFolder)).toEqual(["Point FS25 here", "Choose another"]);
    // The mismatch is part of the blocked notice, not a second one; no "now follows" either.
    expect(n.override).toBeNull();
    expect(n.followed).toBeNull();
    const readError = modsFolderNotices(
      status({ layoutProblem: "Silo couldn't read its database.", layoutProblemCause: "readError" }),
      false,
      acts(),
    );
    expect(labels(readError.modsFolder)).toEqual(["Try again", "Choose another"]);
  });

  it("a missing mods folder offers to create it", () => {
    const n = modsFolderNotices(status({ modsFolderMissing: true }), false, acts());
    expect(n.modsFolder).toMatchObject({ level: "warning" });
    expect(labels(n.modsFolder)).toEqual(["Create it", "Choose a folder"]);
  });

  it("FS25 reading another folder is its own warning", () => {
    const n = modsFolderNotices(status({ mismatch: true, gameReads: "D:/fs" }), false, acts());
    expect(n.modsFolder).toBeNull();
    expect(n.override).toMatchObject({
      key: "override",
      level: "warning",
      title: "Farming Simulator 25 is reading a different mods folder (D:/fs).",
    });
    expect(labels(n.override)).toEqual(["Point FS25 here", "Use the game's folder"]);
    const cantWrite = modsFolderNotices(
      status({ mismatch: true, canWriteGameSetting: false }),
      false,
      acts(),
    );
    expect(labels(cantWrite.override)).toEqual(["Use the game's folder"]);
  });

  it("the one-time 'now follows FS25' notice is dismissible and remembers it", () => {
    const a = acts();
    const n = modsFolderNotices(status({ source: "gameOverride" }), true, a);
    expect(n.followed).toMatchObject({ key: "override-followed", level: "info", dismissible: true });
    n.followed?.onDismiss?.();
    expect(a.dismissFollowed).toHaveBeenCalled();
  });
});
