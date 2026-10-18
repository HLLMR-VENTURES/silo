import { afterEach, describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tauri-apps/api/core")>()),
  invoke: vi.fn(() => Promise.resolve()),
}));
import { invoke as rawInvoke } from "@tauri-apps/api/core";
import { AppError } from "../errors";
import { dismissToast, MAX_VISIBLE_TOASTS, toast, toasts, visibleToasts } from "../ui/toasts.svelte";
import {
  action,
  activity,
  ACTIVITY_LIMIT,
  clearActivity,
  clearNotice,
  error,
  info,
  markActivitySeen,
  notices,
  reportError,
  setNotice,
  sortedNotices,
  success,
  topNotice,
  unseenErrorCount,
} from "./notify.svelte";
import { pushActivity } from "./activity.svelte";
import { running } from "./ops.svelte";

afterEach(() => {
  for (const t of [...toasts]) dismissToast(t.id);
  notices.length = 0;
  running.length = 0;
  clearActivity();
  vi.mocked(rawInvoke).mockClear();
  vi.useRealTimers();
  vi.restoreAllMocks();
});

const notice = (key: string, level: "blocking" | "warning" | "info", title = key) => ({
  key,
  level,
  title,
  dismissible: true,
});

describe("notices", () => {
  it("topNotice is the highest level, then the newest", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1000);
    setNotice(notice("a", "info"));
    vi.setSystemTime(2000);
    setNotice(notice("b", "warning"));
    vi.setSystemTime(3000);
    setNotice(notice("c", "info"));
    expect(topNotice()?.key).toBe("b");
    expect(sortedNotices().map((n) => n.key)).toEqual(["b", "c", "a"]);
    setNotice(notice("d", "blocking"));
    expect(topNotice()?.key).toBe("d");
  });

  it("same-key notices update in place, keeping their time unless the level changes", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1000);
    setNotice(notice("scan", "info", "Scanning"));
    vi.setSystemTime(2000);
    setNotice(notice("other", "info"));
    vi.setSystemTime(3000);
    setNotice({ ...notice("scan", "info", "Scanning 40%"), progress: { done: 40, total: 100, label: "40 of 100" } });
    expect(notices).toHaveLength(2);
    expect(notices[0]).toMatchObject({ key: "scan", title: "Scanning 40%", at: 1000 });
    expect(topNotice()?.key).toBe("other");
    setNotice(notice("scan", "warning", "Scan stalled"));
    expect(notices[0].at).toBe(3000);
    expect(topNotice()?.key).toBe("scan");
  });

  it("clearing a notice removes it and logs it to activity", () => {
    setNotice(notice("game", "blocking", "Farming Simulator 25 is running"));
    clearNotice("game");
    clearNotice("game");
    expect(notices).toEqual([]);
    expect(topNotice()).toBeNull();
    expect(activity).toHaveLength(1);
    expect(activity[0]).toMatchObject({ kind: "notice", level: "error", message: "Farming Simulator 25 is running" });
  });
});

