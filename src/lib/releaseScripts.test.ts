// The release workflow's checks (#359): scripts/check-release-version.mjs (the guard job's tag vs
// app version check) and scripts/release-assets.mjs (build-recorded digests vs the draft's assets).
import { describe, expect, it } from "vitest";
import {
  cargoPackageVersion,
  jsonVersion,
  readVersionFiles,
  releaseVersionProblems,
} from "../../scripts/check-release-version.mjs";
import {
  digestLines,
  parseDigests,
  releaseAssetProblems,
  uploadName,
} from "../../scripts/release-assets.mjs";

const files = (v: { pkg?: string; cargo?: string; conf?: string }) => ({
  packageJson: JSON.stringify({ name: "silo", version: v.pkg ?? "1.2.3" }),
  cargoToml: `[package]\nname = "silo"\nversion = "${v.cargo ?? "1.2.3"}"\n\n[dependencies]\nserde = { version = "1" }\n`,
  tauriConf: JSON.stringify({ productName: "Silo", version: v.conf ?? "1.2.3" }),
});

describe("check-release-version", () => {
  it("passes the repo's own version files for their tag", () => {
    const own = readVersionFiles();
    const version = jsonVersion(own.packageJson);
    expect(version).toMatch(/^\d+\.\d+\.\d+$/);
    expect(releaseVersionProblems({ tag: `v${version}`, ...own })).toEqual([]);
  });

  it("passes when all three agree and the tag is v<version>", () => {
    expect(releaseVersionProblems({ tag: "v1.2.3", ...files({}) })).toEqual([]);
  });

  it("refuses a tag that doesn't match the version", () => {
    const p = releaseVersionProblems({ tag: "v1.2.4", ...files({}) });
    expect(p).toHaveLength(1);
    expect(p[0]).toContain("doesn't match the app version 1.2.3");
  });

  it("refuses version files that disagree", () => {
    const p = releaseVersionProblems({ tag: "v1.2.3", ...files({ cargo: "1.2.2" }) });
    expect(p).toEqual([
      "The version files disagree: package.json=1.2.3, src-tauri/Cargo.toml=1.2.2, src-tauri/tauri.conf.json=1.2.3.",
    ]);
  });

  it("refuses pre-release tags and versions", () => {
    const p = releaseVersionProblems({
      tag: "v1.2.3-rc.1",
      ...files({ pkg: "1.2.3-rc.1", cargo: "1.2.3-rc.1", conf: "1.2.3-rc.1" }),
    });
    expect(p.some((s) => s.includes("has a '-'"))).toBe(true);
    expect(p.filter((s) => s.includes("isn't a plain X.Y.Z"))).toHaveLength(3);
  });

  it("refuses a tag without the v prefix", () => {
    expect(releaseVersionProblems({ tag: "1.2.3", ...files({}) }).length).toBeGreaterThan(0);
  });

  it("refuses a missing version, including tauri.conf.json pointing at package.json", () => {
    const p = releaseVersionProblems({ tag: "v1.2.3", ...files({}), tauriConf: "{}" });
    expect(p).toContain("src-tauri/tauri.conf.json has no version.");
    const q = releaseVersionProblems({ tag: "v1.2.3", ...files({ conf: "../package.json" }) });
    expect(q.some((s) => s.includes("isn't a plain X.Y.Z"))).toBe(true);
  });

  it("reads only the [package] version from Cargo.toml", () => {
    const toml = `[workspace]\nversion = "9.9.9"\n\n[package]\nname = "x"\nversion = "0.4.1" # bumped\n[dependencies]\nversion = "8"\n`;
    expect(cargoPackageVersion(toml)).toBe("0.4.1");
    expect(cargoPackageVersion(`[package]\nversion.workspace = true\n`)).toBeNull();
    expect(cargoPackageVersion(`[dependencies]\nversion = "1"\n`)).toBeNull();
  });

  it("reads a commented [package] header and a literal-string version", () => {
    expect(cargoPackageVersion(`[package] # app metadata\nversion = '1.2.3'\n`)).toBe("1.2.3");
    expect(cargoPackageVersion(`[ package ]\r\nversion="1.2.3"\r\n`)).toBe("1.2.3");
    expect(cargoPackageVersion(`[package.metadata]\nversion = "1.2.3"\n`)).toBeNull();
  });

  it("reads a JSON version only when it is a string", () => {
    expect(jsonVersion(`{"version":"1.0.0"}`)).toBe("1.0.0");
    expect(jsonVersion(`{"version":1}`)).toBeNull();
    expect(jsonVersion(`not json`)).toBeNull();
  });
});

