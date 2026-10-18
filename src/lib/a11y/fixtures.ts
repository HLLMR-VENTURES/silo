// Test fixtures for the a11y and perf suites (#258): scan entries shaped like a real library.
// `syntheticLibrary(2500)` is the §5.5 stress fixture: deterministic (no Math.random), with the
// spread a real mods folder has (categories, maps, scripts, odd titles, missing versions).
import type { ModEntry } from "../types";

const CATEGORIES = [
  "Tractors",
  "Trailers",
  "Harvesters",
  "Tools",
  "Placeables",
  "Maps",
  "Gameplay",
  "Forestry",
  "Animals",
  "Trucks",
];
const SUBCATEGORIES = [null, "Small", "Medium", "Large", null, "Misc"];
const WORDS = ["Valley", "Harvest", "Fendt", "John", "Lizard", "Krone", "Pack", "Silo", "Courseplay", "Ultra"];

/** One scan entry; `i` makes the tech name, path and title unique. */
export function modEntry(i: number, over: Partial<ModEntry> = {}): ModEntry {
  return {
    techName: `FS25_Mod${i}`,
    path: `C:/mods/FS25_Mod${i}.zip`,
    kind: "zip",
    size: 1024 * 1024 * 3,
    mtimeMs: 1000 + i,
    title: `Mod ${i}`,
    author: "Someone",
    version: "1.0.0.0",
    descVersion: 1,
    iconFilename: null,
    isMap: false,
    mapTitle: null,
    category: "Tools",
    subcategory: null,
    organized: false,
    active: false,
    dependencies: [],
    scriptCount: 0,
    registrationCount: 0,
    uniqueType: null,
    storeItemCount: 0,
    mpSupported: true,
    ignoredDigitPrefix: false,
    error: null,
    hasSettings: false,
    settingsSource: "none",
    ...over,
  };
}

/**
 * A deterministic library of `n` mods with a realistic mix: ten categories, some subcategories,
 * one map in 40, scripts in one of three, a missing version or title now and then.
 * @param n how many mods (2,500 for the §5.5 fixture)
 */
export function syntheticLibrary(n: number): ModEntry[] {
  const out: ModEntry[] = [];
  for (let i = 0; i < n; i++) {
    const a = WORDS[i % WORDS.length];
    const b = WORDS[(i * 7 + 3) % WORDS.length];
    const isMap = i % 40 === 0;
    out.push(
      modEntry(i, {
        title: i % 97 === 0 ? null : `${a} ${b} ${i}`,
        author: i % 13 === 0 ? null : `Author ${i % 50}`,
        version: i % 31 === 0 ? null : `1.${i % 9}.${i % 4}.0`,
        size: ((i * 7919) % 400) * 1024 * 64 + 1024,
        mtimeMs: 1_700_000_000_000 + ((i * 104_729) % 10_000_000),
        category: isMap ? "Maps" : CATEGORIES[i % CATEGORIES.length],
        subcategory: SUBCATEGORIES[i % SUBCATEGORIES.length],
        isMap,
        mapTitle: isMap ? `${a} Valley` : null,
        scriptCount: i % 3 === 0 ? 2 : 0,
        uniqueType: i % 211 === 0 ? `type${i % 5}` : null,
        organized: i % 2 === 0,
        active: i % 4 !== 0,
        dependencies: i % 17 === 0 ? [`FS25_Mod${(i + 1) % n}`, "FS25_missingDep"] : [],
      }),
    );
  }
  return out;
}
