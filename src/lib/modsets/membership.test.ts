// The "Mod sets…" dialog's pure logic (#323): tri-state membership, the click cycle, what Save
// writes, and what editing the set you're on does to your active mods.
import { describe, expect, it } from "vitest";
import type { Loadout } from "../types";
import {
  activeChange,
  currentSetNote,
  membershipEdits,
  membershipMessage,
  membershipOf,
  membershipRows,
  next,
  type Membership,
} from "./membership";

const set = (id: number, mods: string[], kind: Loadout["kind"] = "user"): Loadout => ({
  id,
  name: `Set ${id}`,
  mods,
  kind,
  createdAt: null,
});

describe("membershipOf", () => {
  it("is all, none or some", () => {
    expect(membershipOf(["A", "B", "C"], ["A", "B"])).toBe("all");
    expect(membershipOf(["C"], ["A", "B"])).toBe("none");
    expect(membershipOf(["A", "C"], ["A", "B"])).toBe("some");
    expect(membershipOf([], ["A"])).toBe("none");
  });
});

describe("next", () => {
  it("cycles ticked and unticked for a set that didn't open mixed", () => {
    expect(next("all", "all")).toBe("none");
    expect(next("none", "all")).toBe("all");
    expect(next("none", "none")).toBe("all");
  });

  it("goes mixed → ticked → unticked → mixed for a set that opened mixed", () => {
    let s = "some" as Membership;
    const seen = [s];
    for (let i = 0; i < 3; i++) seen.push((s = next(s, "some")));
    expect(seen).toEqual(["some", "all", "none", "some"]);
  });
});

describe("membershipRows", () => {
  it("lists only sets the user saved, in order, with their starting state", () => {
    const rows = membershipRows([set(1, ["A"]), set(2, ["A"], "auto"), set(3, [])], ["A"]);
    expect(rows.map((r) => [r.set.id, r.initial])).toEqual([
      [1, "all"],
      [3, "none"],
    ]);
  });
});

describe("membershipEdits", () => {
  const sets = [set(1, ["X", "A"]), set(2, ["X"]), set(3, ["A", "B", "X"]), set(4, ["B"])];
  const names = ["A", "B"];
  const rows = membershipRows(sets, names);

  it("adds the missing mods to a ticked set, keeping its order", () => {
    const edits = membershipEdits(rows, names, new Map([[1, "all"]]));
    expect(edits).toHaveLength(1);
    expect(edits[0]).toMatchObject({ mods: ["X", "A", "B"], added: ["B"], removed: [] });
  });

  it("removes the chosen mods from an unticked set and nothing else", () => {
    const edits = membershipEdits(rows, names, new Map([[3, "none"]]));
    expect(edits[0]).toMatchObject({ mods: ["X"], added: [], removed: ["A", "B"] });
  });

  it("leaves unchanged, mixed and put-back sets alone", () => {
    const chosen = new Map<number, Membership>([
      [1, "some"], // back to how it opened
      [2, "none"], // as it opened
      [3, "all"],
    ]);
    expect(membershipEdits(rows, names, chosen)).toEqual([]);
    expect(membershipEdits(rows, names, new Map())).toEqual([]);
  });

  it("writes every changed set at once", () => {
    const edits = membershipEdits(
      rows,
      names,
      new Map<number, Membership>([
        [2, "all"],
        [4, "none"],
      ]),
    );
    expect(edits.map((e) => [e.set.id, e.mods])).toEqual([
      [2, ["X", "A", "B"]],
      [4, []],
    ]);
  });
});

describe("activeChange", () => {
  const sets = [set(1, ["A"]), set(2, ["B"])];
  const rows = membershipRows(sets, ["A", "B"]);

  it("turns on what the set you're on gains and parks what it loses", () => {
    const add = membershipEdits(rows, ["A", "B"], new Map([[1, "all"]]));
    expect(activeChange(add, 1, new Set(["A"]))).toEqual({ on: ["B"], off: [] });
    const drop = membershipEdits(rows, ["A", "B"], new Map([[1, "none"]]));
    expect(activeChange(drop, 1, new Set(["A", "B"]))).toEqual({ on: [], off: ["A"] });
  });

  it("does nothing when another set changes, or you're on none", () => {
    const edits = membershipEdits(rows, ["A", "B"], new Map([[2, "none"]]));
    expect(activeChange(edits, 1, new Set(["B"]))).toEqual({ on: [], off: [] });
    expect(activeChange(edits, null, new Set(["B"]))).toEqual({ on: [], off: [] });
  });
});

describe("currentSetNote", () => {
  it("says what changes now, and nothing when your mods are already that way", () => {
    expect(currentSetNote("all", "none", ["A", "B"], new Set())).toBe("also turns them on now");
    expect(currentSetNote("none", "all", ["A"], new Set(["A"]))).toBe("also parks it now");
    expect(currentSetNote("all", "some", ["A"], new Set(["A"]))).toBe("");
    expect(currentSetNote("none", "some", ["A", "B"], new Set())).toBe("");
    expect(currentSetNote("some", "some", ["A", "B"], new Set())).toBe("");
  });
});

describe("membershipMessage", () => {
  const rows = membershipRows([set(1, []), set(2, []), set(3, ["A", "B"])], ["A", "B"]);
  const edits = (chosen: [number, Membership][]) => membershipEdits(rows, ["A", "B"], new Map(chosen));

  it("names one mod and one set, and counts more", () => {
    const one = membershipEdits(membershipRows([set(1, [])], ["A"]), ["A"], new Map([[1, "all"]]));
    expect(membershipMessage(["Courseplay"], one)).toBe("Added Courseplay to Set 1.");
    expect(membershipMessage(["A", "B"], edits([[1, "all"], [2, "all"]]))).toBe("Added 2 mods to 2 sets.");
    expect(membershipMessage(["A", "B"], edits([[3, "none"]]))).toBe("Removed 2 mods from Set 3.");
  });

  it("says both when Save added and removed", () => {
    expect(membershipMessage(["A", "B"], edits([[1, "all"], [3, "none"]]))).toBe(
      "Added 2 mods to Set 1 and removed them from Set 3.",
    );
  });
});
