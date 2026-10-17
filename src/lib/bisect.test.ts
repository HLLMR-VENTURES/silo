import { describe, expect, it } from "vitest";
import { bisectRestoreSet } from "./bisect";

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
