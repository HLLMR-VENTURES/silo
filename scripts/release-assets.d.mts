// Types for the exports of release-assets.mjs (used by src/lib/releaseScripts.test.ts), so the
// type checker reads these instead of type-checking the Node script itself.
export type Digest = { digest: string; name: string };
export declare const LATEST_JSON: string;
export declare function sha256File(path: string): string;
export declare function isFile(path: string): boolean;
export declare function digestLines(
  paths: string[],
  hash?: (path: string) => string,
  fileCheck?: (path: string) => boolean,
): string[];
export declare function parseDigests(text: string): Digest[];
export declare function uploadNameMatches(recorded: string, asset: string): boolean;
export declare function latestJsonProblems(
  latestJson: string | null,
  verified: Set<string>,
  sigText: Record<string, string>,
  repo: string,
  tag: string,
): string[];
export declare function releaseAssetProblems(input: {
  recorded: Digest[];
  assets: Record<string, string>;
  latestJson: string | null;
  sigText: Record<string, string>;
  repo: string;
  tag: string;
}): string[];
