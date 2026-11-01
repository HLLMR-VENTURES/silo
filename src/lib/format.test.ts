import { describe, expect, it } from "vitest";
import { displayPath, displayVersion, modSizeLabel } from "./format";

describe("displayVersion (C2)", () => {
  it("strips a leading v, V or V. and never adds one", () => {
    expect(displayVersion("1.6.4.0")).toBe("1.6.4.0");
    expect(displayVersion("v1.6.4.0")).toBe("1.6.4.0");
    expect(displayVersion("V1.6.4.0")).toBe("1.6.4.0");
    expect(displayVersion("V.1.6.4.0")).toBe("1.6.4.0");
    expect(displayVersion("  v 2.0 ")).toBe("2.0");
    expect(displayVersion("Ver 2.1")).toBe("2.1");
    expect(displayVersion("version.3")).toBe("3");
  });
  it("leaves a version that doesn't start with v and a number alone", () => {
    expect(displayVersion("vanilla-2")).toBe("vanilla-2");
    expect(displayVersion("beta 3")).toBe("beta 3");
  });
});

describe("displayPath (#385)", () => {
  it("gives a Windows path backslashes throughout and leaves others alone", () => {
    expect(
      displayPath("E:/SteamLibrary\\steamapps/common\\Farming Simulator 25"),
    ).toBe("E:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25");
    expect(displayPath("C:")).toBe("C:");
    expect(displayPath("//nas/share/FS25")).toBe("\\\\nas\\share\\FS25");
    expect(
      displayPath("/Users/me/Library/Application Support/FarmingSimulator2025"),
    ).toBe("/Users/me/Library/Application Support/FarmingSimulator2025");
  });
});

describe("modSizeLabel (C5)", () => {
  it("shows a zip's size", () => {
    expect(modSizeLabel({ kind: "zip", size: 2048 })).toBe("2 KB");
  });
  it("leaves out a folder mod's size when it wasn't measured", () => {
    expect(modSizeLabel({ kind: "dir", size: 0 })).toBeNull();
  });
});

describe("Updates row versions (C2)", () => {
  it("shows the catalog's V. tag as a plain number and compares it as one", async () => {
    const { versionChange, isNewerVersion } = await import("./updates/sections");
    expect(versionChange({ current: "1.5.0.0", latest: "V.1.6.4.0" })).toBe("1.5.0.0 → 1.6.4.0");
    expect(isNewerVersion("V.1.6.4.0", "1.5.0.0")).toBe(true);
  });
});
