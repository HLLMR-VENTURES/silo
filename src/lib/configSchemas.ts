// Curated field/preset schemas for editing specific FS25 config XML files via the
// generic ConfigEditor. Values are written verbatim; the game validates on launch,
// and Silo keeps the untouched original (<file>.silo-original.bak) plus the three versions
// before each edit (<file>.bak, .bak.2, .bak.3), so these are safe to tweak.

export interface CfgField {
  path: string;
  label: string;
  kind: "bool" | "number" | "select" | "text";
  options?: string[];
  optionLabels?: Record<string, string>;
  hint?: string;
}

export interface CfgPreset {
  name: string;
  values: Record<string, string>;
}

// ── game.xml — graphics ──
const G = "game.graphic.scalability.";
const D = "game.graphic.display.";

export const GAME_GRAPHICS_FIELDS: CfgField[] = [
  {
    path: `${G}performanceClass`,
    label: "Performance class",
    kind: "select",
    options: ["minimal", "low", "medium", "high", "veryHigh"],
  },
  {
    path: `${G}textureFiltering`,
    label: "Anisotropic filtering",
    kind: "select",
    options: ["0", "2", "4", "8", "16"],
    optionLabels: { "0": "Off", "2": "2×", "4": "4×", "8": "8×", "16": "16×" },
  },
  {
    path: `${G}shadowMapSize`,
    label: "Shadow map size",
    kind: "select",
    options: ["1024", "2048", "4096"],
  },
  { path: `${G}maxNumShadowLights`, label: "Max shadow lights", kind: "number" },
  { path: `${G}softShadows`, label: "Soft shadows", kind: "bool" },
  {
    path: `${G}fsr@quality`,
    label: "FSR upscaling",
    kind: "select",
    options: ["0", "1", "2", "3", "4"],
    optionLabels: {
      "0": "Off (native)",
      "1": "Quality",
      "2": "Balanced",
      "3": "Performance",
      "4": "Ultra Performance",
    },
    hint: "AMD upscaling: higher = more FPS, softer image",
  },
  {
    path: `${G}volumetricFogQuality`,
    label: "Volumetric fog",
    kind: "select",
    options: ["0", "1", "2", "3"],
    optionLabels: { "0": "Off", "1": "Low", "2": "Medium", "3": "High" },
  },
  {
    path: `${G}cloudShadowQuality`,
    label: "Cloud shadows",
    kind: "select",
    options: ["0", "1", "2", "3"],
    optionLabels: { "0": "Off", "1": "Low", "2": "Medium", "3": "High" },
  },
  { path: `${D}vsync`, label: "V-Sync", kind: "bool" },
];

// ── careerSavegame.xml — gameplay settings (safe subset; NOT the internal
//    revision/collision fields) ──
const S = "careerSavegame.settings.";

export const SAVEGAME_FIELDS: CfgField[] = [
  { path: `${S}savegameName`, label: "Save name", kind: "text" },
  {
    path: `${S}economicDifficulty`,
    label: "Economic difficulty",
    kind: "select",
    options: ["EASY", "NORMAL", "HARD"],
  },
  {
    path: `${S}growthMode`,
    label: "Growth",
    kind: "select",
    options: ["1", "2", "3"],
    optionLabels: { "1": "Seasonal", "2": "Paused", "3": "Off" },
  },
  {
    path: `${S}fuelUsage`,
    label: "Fuel usage",
    kind: "select",
    options: ["1", "2", "3"],
    optionLabels: { "1": "Low", "2": "Normal", "3": "High" },
  },
  { path: `${S}dirtInterval`, label: "Dirt interval", kind: "number" },
  { path: `${S}timeScale`, label: "Time scale", kind: "number", hint: "Day length multiplier" },
  { path: `${S}autoSaveInterval`, label: "Autosave interval (min)", kind: "number" },
  { path: `${S}fruitDestruction`, label: "Crop destruction", kind: "bool" },
  { path: `${S}plowingRequiredEnabled`, label: "Plowing required", kind: "bool" },
  { path: `${S}stonesEnabled`, label: "Stones", kind: "bool" },
  { path: `${S}weedsEnabled`, label: "Weeds", kind: "bool" },
  { path: `${S}limeRequired`, label: "Lime required", kind: "bool" },
  { path: `${S}isSnowEnabled`, label: "Snow", kind: "bool" },
  { path: `${S}trafficEnabled`, label: "Traffic", kind: "bool" },
  { path: `${S}helperBuyFuel`, label: "Helpers buy fuel", kind: "bool" },
  { path: `${S}helperBuySeeds`, label: "Helpers buy seeds", kind: "bool" },
  { path: `${S}helperBuyFertilizer`, label: "Helpers buy fertilizer", kind: "bool" },
];

export const SAVEGAME_PRESETS: CfgPreset[] = [
  {
    name: "Casual",
    values: {
      [`${S}economicDifficulty`]: "EASY",
      [`${S}fruitDestruction`]: "false",
      [`${S}plowingRequiredEnabled`]: "false",
      [`${S}stonesEnabled`]: "false",
      [`${S}weedsEnabled`]: "false",
      [`${S}limeRequired`]: "false",
      [`${S}helperBuyFuel`]: "true",
      [`${S}helperBuySeeds`]: "true",
      [`${S}helperBuyFertilizer`]: "true",
    },
  },
  {
    name: "Realistic",
    values: {
      [`${S}economicDifficulty`]: "HARD",
      [`${S}fruitDestruction`]: "true",
      [`${S}plowingRequiredEnabled`]: "true",
      [`${S}stonesEnabled`]: "true",
      [`${S}weedsEnabled`]: "true",
      [`${S}limeRequired`]: "true",
      [`${S}helperBuyFuel`]: "false",
      [`${S}helperBuySeeds`]: "false",
      [`${S}helperBuyFertilizer`]: "false",
    },
  },
];

export const GAME_GRAPHICS_PRESETS: CfgPreset[] = [
  {
    name: "Performance",
    values: {
      [`${G}performanceClass`]: "low",
      [`${G}textureFiltering`]: "4",
      [`${G}shadowMapSize`]: "1024",
      [`${G}maxNumShadowLights`]: "4",
      [`${G}softShadows`]: "false",
      [`${G}fsr@quality`]: "2",
      [`${G}volumetricFogQuality`]: "0",
      [`${G}cloudShadowQuality`]: "0",
    },
  },
  {
    name: "Balanced",
    values: {
      [`${G}performanceClass`]: "medium",
      [`${G}textureFiltering`]: "8",
      [`${G}shadowMapSize`]: "2048",
      [`${G}maxNumShadowLights`]: "8",
      [`${G}softShadows`]: "true",
      [`${G}fsr@quality`]: "1",
      [`${G}volumetricFogQuality`]: "1",
      [`${G}cloudShadowQuality`]: "1",
    },
  },
  {
    name: "Quality",
    values: {
      [`${G}performanceClass`]: "high",
      [`${G}textureFiltering`]: "16",
      [`${G}shadowMapSize`]: "4096",
      [`${G}maxNumShadowLights`]: "12",
      [`${G}softShadows`]: "true",
      [`${G}fsr@quality`]: "0",
      [`${G}volumetricFogQuality`]: "2",
      [`${G}cloudShadowQuality`]: "2",
    },
  },
];