const REPO = "HLLMR-VENTURES/silo";
const TAG = "v1.2.3";
const BASE = `https://github.com/${REPO}/releases/download/${TAG}/`;
const hex = (n: number) => n.toString(16).padStart(64, "0");

/** The updater bundles of a v1.2.3 release, by latest.json platform keys. */
const BUNDLES: [string, string[]][] = [
  ["Silo_aarch64.app.tar.gz", ["darwin-aarch64", "darwin-aarch64-app"]],
  ["Silo_x64.app.tar.gz", ["darwin-x86_64", "darwin-x86_64-app"]],
  ["Silo_1.2.3_amd64.AppImage", ["linux-x86_64", "linux-x86_64-appimage"]],
  ["Silo_1.2.3_amd64.deb", ["linux-x86_64-deb"]],
  ["Silo-1.2.3-1.x86_64.rpm", ["linux-x86_64-rpm"]],
  ["Silo_1.2.3_x64_en-US.msi", ["windows-x86_64", "windows-x86_64-msi"]],
  ["Silo_1.2.3_x64-setup.exe", ["windows-x86_64-nsis"]],
];

/** A whole release as the build jobs recorded it and the draft holds it, with its latest.json. */
function release() {
  const names = [
    ...BUNDLES.flatMap(([n]) => [n, `${n}.sig`]),
    "Silo_1.2.3_aarch64.dmg",
    "Silo_1.2.3_x64.dmg",
    "silo-sbom.cdx.json",
  ];
  const recorded = names.map((name, i) => ({ digest: hex(i + 1), name }));
  const assets: Record<string, string> = Object.fromEntries(recorded.map((r) => [r.name, r.digest]));
  assets["latest.json"] = hex(999);
  const sigText: Record<string, string> = Object.fromEntries(
    BUNDLES.map(([n]) => [`${n}.sig`, `sig of ${n}`]),
  );
  const platforms: Record<string, { url: string; signature: string }> = {};
  for (const [n, keys] of BUNDLES) {
    for (const k of keys) platforms[k] = { url: BASE + n, signature: `sig of ${n}` };
  }
  const manifest = { version: "1.2.3", platforms };
  return { recorded, assets, sigText, manifest, repo: REPO, tag: TAG };
}

const check = (r: ReturnType<typeof release>) =>
  releaseAssetProblems({ ...r, latestJson: JSON.stringify(r.manifest) });

