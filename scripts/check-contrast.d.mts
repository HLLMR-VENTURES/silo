// Types for the exports of check-contrast.mjs (used by src/lib/a11y/contrast.test.ts), so the type
// checker reads these instead of type-checking the Node script itself.
export type Rgb = [number, number, number];
export declare function stripComments(css: string): string;
export declare function blockVars(css: string, selector: string): Record<string, string>;
export declare function parseHex(v: string): Rgb | null;
export declare function resolve(name: string, vars: Record<string, string>): Rgb;
export declare function contrast(a: Rgb, b: Rgb): number;
export declare function pairs(): [string, string, number, string][];
export declare function themes(css: string): Record<string, Record<string, string>>;
export declare function check(css: string): {
  failures: string[];
  rows: { theme: string; fg: string; bg: string; min: number; why: string; ratio: number }[];
};