describe("toasts", () => {
  it("success and info toast and log to activity", () => {
    success("Mod set saved");
    info("Nothing to update");
    expect(toasts.map((t) => [t.message, t.tone])).toEqual([
      ["Mod set saved", "success"],
      ["Nothing to update", "neutral"],
    ]);
    expect(activity.map((a) => [a.level, a.message])).toEqual([
      ["info", "Nothing to update"],
      ["success", "Mod set saved"],
    ]);
  });

  it("error() is sticky, has details, and offers Try again only when retrying can help", () => {
    const retry = vi.fn();
    error(new AppError("LIBRARY_BUSY", "busy"), { op: "Moving mods", retry });
    error(new AppError("NOT_A_MOD", "nope"), { op: "Installing", retry });
    const [busy, notMod] = toasts;
    expect(busy).toMatchObject({
      tone: "danger",
      sticky: true,
      message: "Silo is finishing another change to your library. Try again in a moment.",
    });
    expect(busy.action?.label).toBe("Try again");
    expect(busy.detail).toContain("Code: LIBRARY_BUSY");
    expect(busy.detail).toContain("While: Moving mods");
    expect(notMod.action).toBeUndefined();
    expect(activity[0]).toMatchObject({ level: "error", detail: notMod.detail });
  });

  it("a repeated error bumps a count instead of stacking", () => {
    for (let i = 0; i < 3; i++) error(new AppError("GAME_RUNNING", "running"));
    expect(toasts).toHaveLength(1);
    expect(toasts[0].count).toBe(3);
    expect(activity).toHaveLength(1);
    expect(activity[0].count).toBe(3);
    error(new AppError("DISK_FULL", "full"));
    expect(toasts).toHaveLength(2);
  });

  it("STILL_LOADING: silent for background work, a retryable toast for a user's action", () => {
    vi.spyOn(console, "info").mockImplementation(() => {});
    expect(error(new AppError("STILL_LOADING", "loading"), { background: true })).toBeNull();
    expect(toasts).toEqual([]);
    expect(activity).toEqual([]);
    const retry = vi.fn();
    expect(error(new AppError("STILL_LOADING", "loading"), { op: "Activating a mod", retry })).not.toBeNull();
    expect(toasts[0]).toMatchObject({
      message: "Silo is still loading your library. Try again in a moment.",
      action: { label: "Try again" },
    });
  });

  it("distinct raw messages behind the same copy don't merge; the same one does", () => {
    error("weird failure (os error 1234)", { op: "Saving" });
    error("other failure (os error 4321)", { op: "Saving" });
    expect(toasts).toHaveLength(2);
    error("weird failure (os error 1234)", { op: "Saving" });
    expect(toasts).toHaveLength(2);
    expect(toasts[0].count).toBe(2);
  });

  it("a repeat of an error already seen lights the bell again", () => {
    error(new AppError("DISK_FULL", "full"));
    expect(unseenErrorCount()).toBe(1);
    markActivitySeen();
    expect(unseenErrorCount()).toBe(0);
    error(new AppError("DISK_FULL", "full"));
    expect(toasts[0].count).toBe(2);
    expect(unseenErrorCount()).toBe(1);
    expect(activity).toHaveLength(1);
    // Later repeats still find the same entry.
    error(new AppError("DISK_FULL", "full"));
    expect(activity).toHaveLength(1);
    expect(activity[0].count).toBe(3);
  });

  it(`shows at most ${MAX_VISIBLE_TOASTS}, the newest always among them; the rest queue in order`, () => {
    for (let i = 1; i <= 5; i++) info(`Toast ${i}`);
    expect(visibleToasts().map((t) => t.message)).toEqual(["Toast 1", "Toast 2", "Toast 5"]);
    dismissToast(toasts[0].id);
    expect(visibleToasts().map((t) => t.message)).toEqual(["Toast 2", "Toast 3", "Toast 5"]);
  });

  it(`up to ${MAX_VISIBLE_TOASTS} errors never hide the newest other toast (an Undo)`, () => {
    for (let i = 1; i <= 4; i++) toast({ message: `Error ${i}`, tone: "danger", sticky: true });
    info("Older note");
    const undo = toast({ message: "Switched to Harvest.", action: { label: "Undo", run: () => {} } });
    expect(visibleToasts().map((t) => t.message)).toEqual([
      "Error 1",
      "Error 2",
      "Error 3",
      "Switched to Harvest.",
    ]);
    undo.dismiss();
    expect(visibleToasts().map((t) => t.message)).toEqual(["Error 1", "Error 2", "Error 3", "Older note"]);
    // With one error up, the other toasts get the slots it leaves.
    for (const t of toasts.filter((t) => t.sticky).slice(1)) dismissToast(t.id);
    info("Newest note");
    expect(visibleToasts().map((t) => t.message)).toEqual(["Error 1", "Older note", "Newest note"]);
  });
});

