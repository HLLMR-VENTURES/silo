import { describe, expect, it } from "vitest";
import { formatCount, plural } from "./plural";

describe("plural / formatCount", () => {
  it.each([
    [0, "mods"],
    [1, "mod"],
    [2, "mods"],
    [1.5, "mods"],
  ])("%d -> %s", (n, word) => expect(plural(n, { one: "mod", other: "mods" })).toBe(word));
  it("groups digits", () => {
    expect(formatCount(1204)).toBe("1,204");
    expect(formatCount(7)).toBe("7");
  });
});
