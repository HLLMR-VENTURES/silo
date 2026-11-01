// Diagnose's words (#260): the crash notice, conflict titles and headline, Last run grouping,
// and keeping only the newest conflict check.
import { describe, expect, it } from "vitest";
import { bisectRounds, crashNoticeOffers, crashNoticeText, likelyCulprit } from "./crashNotice";
import { canParkOne, conflictCounts, conflictHeadline, conflictTitle, fillTypeCapNote, groundPileCapNote, severityLabel } from "./conflictCopy";
import { countsLine, groupLogMods, lastRunMeta, lastRunVerdict, logModDetails, logTimeLabel } from "./lastRun";
import { latestOnly } from "./latest";
import type { LogModHealth, LogReport } from "../types";

function mod(modName: string, errors: number, warnings = 0, benign = 0): LogModHealth {
  return { modName, errors, warnings, benign, sample: `Error: ${modName} broke`, sampleLine: 120 };
}
function report(crashed: boolean, mods: LogModHealth[], over: Partial<LogReport> = {}): LogReport {
  return {
    engineVersion: "1.4.0.0",
    modCount: 737,
    cleanExit: !crashed,
    crashed,
    mods,
    unattributed: 0,
    totalFindings: 0, groundPileCap: null, groundPileDisabled: [], fillTypeCap: null, droppedFillTypes: [], truncated: false,
    ...over,
  };
}
const lib = new Map([["FS25_Mulcher", { title: "Field Mulcher Extended", active: true }]]);
const lookup = (t: string) => lib.get(t) ?? null;

describe("crash notice", () => {
  it("names the mod with the most errors, by its library title", () => {
    const c = likelyCulprit(report(true, [mod("FS25_Mulcher", 14), mod("FS25_Other", 2)]), lookup);
    expect(c).toEqual({ modName: "FS25_Mulcher", title: "Field Mulcher Extended", inLibrary: true, active: true, errors: 14 });
    expect(crashNoticeText(c, { suspects: 46 })).toEqual({
      title: "Your last game ended unexpectedly.",
      body: "Most likely cause: Field Mulcher Extended (14 errors in the log).",
    });
    expect(crashNoticeOffers(c, { suspects: 46 })).toEqual({ park: true, find: false });
  });

  it("after parking: says so, and offers no second park", () => {
    const c = likelyCulprit(report(true, [mod("FS25_Mulcher", 1)]), lookup);
    expect(crashNoticeText(c, { parked: true, suspects: 46 }).body).toBe(
      "You parked Field Mulcher Extended. Play again to check.",
    );
    expect(crashNoticeOffers(c, { parked: true, suspects: 46 }).park).toBe(false);
  });

  it("a log name the library doesn't have: shown as is, with details only", () => {
    const c = likelyCulprit(report(true, [mod("FS25_Gone", 3)]), lookup);
    expect(c?.title).toBe("FS25_Gone");
    expect(crashNoticeText(c, { suspects: 46 }).body).toBe(
      "Most likely cause: FS25_Gone (3 errors in the log). It isn't a mod in your library.",
    );
    expect(crashNoticeOffers(c, { suspects: 46 })).toEqual({ park: false, find: false });
  });

  it("no named mod: offers Find the problem mod with about how many launches", () => {
    const c = likelyCulprit(report(true, [mod("FS25_Quiet", 0, 2)]), lookup);
    expect(c).toBeNull();
    expect(crashNoticeText(null, { suspects: 200 })).toEqual({
      title: "Your last game ended unexpectedly, and the log doesn't name a mod.",
      body: "Silo can find the problem mod by testing half your mods at a time (about 8 launches).",
    });
    expect(crashNoticeOffers(null, { suspects: 200 })).toEqual({ park: false, find: true });
    expect(crashNoticeOffers(null, { suspects: 1 }).find).toBe(false);
    expect(bisectRounds(2)).toBe(1);
    expect(bisectRounds(46)).toBe(6);
  });
});

