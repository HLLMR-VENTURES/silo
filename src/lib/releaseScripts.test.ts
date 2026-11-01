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
  uploadNameMatches,
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

  it("reads a JSON version only when it is a string", () => {
    expect(jsonVersion(`{"version":"1.0.0"}`)).toBe("1.0.0");
    expect(jsonVersion(`{"version":1}`)).toBeNull();
    expect(jsonVersion(`not json`)).toBeNull();
  });
});

const REPO = "HLLMR-VENTURES/silo";
const TAG = "v1.2.3";
const BASE = `https://github.com/${REPO}/releases/download/${TAG}/`;
const d = (c: string) => c.repeat(64);

/** A small release: a Linux AppImage, the two macOS updater archives and the SBOM. */
function release() {
  const recorded = [
    { digest: d("1"), name: "Silo_1.2.3_amd64.AppImage" },
    { digest: d("2"), name: "Silo_1.2.3_amd64.AppImage.sig" },
    { digest: d("3"), name: "Silo.app.tar.gz" },
    { digest: d("4"), name: "Silo.app.tar.gz.sig" },
    { digest: d("5"), name: "Silo.app.tar.gz" },
    { digest: d("6"), name: "Silo.app.tar.gz.sig" },
    { digest: d("7"), name: "silo-sbom.cdx.json" },
  ];
  const assets: Record<string, string> = {
    "Silo_1.2.3_amd64.AppImage": d("1"),
    "Silo_1.2.3_amd64.AppImage.sig": d("2"),
    "Silo_aarch64.app.tar.gz": d("3"),
    "Silo_aarch64.app.tar.gz.sig": d("4"),
    "Silo_x64.app.tar.gz": d("5"),
    "Silo_x64.app.tar.gz.sig": d("6"),
    "silo-sbom.cdx.json": d("7"),
    "latest.json": d("8"),
  };
  const sigText: Record<string, string> = {
    "Silo_1.2.3_amd64.AppImage.sig": "sig-linux",
    "Silo_aarch64.app.tar.gz.sig": "sig-arm",
    "Silo_x64.app.tar.gz.sig": "sig-intel",
  };
  const manifest = {
    version: "1.2.3",
    platforms: {
      "linux-x86_64": { url: `${BASE}Silo_1.2.3_amd64.AppImage`, signature: "sig-linux" },
      "darwin-aarch64": { url: `${BASE}Silo_aarch64.app.tar.gz`, signature: "sig-arm" },
      "darwin-x86_64": { url: `${BASE}Silo_x64.app.tar.gz`, signature: "sig-intel" },
    },
  };
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
    r.assets["Silo_1.2.3_amd64.AppImage"] = d("9");
    const p = check(r);
    expect(p.some((s) => s.startsWith("Silo_1.2.3_amd64.AppImage (sha256"))).toBe(true);
    expect(p.some((s) => s.startsWith("Recorded build output Silo_1.2.3_amd64.AppImage"))).toBe(true);
  });

  it("fails recorded bytes uploaded under another file's name", () => {
    const r = release();
    r.assets["silo-sbom.cdx.json"] = d("1");
    expect(check(r).some((s) => s.startsWith("silo-sbom.cdx.json (sha256"))).toBe(true);
  });

  it("fails an asset nobody recorded and a recorded file missing from the release", () => {
    const r = release();
    r.assets["extra.zip"] = d("a");
    delete r.assets["silo-sbom.cdx.json"];
    const p = check(r);
    expect(p).toContain(`extra.zip (sha256 ${d("a")}) doesn't match any file the build jobs recorded.`);
    expect(p).toContain(`Recorded build output silo-sbom.cdx.json (sha256 ${d("7")}) isn't on the release.`);
  });

  it("fails when no digests were recorded", () => {
    const r = release();
    r.recorded = [];
    expect(check(r)).toContain("No recorded build digests were found.");
  });

  it("checks latest.json's version, urls and signatures", () => {
    const r = release();
    r.manifest.version = "1.2.2";
    r.manifest.platforms["linux-x86_64"].signature = "forged";
    r.manifest.platforms["darwin-aarch64"].url = "https://example.com/Silo_aarch64.app.tar.gz";
    const p = check(r);
    expect(p).toContain("latest.json version '1.2.2' doesn't match tag v1.2.3.");
    expect(p).toContain("latest.json linux-x86_64: signature doesn't match the verified Silo_1.2.3_amd64.AppImage.sig.");
    expect(p.some((s) => s.startsWith("latest.json darwin-aarch64: url"))).toBe(true);
  });

  it("fails latest.json pointing at an unverified asset, and a missing latest.json", () => {
    const r = release();
    r.assets["Silo_x64.app.tar.gz"] = d("9");
    expect(check(r)).toContain("latest.json darwin-x86_64: Silo_x64.app.tar.gz isn't a verified asset.");
    expect(releaseAssetProblems({ ...release(), latestJson: null })).toEqual(["The release has no latest.json."]);
  });

  it("matches upload names to built names", () => {
    expect(uploadNameMatches("Silo_1.2.3_x64-setup.exe", "Silo_1.2.3_x64-setup.exe")).toBe(true);
    expect(uploadNameMatches("Silo.app.tar.gz", "Silo_x64.app.tar.gz")).toBe(true);
    expect(uploadNameMatches("Silo.app.tar.gz.sig", "Silo_aarch64.app.tar.gz.sig")).toBe(true);
    expect(uploadNameMatches("Silo.app.tar.gz", "Silo_x64.app.tar.gz.sig")).toBe(false);
    expect(uploadNameMatches("Silo.app.tar.gz", "Other_x64.app.tar.gz")).toBe(false);
    expect(uploadNameMatches("Silo_1.2.3_amd64.deb", "Silo_1.2.3_amd64.AppImage")).toBe(false);
  });

  it("parses sha256sum lines, CRLF included, and rejects malformed ones", () => {
    expect(parseDigests(`${d("a")}  a.exe\r\n${d("b")} *b.msi\r\n\r\n`)).toEqual([
      { digest: d("a"), name: "a.exe" },
      { digest: d("b"), name: "b.msi" },
    ]);
    expect(() => parseDigests("nothex  a.exe")).toThrow(/Malformed/);
  });

  it("records files, skips directories, and refuses an empty build", () => {
    const hash = (p: string) => (p.endsWith("Silo.dmg") ? d("c") : d("e"));
    const fileCheck = (p: string) => !p.endsWith(".app");
    expect(digestLines(["/b/macos/Silo.app", "/b/dmg/Silo.dmg"], hash, fileCheck)).toEqual([`${d("c")}  Silo.dmg`]);
    expect(() => digestLines(["/b/macos/Silo.app"], hash, fileCheck)).toThrow(/No build output/);
    expect(() => digestLines([], hash, fileCheck)).toThrow(/No build output/);
  });

  it("fails on a build output path that doesn't exist", () => {
    expect(() => digestLines(["no/such/file.exe"])).toThrow();
  });
});
