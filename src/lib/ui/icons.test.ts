import { describe, expect, it } from "vitest";
import { ICONS } from "./icons";

describe("ICONS", () => {
  it("maps every concept to a defined Lucide component", () => {
    const missing = Object.entries(ICONS)
      .filter(([, cmp]) => typeof cmp !== "function" && (typeof cmp !== "object" || cmp === null))
      .map(([key]) => key);
    expect(missing).toEqual([]);
  });

  it("has at least one icon", () => {
    expect(Object.keys(ICONS).length).toBeGreaterThan(0);
  });
});
