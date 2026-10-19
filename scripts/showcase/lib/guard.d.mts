// Types for guard.mjs (used by src/lib/showcase.test.ts), so the type checker reads these
// instead of type-checking the Node script itself.
export declare function realProfileRoots(): { path: string; depth: number }[];
export declare function fingerprintRealProfile(roots?: { path: string; depth: number }[]): Map<string, Map<string, string>>;
export declare function diffFingerprints(before: Map<string, Map<string, string>>, after: Map<string, Map<string, string>>): string[];
export declare function assertUnchanged(before: Map<string, Map<string, string>>): void;
export declare function personalTextPattern(home?: string, user?: string): RegExp;
