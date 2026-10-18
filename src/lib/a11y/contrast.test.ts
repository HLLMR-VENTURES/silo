// The contrast guard behind `npm run check` (scripts/check-contrast.mjs, #258): the WCAG maths,
// token resolution (var() and the color-mix soft tints), the real tokens.css passing, and a
// lowered token failing with a readable line.
import { describe, expect, it } from "vitest";
import { check, contrast, parseHex, readTokens, resolve, themes } from "../../../scripts/check-contrast.mjs";

const css = readTokens();

describe("check-contrast", () => {
  it("computes WCAG ratios", () => {
    expect(contrast([0, 0, 0], [255, 255, 255])).toBeCloseTo(21, 5);
    expect(contrast(parseHex("#777")!, parseHex("#fff")!)).toBeCloseTo(4.48, 2);
    expect(parseHex("#abc")).toEqual([0xaa, 0xbb, 0xcc]);
  });

  it("resolves var() and the color-mix soft tints", () => {
    const vars = { "--a": "#ffffff", "--b": "#000000", "--c": "var(--a)", "--mix": "color-mix(in srgb, var(--b) 50%, var(--c))" };
    expect(resolve("--c", vars)).toEqual([255, 255, 255]);
    expect(resolve("--mix", vars)).toEqual([128, 128, 128]);
  });

  it("finds the light theme and both dark blocks", () => {
    const t = themes(css);
    expect(Object.keys(t)).toEqual(["light", "dark (system)", "dark (chosen)"]);
    expect(t.light["--bg"]).toBe("#f6f3ec");
    expect(t["dark (chosen)"]["--bg"]).toBe("#171a14");
  });

  it("passes on the real tokens.css", () => {
    expect(check(css).failures).toEqual([]);
  });

  it("fails a muted text token that drops below 4.5:1", () => {
    const worse = css.replace("--muted: #5e5d50;", "--muted: #8a8a80;");
    const { failures } = check(worse);
    expect(failures.some((f) => f.startsWith("light: --muted on --bg"))).toBe(true);
  });

  it("fails when the two dark blocks drift apart", () => {
    const i = css.indexOf(':root[data-theme="dark"]');
    const drifted = css.slice(0, i) + css.slice(i).replace("--danger: #ee9481;", "--danger: #ee9482;");
    expect(check(drifted).failures).toContain(
      "dark themes differ: --danger is #ee9481 (system) but #ee9482 (chosen)",
    );
  });
});
