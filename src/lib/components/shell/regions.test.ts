// @vitest-environment jsdom
// F6 region cycling order, wrapping, missing regions, and DOM lookup.
import { describe, expect, it } from "vitest";
import { nextRegion, presentRegions, regionOf, REGIONS } from "./regions";

describe("nextRegion", () => {
  it("cycles forward and backward through every region, wrapping", () => {
    expect(nextRegion("rail", 1)).toBe("header");
    expect(nextRegion("main", 1)).toBe("drawer");
    expect(nextRegion("drawer", 1)).toBe("rail");
    expect(nextRegion("rail", -1)).toBe("drawer");
    expect(nextRegion("header", -1)).toBe("rail");
  });

  it("skips regions that are not on screen", () => {
    const present = ["rail", "header", "main"] as const;
    expect(nextRegion("header", 1, present)).toBe("main");
    expect(nextRegion("main", 1, present)).toBe("rail");
    expect(nextRegion("rail", -1, present)).toBe("main");
  });

  it("visits regions in shell order whatever order they are passed in", () => {
    expect(nextRegion("rail", 1, ["main", "header", "rail"])).toBe("header");
  });

  it("from outside any region: forward goes to main, backward to the last", () => {
    expect(nextRegion(null, 1)).toBe("main");
    expect(nextRegion(null, -1)).toBe("drawer");
    expect(nextRegion(null, 1, ["rail", "header"])).toBe("rail");
    expect(nextRegion("drawer", 1, ["rail", "main"])).toBe("main");
  });

  it("returns null when nothing is present", () => {
    expect(nextRegion("main", 1, [])).toBeNull();
  });
});

describe("DOM helpers", () => {
  it("finds present regions and the region of a node", () => {
    document.body.innerHTML = `
      <main data-region="main"><button id="b">x</button></main>
      <nav data-region="rail"></nav>
      <div data-region="bogus"></div>`;
    expect(presentRegions(document)).toEqual(["rail", "main"]);
    expect(regionOf(document.getElementById("b"))).toBe("main");
    expect(regionOf(document.body)).toBeNull();
    expect(regionOf(document.querySelector("[data-region='bogus']"))).toBeNull();
    expect(REGIONS).toHaveLength(5);
    document.body.innerHTML = "";
  });
});
