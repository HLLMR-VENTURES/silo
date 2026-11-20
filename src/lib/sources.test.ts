import { describe, expect, it } from "vitest";
import { checkYourselfList, sourceHost } from "./sources";
import type { SourceRow, UpdateRow } from "./types";

const mod = (techName: string, title?: string) => ({ techName, title: title ?? null });
const src = (techName: string, url: string, lastChecked: number | null = null): SourceRow => ({
  techName,
  url,
  lastChecked,
});
const auto = (techName: string, extra: Partial<UpdateRow> = {}): UpdateRow => ({
  techName,
  title: techName,
  path: `/mods/${techName}.zip`,
  ...extra,
});

describe("sourceHost", () => {
  it("shows the site name without www", () => {
    expect(sourceHost("https://www.example.com/mods/a")).toBe("example.com");
    expect(sourceHost("https://kingmods.net/en/fs25/a")).toBe("kingmods.net");
  });
  it("falls back to the text when it is not an address", () => {
    expect(sourceHost("junk")).toBe("junk");
  });
});

describe("checkYourselfList", () => {
  it("lists mods with a source page and no automatic answer", () => {
    const list = checkYourselfList(
      [mod("A", "Alpha"), mod("B", "Bravo"), mod("C", "Charlie")],
      { A: src("A", "https://a.example/x"), B: src("B", "https://b.example/y") },
      [auto("B", { hasUpdate: false, latest: "1.0" })],
    );
    expect(list).toEqual([
      { techName: "A", title: "Alpha", url: "https://a.example/x", host: "a.example", lastChecked: null, version: null },
    ]);
  });

  it("carries the version you have, so the page can be compared with it (#457)", () => {
    const list = checkYourselfList(
      [{ techName: "A", title: "Alpha", version: " 1.2.0.0 " }, { techName: "B", title: "Bravo", version: "" }],
      { A: src("A", "https://a.example/x"), B: src("B", "https://b.example/y") },
      [],
    );
    expect(list.map((r) => r.version)).toEqual(["1.2.0.0", null]);
  });

  it("lists a mod with more than one copy once, with the copy the update check reads", () => {
    const pages = { A: src("A", "https://a.example/x") };
    const one = (version: string | null) => ({ techName: "A", title: "Alpha", version });
    expect(checkYourselfList([one("1.0"), one("2.0")], pages, [])).toMatchObject([{ techName: "A", version: "2.0" }]);
    expect(checkYourselfList([one("2.0"), one("1.0")], pages, [])).toMatchObject([{ techName: "A", version: "1.0" }]);
    // The chosen copy names no version: none is borrowed from the other copy.
    expect(checkYourselfList([one("2.0"), one(null)], pages, [])).toMatchObject([{ techName: "A", version: null }]);
  });

  it("counts a failed check with no catalog match as no answer", () => {
    const list = checkYourselfList(
      [mod("A")],
      { A: src("A", "https://a.example/x") },
      [auto("A", { error: "rate limited" })],
    );
    expect(list.map((r) => r.techName)).toEqual(["A"]);
  });

  it("counts an update found as an answer", () => {
    const list = checkYourselfList(
      [mod("A")],
      { A: src("A", "https://a.example/x") },
      [auto("A", { hasUpdate: true, latest: "2.0" })],
    );
    expect(list).toEqual([]);
  });

  it("skips saved pages for mods no longer in the library", () => {
    expect(checkYourselfList([], { A: src("A", "https://a.example/x") }, [])).toEqual([]);
  });

  it("puts never-checked first, then the oldest check, then by title", () => {
    const list = checkYourselfList(
      [mod("A", "Alpha"), mod("B", "Bravo"), mod("C", "Charlie"), mod("D", "Delta")],
      {
        A: src("A", "https://a.example/", 300),
        B: src("B", "https://b.example/", 100),
        C: src("C", "https://c.example/", null),
        D: src("D", "https://d.example/", 100),
      },
      [],
    );
    expect(list.map((r) => r.techName)).toEqual(["C", "B", "D", "A"]);
  });

  it("uses the tech name when a mod has no title", () => {
    const list = checkYourselfList([mod("FS25_X")], { FS25_X: src("FS25_X", "https://x.example/") }, []);
    expect(list[0].title).toBe("FS25_X");
  });
});