describe("conflict copy", () => {
  it("severities read Problem / Review / Note", () => {
    expect(["critical", "warning", "info"].map((s) => severityLabel(s as "critical"))).toEqual(["Problem", "Review", "Note"]);
  });

  it("plain titles by kind", () => {
    const two = ["A", "B"];
    expect(conflictTitle({ kind: "action", mods: two })).toBe("Two mods add the same control action");
    expect(conflictTitle({ kind: "script", mods: two })).toBe("Two mods ship a script with the same file name");
    expect(conflictTitle({ kind: "fillType", mods: ["A", "B", "C"] })).toBe("3 mods define the same fill type");
    expect(conflictTitle({ kind: "uniqueType", mods: two })).toBe("Two mods claim the same exclusive slot");
    // #339: the ground-pile budget reads by how serious it is, not by how many mods.
    expect(conflictTitle({ kind: "heightTypes", mods: two, severity: "critical" })).toBe("More ground-pile types than the map has room for");
    expect(conflictTitle({ kind: "heightTypes", mods: two, severity: "warning" })).toBe("Possibly more ground-pile types than the map has room for");
    expect(conflictTitle({ kind: "heightTypes", mods: [], severity: "info" })).toBe("Ground-pile slots are nearly full");
    expect(conflictTitle({ kind: "fillTypeCap", mods: two, severity: "warning" })).toBe(
      "Possibly more material types than the game supports",
    );
    expect(conflictTitle({ kind: "fillTypeCap", mods: [], severity: "info" })).toBe("Material types are nearly full");
    expect(conflictTitle({ kind: "map", mods: ["M1", "M2", "M3", "M4"] })).toBe("More than one map is active");
    expect(conflictTitle({ kind: "somethingNew", mods: two })).toBe("Two mods clash");
  });

  it("the maps note has no Park one menu", () => {
    expect(canParkOne({ kind: "map", techNames: ["M1", "M2"] })).toBe(false);
    expect(canParkOne({ kind: "script", techNames: ["A", "B"] })).toBe(true);
    expect(canParkOne({ kind: "script", techNames: ["A"] })).toBe(false);
  });

  it("headline: problems, to review, notes hidden", () => {
    const c = conflictCounts([
      { severity: "warning" },
      { severity: "info" },
      { severity: "info" },
      { severity: "warning" },
    ]);
    expect(c).toEqual({ problems: 0, review: 2, notes: 2 });
    expect(conflictHeadline(c, false)).toEqual(["0 problems", "2 to review", "2 notes hidden"]);
    expect(conflictHeadline(c, true)).toEqual(["0 problems", "2 to review", "2 notes"]);
    expect(conflictHeadline({ problems: 1, review: 0, notes: 0 }, false)).toEqual(["1 problem", "0 to review"]);
    expect(conflictHeadline({ problems: 0, review: 0, notes: 0 }, false)).toEqual(["No conflicts"]);
  });
});

describe("Last run", () => {
  const r = report(false, [mod("FS25_A", 2, 3, 1), mod("FS25_B", 0, 2, 0), mod("FS25_C", 0, 4, 4)]);

  it("groups causes, worth a look and cosmetic-only", () => {
    const g = groupLogMods(r);
    expect(g.causes.map((m) => m.modName)).toEqual(["FS25_A"]);
    expect(g.worth.map((m) => m.modName)).toEqual(["FS25_B"]);
    expect(g.cosmetic.map((m) => m.modName)).toEqual(["FS25_C"]);
  });

  it("verdicts", () => {
    expect(lastRunVerdict(report(true, []), 0).title).toBe("Your last game ended unexpectedly.");
    expect(lastRunVerdict(r, 1)).toEqual({
      tone: "warn",
      title: "Your last game closed normally,",
      body: "but 1 mod logged errors worth a look.",
    });
    expect(lastRunVerdict(report(false, []), 0).title).toBe("Healthy.");
  });

  it("counts, details and the meta line", () => {
    expect(countsLine(mod("X", 14, 4, 1))).toBe("14 errors · 3 warnings · 1 cosmetic");
    expect(logModDetails(mod("FS25_Mulcher", 1), "Field Mulcher Extended")).toBe(
      "Field Mulcher Extended (FS25_Mulcher)\n1 error\nlog.txt line 120: Error: FS25_Mulcher broke",
    );
    expect(lastRunMeta(r, "9:42 PM")).toBe("FS25 1.4.0.0 · 737 mods loaded · log from 9:42 PM");
    expect(lastRunMeta(report(false, [], { engineVersion: null, modCount: 1 }), null)).toBe("1 mod loaded");
    expect(lastRunMeta(report(false, [], { truncated: true }), null)).toBe(
      "FS25 1.4.0.0 · 737 mods loaded · very long log: Silo read only its start and end",
    );
  });

  it("log time: the time today, the date before that", () => {
    const now = new Date(2026, 9, 18, 23, 0).getTime();
    expect(logTimeLabel(new Date(2026, 9, 18, 21, 42).getTime(), now, "en-US")).toBe("9:42 PM");
    expect(logTimeLabel(new Date(2026, 9, 12, 21, 42).getTime(), now, "en-US")).toBe("Oct 12, 9:42 PM");
  });
});

