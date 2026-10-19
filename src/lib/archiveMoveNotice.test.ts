import { describe, expect, it, vi } from "vitest";
import type { ArchiveStatus, MoveMarker, MoveProgress } from "./api";
import {
  archiveNotice,
  ARCHIVE_MOVE_KEY,
  ARCHIVE_UNAVAILABLE_KEY,
  type ArchiveNoticeState,
} from "./archiveMoveNotice";

const actions = () => ({
  pause: vi.fn(),
  resume: vi.fn(),
  cancel: vi.fn(),
  retry: vi.fn(),
  chooseAnother: vi.fn(),
});

const marker = (over: Partial<MoveMarker> = {}): MoveMarker => ({
  from: "C:/mods/archive",
  to: "E:/silo",
  direction: "forward",
  paused: false,
  pauseReason: null,
  errors: [],
  moved: [],
  ...over,
});

const archive = (over: Partial<ArchiveStatus> = {}): ArchiveStatus => ({
  path: "C:/mods/archive",
  isDefault: true,
  available: true,
  moving: null,
  running: false,
  ...over,
});

const idle: ArchiveNoticeState = {
  archive: archive(),
  progress: null,
  running: false,
  unfinished: false,
  cancelling: false,
  pauseRequested: false,
};

const progress: MoveProgress = {
  done: 3,
  total: 12,
  bytesDone: 3 * 1024 ** 2,
  bytesTotal: 12 * 1024 ** 2,
  current: "FS25_X.zip",
  cancelling: false,
};

const labels = (n: ReturnType<typeof archiveNotice>) => n?.actions?.map((a) => a.label);

describe("archiveNotice", () => {
  it("says nothing when no move is unfinished and the archive is there", () => {
    expect(archiveNotice(idle, actions())).toBeNull();
    expect(archiveNotice({ ...idle, archive: null }, actions())).toBeNull();
  });

  it("a running move: a warning with progress, Pause and Cancel", () => {
    const a = actions();
    const n = archiveNotice(
      { ...idle, archive: archive({ moving: marker(), running: true }), running: true, unfinished: true, progress },
      a,
    );
    expect(n).toMatchObject({ key: ARCHIVE_MOVE_KEY, level: "warning", title: "Moving your library to E:/silo." });
    expect(n?.progress).toEqual({ done: 3, total: 12, label: "Mods moved" });
    expect(n?.body).toMatch(/^3 of 12 mods · .* You can't press Play until it pauses or finishes\.$/);
    expect(labels(n)).toEqual(["Pause", "Cancel move"]);
    void n?.actions?.[0].run();
    expect(a.pause).toHaveBeenCalled();
  });

  it("before the first progress event it says it's starting; Pause turns into Pausing… once clicked", () => {
    const n = archiveNotice(
      { ...idle, archive: archive({ moving: marker() }), running: true, unfinished: true, pauseRequested: true },
      actions(),
    );
    expect(n?.body).toMatch(/^Starting\. /);
    expect(n?.progress).toBeUndefined();
    expect(n?.actions?.[0]).toMatchObject({ label: "Pausing…", disabled: true });
  });

  it("moving back names where it goes and offers no Cancel", () => {
    const n = archiveNotice(
      {
        ...idle,
        archive: archive({ moving: marker({ direction: "back" }) }),
        running: true,
        unfinished: true,
        cancelling: true,
      },
      actions(),
    );
    expect(n?.title).toBe("Moving your library back to C:/mods/archive.");
    expect(labels(n)).toEqual(["Pause"]);
  });

  it("paused with failed mods: blocking, Retry resumes", () => {
    const a = actions();
    const n = archiveNotice(
      {
        ...idle,
        archive: archive({ moving: marker({ errors: ["FS25_A.zip is in use."], pauseReason: "FS25 started" }) }),
        unfinished: true,
      },
      a,
    );
    expect(n).toMatchObject({ key: ARCHIVE_MOVE_KEY, level: "blocking", title: "1 mod couldn't be moved." });
    expect(n?.body).toBe(
      "FS25_A.zip is in use. FS25 started. The move is paused. Press Try again to move them, or Cancel move to move everything back.",
    );
    expect(labels(n)).toEqual(["Try again", "Cancel move"]);
    void n?.actions?.[0].run();
    expect(a.resume).toHaveBeenCalled();
  });

  it("paused: a warning with Resume", () => {
    const n = archiveNotice(
      { ...idle, archive: archive({ moving: marker({ paused: true }) }), unfinished: true },
      actions(),
    );
    expect(n).toMatchObject({ level: "warning", title: "Library move paused." });
    expect(labels(n)).toEqual(["Resume", "Cancel move"]);
  });

  it("an archive whose folder is gone: blocking, under its own key", () => {
    const a = actions();
    const n = archiveNotice(
      { ...idle, archive: archive({ path: "E:/silo", isDefault: false, available: false }) },
      a,
    );
    expect(n).toMatchObject({
      key: ARCHIVE_UNAVAILABLE_KEY,
      level: "blocking",
      title: "Your library archive isn't available: E:/silo.",
      dismissible: false,
    });
    expect(labels(n)).toEqual(["Try again", "Choose another"]);
    void n?.actions?.[1].run();
    expect(a.chooseAnother).toHaveBeenCalled();
  });
});
