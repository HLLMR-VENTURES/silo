// Browse card rules (#246): spelled-out source chips, the install state behind a card's button,
// the badge rules (Verified author only with a claim, never styled like Verified build), the
// catalog error copy and the empty-results title, and columns from width.
import { describe, expect, it, vi } from "vitest";

vi.mock("./api", () => ({ catalogImage: vi.fn() }));

import {
  browseBadges,
  browseColumns,
  catalogErrorCopy,
  emptyResultsTitle,
  installLabel,
  installState,
  label,
  progressPct,
  sourceChip,
  verifiedAuthorLogin,
} from "./browse";
import { AppError } from "./errors";
import type { BrowseMod, ModSourceOption } from "./types";

const gh: ModSourceOption = {
  source: "github",
  sourceUrl: "https://github.com/o/r",
  version: "1.0.6.0",
  installable: true,
  downloadUrl: "https://github.com/o/r/releases/download/v1/FS25_Lizard.zip",
};
const mh: ModSourceOption = {
  source: "modhub",
  sourceUrl: "https://www.farming-simulator.com/mod.php?mod_id=1",
  version: "1.0.5.0",
  installable: false,
  downloadUrl: null,
};

function mod(over: Partial<BrowseMod> = {}): BrowseMod {
  return {
    id: "m1",
    techName: "FS25_Lizard",
    title: "Lizard R-Series",
    author: "Lizard Modding",
    category: null,
    description: null,
    imageUrl: null,
    latestVersion: "1.0.6.0",
    trustScore: null,
    updatedAt: null,
    sources: [gh, mh],
    pageUrl: null,
    ...over,
  };
}

const none = new Set<string>();
const mine = new Set(["FS25_Lizard"]);

describe("source chips", () => {
  it("spell the source out, with that source's version", () => {
    expect(sourceChip(gh)).toBe("GitHub 1.0.6.0");
    expect(sourceChip(mh)).toBe("ModHub 1.0.5.0");
  });

  it("never use the old two-letter codes", () => {
    for (const s of [gh, mh]) expect(sourceChip(s)).not.toMatch(/\b(GH|MH)\b/);
  });

  it("show just the name without a version, and an unknown source as it came", () => {
    expect(sourceChip({ source: "github", version: null })).toBe("GitHub");
    expect(sourceChip({ source: "github", version: "  " })).toBe("GitHub");
    expect(label("curseforge")).toBe("curseforge");
  });
});

describe("installState", () => {
  it("offers Install from the first source Silo can install from", () => {
    const st = installState(mod({ sources: [mh, gh] }), { installed: none, installingId: null });
    expect(st).toEqual({ kind: "install", source: gh });
    expect(installLabel(st)).toBe("Install");
  });

  it("opens ModHub for an index-only mod", () => {
    const st = installState(mod({ sources: [mh] }), { installed: none, installingId: null });
    expect(st).toEqual({ kind: "open", source: mh });
    expect(installLabel(st)).toBe("Open on ModHub");
  });

  it("shows progress while this mod installs, and only for this mod", () => {
    const busy = { installed: none, installingId: "m1", progress: { done: 42, total: 100 } };
    const st = installState(mod(), busy);
    expect(st).toEqual({ kind: "installing", pct: 42 });
    expect(installLabel(st)).toBe("Installing… 42%");
    expect(installLabel(installState(mod(), { ...busy, progress: { done: 5, total: null } }))).toBe(
      "Installing…",
    );
    expect(installState(mod({ id: "m2" }), busy).kind).toBe("install");
  });

  it("says Show in library once installed, and See update when a newer version is known", () => {
    expect(installLabel(installState(mod(), { installed: mine, installingId: null }))).toBe(
      "Show in library",
    );
    const st = installState(mod(), { installed: mine, updates: mine, installingId: null });
    expect(st.kind).toBe("update");
    expect(installLabel(st)).toBe("See update");
  });

  it("ignores a known update for a mod that isn't installed", () => {
    expect(installState(mod(), { installed: none, updates: mine, installingId: null }).kind).toBe("install");
  });

  it("has nothing to do for a mod with no sources", () => {
    const st = installState(mod({ sources: [] }), { installed: none, installingId: null });
    expect(st.kind).toBe("none");
  });

  it("clamps progress and needs a total", () => {
    expect(progressPct(undefined)).toBeNull();
    expect(progressPct({ done: 10, total: 0 })).toBeNull();
    expect(progressPct({ done: 150, total: 100 })).toBe(100);
    expect(progressPct({ done: 1, total: 3 })).toBe(33);
  });
});