describe("latestOnly (the conflict check)", () => {
  it("an older, slower check never overwrites the newer one", async () => {
    const latest = latestOnly<string>();
    let applied: string | null = null;
    let finishOld!: (v: string) => void;
    const old = latest.run(
      () => new Promise<string>((r) => (finishOld = r)),
      (v) => (applied = v),
    );
    const fresh = latest.run(async () => "new set: maps note", (v) => (applied = v));
    expect(await fresh).toBe(true);
    finishOld("old set: nothing");
    expect(await old).toBe(false);
    expect(applied).toBe("new set: maps note");
  });

  it("cancel drops a check in flight", async () => {
    const latest = latestOnly<number>();
    let applied = 0;
    const p = latest.run(async () => 5, (v) => (applied = v));
    latest.cancel();
    expect(await p).toBe(false);
    expect(applied).toBe(0);
  });
});

describe("header mini-status", () => {
  it("names the round once one is running", async () => {
    const { bisectStatusText } = await import("../components/shell/status");
    expect(bisectStatusText(null)).toBe("Finding the problem mod");
    expect(bisectStatusText(2)).toBe("Finding the problem mod · round 2");
  });
});

describe("crash notice model", () => {
  const culprit = likelyCulprit(report(true, [mod("FS25_Mulcher", 14)]), lookup);
  const base = {
    culprit,
    live: { title: "Field Mulcher Extended", active: true },
    parked: false,
    suspects: 46,
    libraryKnown: true,
    bisecting: false,
    recoveryPending: false,
  };

  it("offers Park for an active library mod", async () => {
    const { crashNoticeModel } = await import("./crashNotice");
    expect(crashNoticeModel(base)).toEqual({
      title: "Your last game ended unexpectedly.",
      body: "Most likely cause: Field Mulcher Extended (14 errors in the log).",
      park: "Field Mulcher Extended",
      find: false,
    });
  });

  it("hidden until the library is read, and while Find the problem mod runs", async () => {
    const { crashNoticeModel } = await import("./crashNotice");
    expect(crashNoticeModel({ ...base, libraryKnown: false })).toBeNull();
    expect(crashNoticeModel({ ...base, bisecting: true })).toBeNull();
  });

  it("parked: no second Park; Undo (active again) offers Park again", async () => {
    const { crashNoticeModel } = await import("./crashNotice");
    const parked = crashNoticeModel({ ...base, parked: true, live: { title: "Field Mulcher Extended", active: false } });
    expect(parked?.body).toBe("You parked Field Mulcher Extended. Play again to check.");
    expect(parked?.park).toBeNull();
    const undone = crashNoticeModel({ ...base, parked: true });
    expect(undone?.park).toBe("Field Mulcher Extended");
    expect(undone?.body).toMatch(/^Most likely cause/);
  });

  it("no named mod: Find, except while an interrupted search waits to be restored", async () => {
    const { crashNoticeModel } = await import("./crashNotice");
    expect(crashNoticeModel({ ...base, culprit: null, live: null })?.find).toBe(true);
    const waiting = crashNoticeModel({ ...base, culprit: null, live: null, recoveryPending: true });
    expect(waiting?.find).toBe(false);
    expect(waiting?.body).toMatch(/interrupted search first/);
  });

  it("uses the library's current title", async () => {
    const { crashNoticeModel } = await import("./crashNotice");
    expect(crashNoticeModel({ ...base, live: { title: "Mulcher XL", active: true } })?.park).toBe("Mulcher XL");
  });
});

describe("ground-pile cap note (#339)", () => {
  it("names only what actually turned itself off", () => {
    const both = groundPileCapNote(63, ["Pumps N' Hoses", "Straw Harvest"]);
    expect(both.title).toBe("The game ran out of ground-pile slots.");
    expect(both.body).toContain("room for 63 ground-pile types");
    expect(both.body).toContain("and Pumps N' Hoses and Straw Harvest reported turning themselves off and closing the save");
    expect(groundPileCapNote(63, ["Straw Harvest"]).body).toContain("Straw Harvest reported turning itself off");
    const none = groundPileCapNote(63, []).body;
    expect(none).toContain("it rejected the ones that didn't fit");
    expect(none).not.toMatch(/Straw Harvest|Pumps/);
    expect(none).not.toMatch(/height type|densityMap|—/);
  });
});

describe("fill-type cap note (#341)", () => {
  it("says how many were skipped and names the first few", () => {
    const one = fillTypeCapNote(255, ["LIQUID_UREA"]);
    expect(one.title).toBe("The game ran out of material types.");
    expect(one.body).toContain("It supports 255");
    expect(one.body).toContain("skipped 1 material: LIQUID_UREA.");
    const many = fillTypeCapNote(255, ["A", "B", "C", "D", "E", "F", "G"]);
    expect(many.body).toContain("skipped 7 materials: A, B, C, D, E and 2 more.");
    expect(many.body).not.toMatch(/fill type|fillType|—/);
  });
});
