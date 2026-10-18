// The Library's selection model (#242): toggle, ranges from the anchor (Shift), select all,
// clear, and pruning keys that left the list.
import { describe, expect, it } from "vitest";
import { Selection } from "./selection.svelte";

const keys = ["a", "b", "c", "d", "e", "f"];
const sel = (s: Selection) => [...s.selected].sort();

describe("Selection", () => {
  it("toggles one key and makes it the anchor", () => {
    const s = new Selection();
    s.toggle("b");
    expect(sel(s)).toEqual(["b"]);
    expect(s.anchor).toBe("b");
    expect(s.has("b")).toBe(true);
    s.toggle("d");
    expect(sel(s)).toEqual(["b", "d"]);
    s.toggle("b");
    expect(sel(s)).toEqual(["d"]);
    expect(s.size).toBe(1);
  });

  it("selects a range from the anchor, in list order, either direction", () => {
    const s = new Selection();
    s.toggle("b");
    s.range("e", keys);
    expect(sel(s)).toEqual(["b", "c", "d", "e"]);
    // Moving back shrinks the range (Shift+↑), the anchor stays.
    s.range("c", keys);
    expect(sel(s)).toEqual(["b", "c"]);
    s.range("a", keys);
    expect(sel(s)).toEqual(["a", "b"]);
    expect(s.anchor).toBe("b");
  });

  it("adds a range to what was selected before the anchor was set", () => {
    const s = new Selection();
    s.toggle("a");
    s.toggle("d"); // anchor d, base {a, d}
    s.range("f", keys);
    expect(sel(s)).toEqual(["a", "d", "e", "f"]);
  });

  it("with no anchor, a range starts at `from` (Shift+arrow) or the key itself", () => {
    const s = new Selection();
    s.range("c", keys, "b");
    expect(sel(s)).toEqual(["b", "c"]);
    expect(s.anchor).toBe("b");
    const t = new Selection();
    t.range("c", keys);
    expect(sel(t)).toEqual(["c"]);
  });

  it("apply: Shift → range, plain or Ctrl → toggle", () => {
    const s = new Selection();
    s.apply("b", { shift: false, ctrl: false }, keys);
    s.apply("d", { shift: true, ctrl: false }, keys);
    expect(sel(s)).toEqual(["b", "c", "d"]);
    s.apply("c", { shift: false, ctrl: true }, keys);
    expect(sel(s)).toEqual(["b", "d"]);
    s.apply("e", { shift: true, ctrl: false, range: true, from: "d" }, keys);
    expect(sel(s)).toEqual(["b", "c", "d", "e"]);
  });

  it("selectAll and clear", () => {
    const s = new Selection();
    s.selectAll(keys);
    expect(s.size).toBe(6);
    expect(s.anchor).toBeNull();
    s.clear();
    expect(s.size).toBe(0);
  });

  it("prunes keys no longer in the list, and the anchor with them", () => {
    const s = new Selection();
    s.toggle("a");
    s.toggle("c");
    const before = s.selected;
    s.prune(["a", "b", "c"]);
    expect(s.selected).toBe(before); // nothing gone: no new snapshot
    s.prune(new Set(["a", "b"]));
    expect(sel(s)).toEqual(["a"]);
    expect(s.anchor).toBeNull(); // c was the anchor
    // A Shift gesture now starts a fresh range.
    s.range("b", ["a", "b"], "a");
    expect(sel(s)).toEqual(["a", "b"]);
  });
});
