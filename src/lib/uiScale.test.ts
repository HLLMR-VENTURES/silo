import { describe, expect, it } from "vitest";
import { parseUiScale } from "./uiScale";

describe("parseUiScale", () => {
  it("defaults to 1 when nothing is stored", () => {
    expect(parseUiScale(null)).toBe(1);
  });

  it("accepts the allowed sizes", () => {
    expect(parseUiScale("1.1")).toBe(1.1);
    expect(parseUiScale("0.9")).toBe(0.9);
  });

  it("falls back to 1 for values outside the list", () => {
    expect(parseUiScale("2")).toBe(1);
  });

  it("falls back to 1 for garbage", () => {
    expect(parseUiScale("abc")).toBe(1);
  });
});
