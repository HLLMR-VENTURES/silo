// Types for catalog.mjs (used by src/lib/showcase.test.ts).
export interface DemoMod {
  techName: string;
  title: string;
  author: string;
  version: string;
  descVersion: number;
  glyph: string;
  hue: number;
  store: string | null;
  isMap: boolean;
  mapId: string | null;
  uniqueType: string | null;
  mp: boolean;
  source: "github" | "modhub" | "other";
  gh: { owner: string; repo: string } | null;
  sizeKb: number;
  active: boolean;
  addedDaysAgo: number;
}
export interface DemoLibrary {
  mods: DemoMod[];
  sets: { name: string; mods: string[] }[];
  roles: Record<"map" | "otherMap" | "conflictA" | "conflictB" | "culprit" | "verified" | "modified", string>;
}
export declare function rng(seed: number): () => number;
export declare function techNameFor(title: string): string;
export declare function buildCatalog(seed?: number): DemoLibrary;