describe("browseBadges", () => {
  const ids = (m: BrowseMod, installed = none, updates?: Set<string>) =>
    browseBadges(m, { installed, updates }).map((b) => b.id);

  it("shows nothing for a plain catalog mod", () => {
    expect(ids(mod())).toEqual([]);
  });

  it("marks a mod you have, without dimming it, and a known update", () => {
    expect(ids(mod(), mine)).toEqual(["in-library"]);
    expect(ids(mod(), mine, mine)).toEqual(["in-library", "update"]);
  });

  it("shows Verified build only for a hashed build", () => {
    expect(ids(mod({ verified: true }))).toEqual(["verified-build"]);
    expect(ids(mod({ verified: false }))).toEqual([]);
    expect(ids(mod({ verified: null }))).toEqual([]);
  });

  it("shows Verified author only when the catalog carries a claim", () => {
    expect(ids(mod({ verifiedAuthor: null }))).toEqual([]);
    expect(ids(mod({ verifiedAuthor: undefined }))).toEqual([]);
    expect(ids(mod({ verifiedAuthor: { login: " " } }))).toEqual([]);
    // A verified build alone is not a verified author.
    expect(ids(mod({ verified: true }))).not.toContain("verified-author");
    const claimed = browseBadges(mod({ verifiedAuthor: { login: "lizard" } }), { installed: none });
    expect(claimed.map((b) => b.id)).toEqual(["verified-author"]);
    expect(claimed[0].title).toContain("lizard");
    expect(verifiedAuthorLogin(mod({ verifiedAuthor: { login: "lizard" } }))).toBe("lizard");
  });

  it("keeps Verified build and Verified author apart: green shield vs sky-blue badge", () => {
    const [build, author] = browseBadges(mod({ verified: true, verifiedAuthor: { login: "lizard" } }), {
      installed: none,
    });
    expect(build).toMatchObject({ label: "Verified build", tone: "success", icon: "verifiedBuild" });
    expect(author).toMatchObject({ label: "Verified author", tone: "info", icon: "verifiedAuthor" });
    expect(build.tone).not.toBe(author.tone);
    expect(build.icon).not.toBe(author.icon);
  });
});

describe("catalog error and empty states", () => {
  it("reads network trouble as an unreachable catalog, and the library as fine", () => {
    for (const code of ["NET_OFFLINE", "NET_TIMEOUT"]) {
      const c = catalogErrorCopy(new AppError(code, "error sending request"));
      expect(c.title).toBe("Silo can't reach its catalog right now.");
      expect(c.body).toBe("Your library still works.");
      expect(c.details).toContain("error sending request");
    }
    expect(catalogErrorCopy("connection refused").title).toBe("Silo can't reach its catalog right now.");
  });

  it("keeps the words for a problem Silo knows", () => {
    const c = catalogErrorCopy(new AppError("GITHUB_RATE_LIMIT", "rate limited"));
    expect(c.title).not.toBe("Silo can't reach its catalog right now.");
    expect(c.title).toMatch(/GitHub/);
    expect(c.body).toBe("Your library still works.");
  });

  it("names the search that found nothing", () => {
    expect(emptyResultsTitle("lizard", false)).toBe("No mods match “lizard”.");
    expect(emptyResultsTitle("  lizard ", true)).toBe("No mods match “lizard”.");
    expect(emptyResultsTitle("", true)).toBe("No mods match these filters.");
  });
});

describe("browseColumns", () => {
  it("fits 232px cards with 16px gaps, never fewer than one", () => {
    expect(browseColumns(0)).toBe(4); // not measured yet
    expect(browseColumns(200)).toBe(1);
    expect(browseColumns(480)).toBe(2);
    expect(browseColumns(1240)).toBe(5);
    expect(browseColumns(1880)).toBe(7);
  });
});