describe("reportError", () => {
  it("logs to the backend and toasts", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    reportError(new AppError("DISK_FULL", "No space left on device (os error 28)"), "Installing FS25_Foo");
    expect(rawInvoke).toHaveBeenCalledWith("log_frontend_error", {
      level: "error",
      message: "Installing FS25_Foo: No space left on device (os error 28)",
      context: "DISK_FULL",
    });
    expect(toasts).toHaveLength(1);
  });

  it("quiet: logs and adds an activity entry, no toast", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    reportError(new Error("render blew up"), "showing Library", { quiet: true });
    expect(toasts).toEqual([]);
    expect(activity[0]).toMatchObject({ level: "error", message: "Something went wrong while showing Library." });
    expect(rawInvoke).toHaveBeenCalledTimes(1);
  });

  it("silent codes from background work are logged but never shown", () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    reportError(new AppError("STILL_LOADING", "loading"), "Scanning", { background: true });
    reportError(new AppError("STILL_LOADING", "loading"), "Scanning", { quiet: true });
    expect(rawInvoke).toHaveBeenCalledTimes(2);
    expect(toasts).toEqual([]);
    expect(activity).toEqual([]);
  });

  it("STILL_LOADING from a user's action says so", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    reportError(new AppError("STILL_LOADING", "loading"), "Activating a mod");
    expect(toasts.map((t) => t.message)).toEqual(["Silo is still loading your library. Try again in a moment."]);
    await action("Organizing", () => Promise.reject(new AppError("STILL_LOADING", "loading")));
    expect(toasts[0].count).toBe(2);
  });

  it("a failing log call is swallowed", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(rawInvoke).mockRejectedValueOnce("no backend");
    expect(() => reportError("boom", "Saving")).not.toThrow();
    await Promise.resolve();
  });
});

describe("action", () => {
  it("runs as an op, toasts success, returns the result", async () => {
    let seen: string[] = [];
    const out = await action(
      "Saving the mod set",
      async () => {
        seen = running.map((o) => o.label);
        return 42;
      },
      { success: "Mod set saved", exclusive: "library-write" },
    );
    expect(out).toBe(42);
    expect(seen).toEqual(["Saving the mod set"]);
    expect(running).toEqual([]);
    expect(toasts.map((t) => t.message)).toEqual(["Mod set saved"]);
    expect(activity.map((a) => a.kind)).toEqual(["op", "toast"]);
  });

  it("reports a failure, returns undefined, and still ends its op", async () => {
    vi.spyOn(console, "error").mockImplementation(() => {});
    const retry = vi.fn();
    const out = await action(
      "Moving mods",
      async () => {
        throw new AppError("FILE_IN_USE", "in use");
      },
      { retry, success: "Moved" },
    );
    expect(out).toBeUndefined();
    expect(running).toEqual([]);
    expect(toasts).toHaveLength(1);
    expect(toasts[0].action?.run).toBe(retry);
    expect(rawInvoke).toHaveBeenCalledWith("log_frontend_error", expect.objectContaining({ context: "FILE_IN_USE" }));
  });
});

describe("activity", () => {
  it("keeps the newest 100 and counts unseen errors", () => {
    for (let i = 0; i < ACTIVITY_LIMIT + 5; i++) pushActivity({ kind: "toast", level: "info", message: `m${i}` });
    expect(activity).toHaveLength(ACTIVITY_LIMIT);
    expect(activity[0].message).toBe(`m${ACTIVITY_LIMIT + 4}`);
    markActivitySeen();
    expect(unseenErrorCount()).toBe(0);
    pushActivity({ kind: "toast", level: "error", message: "bad" });
    pushActivity({ kind: "toast", level: "warning", message: "meh" });
    expect(unseenErrorCount()).toBe(1);
    markActivitySeen();
    expect(unseenErrorCount()).toBe(0);
  });

  it("past the limit, the oldest non-error entries go first, so errors survive a flood", () => {
    pushActivity({ kind: "toast", level: "error", message: "first error" });
    for (let i = 0; i < ACTIVITY_LIMIT + 20; i++) pushActivity({ kind: "op", level: "info", message: `op ${i}` });
    expect(activity).toHaveLength(ACTIVITY_LIMIT);
    expect(activity.at(-1)?.message).toBe("first error");
    expect(activity[0].message).toBe(`op ${ACTIVITY_LIMIT + 19}`);
    // Only errors left past the limit: the oldest error goes.
    clearActivity();
    for (let i = 0; i < ACTIVITY_LIMIT + 1; i++) pushActivity({ kind: "toast", level: "error", message: `e${i}` });
    expect(activity).toHaveLength(ACTIVITY_LIMIT);
    expect(activity.at(-1)?.message).toBe("e1");
  });
});
