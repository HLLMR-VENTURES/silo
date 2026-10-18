// tileGrid: columns for the Library card grid at the usual and wide content caps.
import { describe, expect, it } from "vitest";
import { tileColumns } from "./tileGrid";

describe("tileColumns", () => {
  it("keeps two columns up to the regular 1280px content cap", () => {
    expect(tileColumns(616)).toBe(2); // a 720px window with the rail collapsed
    expect(tileColumns(1240)).toBe(2); // 1280px cap minus padding
  });

  it("opens more columns on a wide window instead of stretching tiles", () => {
    expect(tileColumns(1880)).toBe(3); // the wide 1920px cap minus padding
    expect(tileColumns(1400)).toBe(3);
  });

  it("drops to one column when the list is narrow (a docked drawer on a small window)", () => {
    expect(tileColumns(599)).toBe(1);
    expect(tileColumns(320)).toBe(1);
  });

  it("uses the two-column default before the list is measured", () => {
    expect(tileColumns(0)).toBe(2);
    expect(tileColumns(Number.NaN)).toBe(2);
  });
});
