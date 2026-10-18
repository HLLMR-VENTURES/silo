// Help (#250): topic search (with aliases) and the third-party licenses list.
import { describe, expect, it } from "vitest";
import { HELP_TOPICS, searchTopics } from "./topics";
import { THIRD_PARTY } from "./licenses";
import pkg from "../../../package.json";
import frauncesFile from "../../assets/fonts/OFL-Fraunces.txt?raw";
import hankenFile from "../../assets/fonts/OFL-HankenGrotesk.txt?raw";

describe("searchTopics", () => {
  it("finds Mod sets by its old name", () => {
    expect(searchTopics("loadouts").map((t) => t.id)).toEqual(["mod-sets"]);
    expect(searchTopics("Loadout").map((t) => t.id)).toEqual(["mod-sets"]);
  });

  it("matches every word, in the title, text or aliases", () => {
    expect(searchTopics("undo organizing").map((t) => t.id)).toContain("undo-organizing");
    expect(searchTopics("crash log").map((t) => t.id)).toEqual(["crash"]);
    expect(searchTopics("nothing-like-this")).toEqual([]);
  });

  it("an empty query lists every topic", () => {
    expect(searchTopics("  ")).toEqual(HELP_TOPICS);
  });

  it("topic ids are unique and copy follows the guide", () => {
    expect(new Set(HELP_TOPICS.map((t) => t.id)).size).toBe(HELP_TOPICS.length);
    for (const t of HELP_TOPICS) {
      expect(t.body).not.toMatch(/—|\bbeta\b|loadout/i);
      expect(t.title[0]).toBe(t.title[0].toUpperCase());
    }
  });
});

describe("THIRD_PARTY", () => {
  const byName = new Map(THIRD_PARTY.map((l) => [l.name, l]));

  it("includes both fonts with their OFL texts", () => {
    for (const [name, file] of [
      ["Fraunces (font)", frauncesFile],
      ["Hanken Grotesk (font)", hankenFile],
    ]) {
      const l = byName.get(name);
      expect(l?.license).toBe("OFL-1.1");
      expect(l?.text).toBe(file.trim());
      expect(file).toMatch(/Copyright 20\d\d The .* Project Authors/);
      expect(l?.text).toMatch(/SIL OPEN FONT LICENSE/i);
    }
  });

  it("includes Lucide under ISC, with its text", () => {
    const l = byName.get("@lucide/svelte");
    expect(l?.license).toBe("ISC");
    expect(l?.text).toMatch(/ISC License/i);
  });

  it("lists every runtime dependency, each with a license text", () => {
    for (const name of Object.keys(pkg.dependencies)) expect(byName.has(name), name).toBe(true);
    for (const l of THIRD_PARTY) expect(l.text.length, l.name).toBeGreaterThan(100);
  });
});
