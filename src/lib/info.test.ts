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

  it("adds a change's error after the rescan's own problems, which it doesn't wipe", () => {
    // applyLoose (#187): filing failed, then the rescan reported a problem of its own.
    expect(appendInfo("FS25_B.zip couldn't be read.", ["FS25_A.zip: access denied"])).toBe(
      "FS25_B.zip couldn't be read. FS25_A.zip: access denied",
    );
    // A clean rescan left the error banner empty.
    expect(appendInfo(null, ["FS25_A.zip: access denied"])).toBe("FS25_A.zip: access denied");
  });

  it("adds nothing for nothing, and never repeats a line", () => {
    expect(appendInfo(null, [])).toBeNull();
    expect(appendInfo("Same.", ["Same.", ""])).toBe("Same.");
  });
});
