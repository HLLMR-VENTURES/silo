// Types for the exports of check-release-version.mjs (used by src/lib/releaseScripts.test.ts), so
// the type checker reads these instead of type-checking the Node script itself.
export declare function cargoPackageVersion(toml: string): string | null;
export declare function jsonVersion(text: string): string | null;
export declare function releaseVersionProblems(input: {
  tag: string;
  packageJson: string;
  cargoToml: string;
  tauriConf: string;
}): string[];
export declare function readVersionFiles(root?: string): {
  packageJson: string;
  cargoToml: string;
  tauriConf: string;
};
