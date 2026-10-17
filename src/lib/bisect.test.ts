import { describe, expect, it } from "vitest";
import { bisectRestoreSet, canStartBisect, finishBisect, type BisectFinishDeps } from "./bisect";

describe("bisectRestoreSet", () => {
  it("restores the snapshot saved when bisection started", () => {
    expect(bisectRestoreSet(["map", "a", "b", "c", "d"], ["map"], ["a", "b"])).toEqual([
      "map", "a", "b", "c", "d",
    ]);
  });
  it("falls back to the current set only when no snapshot exists", () => {
    expect(bisectRestoreSet(null, ["map"], ["a", "b"])).toEqual(["map", "a", "b"]);
    expect(bisectRestoreSet([], ["map"], ["a"])).toEqual(["map", "a"]);
  });
  it("never duplicates a mod", () => {
    expect(bisectRestoreSet(["a", "a", "b"], [], [])).toEqual(["a", "b"]);
  });
});

describe("canStartBisect", () => {
  it("refuses while an earlier run's snapshot is waiting", () => {
    expect(canStartBisect(["map", "a"])).toBe(false);
  });
  it("allows a start when there is no snapshot", () => {
    expect(canStartBisect(null)).toBe(true);
    expect(canStartBisect(undefined)).toBe(true);
    expect(canStartBisect([])).toBe(true);
  });
});

/** Fake snapshot store + apply that records the order of calls. */
function fakeDeps(opts: { snapshot: string[] | null; applyFails?: boolean; getFails?: boolean }) {
  const calls: string[] = [];
  let stored: string[] | null = opts.snapshot;
  const applied: string[][] = [];
  const deps: BisectFinishDeps = {
    get: async () => {
      calls.push("get");
      if (opts.getFails) throw "db locked";
      return stored;
    },
    apply: async (s) => {
      calls.push("apply");
      if (opts.applyFails) throw "could not remove a file";
      applied.push(s);
    },
    clear: async () => {
      calls.push("clear");
      stored = null;
    },
  };
  return { deps, calls, applied, stored: () => stored };
}

describe("finishBisect", () => {
  it("applies the snapshot, then clears it", async () => {
    const f = fakeDeps({ snapshot: ["map", "a", "b", "c"] });
    expect(await finishBisect(f.deps, ["map"], ["a"])).toEqual({ ok: true });
    expect(f.calls).toEqual(["get", "apply", "clear"]);
    expect(f.applied).toEqual([["map", "a", "b", "c"]]);
    expect(f.stored()).toBeNull();
  });
  it("keeps the snapshot when applying fails", async () => {
    const f = fakeDeps({ snapshot: ["map", "a", "b"], applyFails: true });
    expect(await finishBisect(f.deps, ["map"], ["a"])).toEqual({
      ok: false,
      error: "could not remove a file",
    });
    expect(f.calls).toEqual(["get", "apply"]);
    expect(f.stored()).toEqual(["map", "a", "b"]);
  });
  it("keeps the snapshot when reading it fails", async () => {
    const f = fakeDeps({ snapshot: ["map", "a"], getFails: true });
    expect(await finishBisect(f.deps, ["map"], ["a"])).toEqual({ ok: false, error: "db locked" });
    expect(f.calls).toEqual(["get"]);
    expect(f.stored()).toEqual(["map", "a"]);
  });
  it("falls back to the current set when no snapshot exists", async () => {
    const f = fakeDeps({ snapshot: null });
    expect(await finishBisect(f.deps, ["map"], ["a", "b"])).toEqual({ ok: true });
    expect(f.applied).toEqual([["map", "a", "b"]]);
  });
});
