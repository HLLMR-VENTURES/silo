import { describe, expect, it } from "vitest";
import { currentEntry } from "./drawerMod";

type E = { path: string; techName: string; category: string; organized: boolean };
const entry = (path: string, techName: string, category = "Tractors", organized = false): E => ({
  path,
  techName,
  category,
  organized,
});

describe("currentEntry (the drawer follows the library)", () => {
  it("is null while the drawer is closed", () => {
    expect(currentEntry(null, [entry("/m/a.zip", "a")])).toBeNull();
  });

  it("picks up a category override (a new entry for the same file)", () => {
    const shown = entry("/m/a.zip", "a", "Tractors");
    const now = [entry("/m/a.zip", "a", "Trailers")];
    expect(currentEntry(shown, now)?.category).toBe("Trailers");
  });

  it("follows a mod that filing moved to the archive", () => {
    const shown = entry("/mods/a.zip", "a");
    const now = [entry("/archive/Tractors/a.zip", "a", "Tractors", true)];
    const got = currentEntry(shown, now);
    expect(got?.path).toBe("/archive/Tractors/a.zip");
    expect(got?.organized).toBe(true);
  });

  it("prefers the same path when two entries share a tech name", () => {
    const shown = entry("/mods/b.zip", "a");
    const now = [entry("/mods/a.zip", "a"), entry("/mods/b.zip", "a")];
    expect(currentEntry(shown, now)?.path).toBe("/mods/b.zip");
  });

  it("keeps the shown entry when the mod is gone", () => {
    const shown = entry("/mods/a.zip", "a");
    expect(currentEntry(shown, [entry("/mods/z.zip", "z")])).toBe(shown);
  });
});
