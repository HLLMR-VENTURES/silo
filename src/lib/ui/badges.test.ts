import { describe, expect, it } from "vitest";
import { capBadges } from "./badges";

describe("capBadges", () => {
  it("shows everything when under the cap", () => {
    expect(capBadges(["a", "b"])).toEqual({ shown: ["a", "b"], more: 0 });
  });

  it("shows exactly the cap with nothing more", () => {
    expect(capBadges(["a", "b", "c"])).toEqual({ shown: ["a", "b", "c"], more: 0 });
  });

  it("caps at 3 by default and counts the rest", () => {
    expect(capBadges([1, 2, 3, 4, 5])).toEqual({ shown: [1, 2, 3], more: 2 });
  });

  it("honours a custom max", () => {
    expect(capBadges([1, 2, 3, 4], 1)).toEqual({ shown: [1], more: 3 });
  });

  it("treats a negative max as zero", () => {
    expect(capBadges([1, 2], -1)).toEqual({ shown: [], more: 2 });
  });

  it("handles an empty list", () => {
    expect(capBadges([])).toEqual({ shown: [], more: 0 });
  });
});
