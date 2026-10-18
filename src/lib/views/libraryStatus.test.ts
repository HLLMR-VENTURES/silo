// Status chip counts, the Play menu's active-set line and the Library status strip (#230).
import { describe, expect, it } from "vitest";
import { libraryStatusCounts, stripItemSpecs, stripLeft } from "./libraryStatus";

const none = {
  criticalConflicts: 0,
  warningConflicts: 0,
  missingDeps: 0,
  corrupt: 0,
  ignored: 0,
  foreign: 0,
  inactiveFindings: 0,
};

describe("libraryStatusCounts", () => {
  it("nothing to report", () => {
    expect(libraryStatusCounts(none)).toEqual({ problems: 0, review: 0 });
  });

  it("an unresolved crash is one problem", () => {
    expect(libraryStatusCounts({ ...none, crash: 1 })).toEqual({ problems: 1, review: 0 });
  });

  it("critical conflicts and mods that can't load are problems", () => {
    expect(
      libraryStatusCounts({ ...none, criticalConflicts: 1, missingDeps: 2, corrupt: 3, ignored: 4 }),
    ).toEqual({ problems: 10, review: 0 });
  });

  it("health findings on inactive mods are to review, not problems", () => {
    expect(libraryStatusCounts({ ...none, missingDeps: 1, inactiveFindings: 4 })).toEqual({
      problems: 1,
      review: 4,
    });
  });

  it("warning conflicts and foreign files are to review", () => {
    expect(libraryStatusCounts({ ...none, warningConflicts: 2, foreign: 1 })).toEqual({
      problems: 0,
      review: 3,
    });
  });
});

describe("stripLeft", () => {
  const base = { scanning: false, scanned: true, ready: true, active: 142, total: 1204, setName: null };
  it("ready, with counts", () => {
    expect(stripLeft(base)).toBe("Ready to play · 142 of 1,204 active");
  });
  it("adds the matching set's name", () => {
    expect(stripLeft({ ...base, setName: "Harvest 2026" })).toBe(
      "Ready to play · 142 of 1,204 active · Harvest 2026",
    );
  });
  it("before the first scan", () => {
    expect(stripLeft({ ...base, scanned: false, scanning: true })).toBe("Reading your library…");
    expect(stripLeft({ ...base, scanned: false })).toBe("Your library hasn't been read yet");
  });
  it("while rescanning", () => {
    expect(stripLeft({ ...base, scanning: true })).toBe("Rescanning · 142 of 1,204 active");
  });
  it("leads with the counts when Play isn't ready (no game, Play blocked, a critical conflict)", () => {
    expect(stripLeft({ ...base, ready: false })).toBe("142 of 1,204 active");
    expect(stripLeft({ ...base, ready: false, setName: "Harvest 2026" })).toBe(
      "142 of 1,204 active · Harvest 2026",
    );
    expect(stripLeft({ ...base, ready: false, scanning: true })).toBe("Rescanning · 142 of 1,204 active");
  });
});

describe("stripItemSpecs", () => {
  it("nothing to show", () => {
    expect(stripItemSpecs({ criticalConflicts: 0, unfiled: 0, updates: 0 })).toEqual([]);
  });

  it("orders critical conflicts first, then unfiled mods, then updates", () => {
    const items = stripItemSpecs({ criticalConflicts: 2, unfiled: 5, updates: 1 });
    expect(items.map((i) => i.id)).toEqual(["critical", "unfiled", "updates"]);
    expect(items[0]).toEqual({ id: "critical", text: "2 conflicts need fixing", action: "See why" });
    expect(items[1]).toEqual({ id: "unfiled", text: "5 new mods aren't filed", action: "Review" });
    expect(items[2]).toEqual({ id: "updates", text: "1 update", action: "See updates" });
  });

  it("singular copy", () => {
    const items = stripItemSpecs({ criticalConflicts: 1, unfiled: 1, updates: 3 });
    expect(items.map((i) => i.text)).toEqual(["1 conflict needs fixing", "1 new mod isn't filed", "3 updates"]);
  });

  it("puts an unresolved crash first", () => {
    const items = stripItemSpecs({ crash: true, criticalConflicts: 1, unfiled: 0, updates: 2 });
    expect(items.map((i) => i.id)).toEqual(["crash", "critical", "updates"]);
    expect(items[0]).toEqual({ id: "crash", text: "Your last game ended unexpectedly", action: "See details" });
  });
});
