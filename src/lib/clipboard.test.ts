import { describe, expect, it } from "vitest";
import { copyOrSave } from "./clipboard";

describe("copyOrSave", () => {
  it("copies when the clipboard works", async () => {
    const saved: string[] = [];
    const out = await copyOrSave(
      "text",
      "silo-diagnostics.md",
      async () => {},
      async (name) => {
        saved.push(name);
        return true;
      },
    );
    expect(out).toBe("copied");
    expect(saved).toEqual([]);
  });

  it("saves to a file when the clipboard fails", async () => {
    const saved: [string, string][] = [];
    const out = await copyOrSave(
      "the bundle",
      "silo-diagnostics.md",
      async () => {
        throw new Error("no clipboard");
      },
      async (name, text) => {
        saved.push([name, text]);
        return true;
      },
    );
    expect(out).toBe("saved");
    expect(saved).toEqual([["silo-diagnostics.md", "the bundle"]]);
  });

  it("reports a cancelled save", async () => {
    const out = await copyOrSave(
      "t",
      "x.md",
      async () => {
        throw new Error("no clipboard");
      },
      async () => false,
    );
    expect(out).toBe("cancelled");
  });
});