describe("release-assets", () => {
  it("passes a release whose assets are exactly the recorded build outputs", () => {
    expect(check(release())).toEqual([]);
  });

  it("fails an asset swapped on the draft", () => {
    const r = release();
    r.assets["Silo_1.2.3_amd64.AppImage"] = hex(500);
    const p = check(r);
    expect(p.some((s) => s.startsWith("Silo_1.2.3_amd64.AppImage (sha256") && s.includes("isn't what the build recorded"))).toBe(true);
    expect(p).toContain("latest.json linux-x86_64: Silo_1.2.3_amd64.AppImage isn't a verified asset.");
  });

  it("fails recorded bytes uploaded under another file's name", () => {
    const r = release();
    r.assets["silo-sbom.cdx.json"] = r.assets["Silo_1.2.3_amd64.deb"];
    expect(check(r).some((s) => s.startsWith("silo-sbom.cdx.json (sha256"))).toBe(true);
  });

  it("fails the two macOS builds swapped, signatures and latest.json included", () => {
    const r = release();
    const a = "Silo_aarch64.app.tar.gz";
    const x = "Silo_x64.app.tar.gz";
    [r.assets[a], r.assets[x]] = [r.assets[x], r.assets[a]];
    [r.assets[`${a}.sig`], r.assets[`${x}.sig`]] = [r.assets[`${x}.sig`], r.assets[`${a}.sig`]];
    [r.sigText[`${a}.sig`], r.sigText[`${x}.sig`]] = [r.sigText[`${x}.sig`], r.sigText[`${a}.sig`]];
    for (const k of ["darwin-aarch64", "darwin-aarch64-app"]) r.manifest.platforms[k].signature = `sig of ${x}`;
    for (const k of ["darwin-x86_64", "darwin-x86_64-app"]) r.manifest.platforms[k].signature = `sig of ${a}`;
    const p = check(r);
    expect(p.filter((s) => s.includes("isn't what the build recorded"))).toHaveLength(4);
  });

  it("fails an asset nobody recorded and a recorded file missing from the release", () => {
    const r = release();
    r.assets["extra.zip"] = hex(700);
    delete r.assets["silo-sbom.cdx.json"];
    const p = check(r);
    expect(p).toContain(`extra.zip (sha256 ${hex(700)}) isn't a file the build jobs recorded.`);
    expect(p.some((s) => s.startsWith("Recorded build output silo-sbom.cdx.json"))).toBe(true);
  });

  it("fails when no digests were recorded, or one name was recorded twice", () => {
    const r = release();
    r.recorded = [];
    expect(check(r)).toContain("No recorded build digests were found.");
    const t = release();
    t.recorded.push({ ...t.recorded[0] });
    expect(check(t)).toContain(`${t.recorded[0].name} was recorded twice.`);
  });

  it("checks latest.json's version, urls and signatures", () => {
    const r = release();
    r.manifest.version = "1.2.2";
    r.manifest.platforms["linux-x86_64-deb"].signature = "forged";
    r.manifest.platforms["darwin-aarch64"].url = "https://example.com/Silo_aarch64.app.tar.gz";
    const p = check(r);
    expect(p).toContain("latest.json version '1.2.2' doesn't match tag v1.2.3.");
    expect(p).toContain("latest.json linux-x86_64-deb: signature doesn't match the verified Silo_1.2.3_amd64.deb.sig.");
    expect(p.some((s) => s.startsWith("latest.json darwin-aarch64: url"))).toBe(true);
  });

  it("fails latest.json with a platform missing, an unknown one, or another platform's build", () => {
    const r = release();
    delete r.manifest.platforms["darwin-x86_64"];
    r.manifest.platforms["linux-aarch64"] = { url: `${BASE}Silo_1.2.3_amd64.deb`, signature: "sig of Silo_1.2.3_amd64.deb" };
    r.manifest.platforms["darwin-aarch64"] = {
      url: `${BASE}Silo_1.2.3_amd64.AppImage`,
      signature: "sig of Silo_1.2.3_amd64.AppImage",
    };
    const p = check(r);
    expect(p).toContain("latest.json has no darwin-x86_64 entry.");
    expect(p).toContain("latest.json lists linux-aarch64, which this release doesn't build.");
    expect(p).toContain("latest.json darwin-aarch64: Silo_1.2.3_amd64.AppImage isn't the darwin-aarch64 build.");
  });

  it("fails a missing latest.json", () => {
    expect(releaseAssetProblems({ ...release(), latestJson: null })).toEqual(["The release has no latest.json."]);
  });

  it("names files as tauri-action uploads them", () => {
    expect(uploadName("Silo_1.2.3_x64-setup.exe", "")).toBe("Silo_1.2.3_x64-setup.exe");
    expect(uploadName("Silo_1.2.3_aarch64.dmg", "aarch64-apple-darwin")).toBe("Silo_1.2.3_aarch64.dmg");
    expect(uploadName("Silo.app.tar.gz", "x86_64-apple-darwin")).toBe("Silo_x64.app.tar.gz");
    expect(uploadName("Silo.app.tar.gz.sig", "aarch64-apple-darwin")).toBe("Silo_aarch64.app.tar.gz.sig");
    expect(() => uploadName("Silo.app.tar.gz", "")).toThrow(/No macOS arch/);
    expect(() => uploadName("Silo.app.tar.gz", "toString")).toThrow(/No macOS arch/);
  });

  it("parses sha256sum lines, CRLF included, and rejects malformed ones", () => {
    expect(parseDigests(`${hex(10)}  a.exe\r\n${hex(11)} *b.msi\r\n\r\n`)).toEqual([
      { digest: hex(10), name: "a.exe" },
      { digest: hex(11), name: "b.msi" },
    ]);
    expect(() => parseDigests("nothex  a.exe")).toThrow(/Malformed/);
  });

  it("records files under their upload names, skips directories, and refuses an empty build", () => {
    const hash = (p: string) => (p.endsWith(".dmg") ? hex(1) : hex(2));
    const fileCheck = (p: string) => !p.endsWith(".app");
    const opts = { rustTarget: "x86_64-apple-darwin", hash, fileCheck };
    const paths = ["/b/macos/Silo.app", "/b/macos/Silo.app.tar.gz", "/b/dmg/Silo_1.2.3_x64.dmg"];
    expect(digestLines(paths, opts)).toEqual([`${hex(2)}  Silo_x64.app.tar.gz`, `${hex(1)}  Silo_1.2.3_x64.dmg`]);
    expect(() => digestLines(["/b/macos/Silo.app"], opts)).toThrow(/No build output/);
    expect(() => digestLines([], opts)).toThrow(/No build output/);
  });

  it("fails on a build output path that doesn't exist", () => {
    expect(() => digestLines(["no/such/file.exe"])).toThrow();
  });
});
