import { afterEach, describe, expect, it, vi } from "vitest";
import { activity, clearActivity } from "./activity.svelte";
import { blocked, current, running, start, tryStart } from "./ops.svelte";

afterEach(() => {
  running.length = 0;
  clearActivity();
  vi.useRealTimers();
});

describe("ops", () => {
  it("ending one op leaves the other running", () => {
    const a = start("Scanning mods");
    const b = start("Checking for updates");
    expect(running.map((o) => o.label)).toEqual(["Scanning mods", "Checking for updates"]);
    a.end();
    expect(running.map((o) => o.label)).toEqual(["Checking for updates"]);
    b.end();
    expect(running).toEqual([]);
  });

  it("end() is idempotent and only ever ends its own op", () => {
    const a = start("A");
    const b = start("B");
    a.end();
    a.end();
    expect(running.map((o) => o.id)).toEqual([b.id]);
    expect(activity.filter((e) => e.message === "A")).toHaveLength(1);
  });

  it("blocked() names the op holding a class; tryStart() skips while it's held", () => {
    expect(blocked("library-write")).toEqual({ blocked: false, reason: "" });
    const move = start("Moving your library", { exclusive: "library-write" });
    start("Checking for updates");
    expect(blocked("library-write")).toEqual({
      blocked: true,
      reason: "Silo is moving your library.",
    });
    expect(tryStart("Scanning", { exclusive: "library-write" })).toBeNull();
    expect(tryStart("Reading the log")).not.toBeNull();
    move.end();
    expect(blocked("library-write").blocked).toBe(false);
    const scan = tryStart("Scanning", { exclusive: "library-write" });
    expect(scan).not.toBeNull();
    scan!.end();
  });

  it("current() prefers the most recent exclusive op, else the most recent", () => {
    expect(current()).toBeNull();
    const a = start("A");
    start("Write", { exclusive: "library-write" });
    start("C");
    expect(current()?.label).toBe("Write");
    running.splice(1, 1); // drop the exclusive one
    expect(current()?.label).toBe("C");
    a.end();
  });

  it("progress() and setLabel() update the op in place", () => {
    const op = start("Installing", { progress: true });
    expect(running[0]).toMatchObject({ hasProgress: true, progress: null });
    op.progress(3, 12, "3 of 12 files");
    expect(running[0].progress).toEqual({ done: 3, total: 12, label: "3 of 12 files" });
    op.setLabel("Installing FS25_Foo");
    expect(running[0].label).toBe("Installing FS25_Foo");
    op.end();
    op.progress(4, 12); // after end: ignored
    expect(running).toEqual([]);
  });

  it("a finished op lands in activity with its duration", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000_000);
    const op = start("Organizing");
    vi.setSystemTime(1_002_500);
    op.end();
    expect(activity[0]).toMatchObject({ kind: "op", level: "info", message: "Organizing", durationMs: 2500 });
  });
});
