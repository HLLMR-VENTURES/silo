import { describe, expect, it } from "vitest";
import { appendInfo } from "./info";

describe("appendInfo", () => {
  it("keeps what the banner already says", () => {
    expect(appendInfo("Silo restored its backup.", ["An update was finished."])).toBe(
      "Silo restored its backup. An update was finished.",
    );
  });

  it("starts the banner when it was empty", () => {
    expect(appendInfo(null, ["One.", "Two."])).toBe("One. Two.");
  });

  it("adds nothing for nothing, and never repeats a line", () => {
    expect(appendInfo(null, [])).toBeNull();
    expect(appendInfo("Same.", ["Same.", ""])).toBe("Same.");
  });
});
