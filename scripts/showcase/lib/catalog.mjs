// The demo library: ~700 fictional mods with believable titles, authors, versions and FS store
// categories. Every brand, author, map and GitHub owner here is invented, so no real mod or
// modder is ever shown (least of all as a crash culprit). Deterministic: the same seed gives
// the same library.
//   const lib = buildCatalog();  // { mods: [...], roles: { culprit, conflict, ... } }

/** Mulberry32: small, fast, seedable. */
export function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const BRANDS = [
  ["Torvik", 205], ["Halvard", 12], ["Brannock", 140], ["Kestrel", 28], ["Valmora", 350],
  ["Ostreg", 95], ["Lindqvist", 220], ["Corvane", 45], ["Maritz", 160], ["Selden", 265],
  ["Pellham", 0], ["Rudmann", 120], ["Holmgard", 190], ["Tamsin", 300], ["Aldervik", 75],
  ["Greyhurst", 235],
];

// No "&" in names or titles: Silo currently cuts modDesc text at an escaped ampersand.
const AUTHORS = [
  "FieldworkFox", "Ostmark Modding", "PrairieWorks", "TJ Agri", "Nordlicht Mods", "Clayfield Studio",
  "HedgerowMods", "Bauer und Sohn Modding", "GreenAcre Lab", "Rivermouth Farms", "Kaskade Modding",
  "Low Valley Mods", "Sixtyfive Acres", "Ironfurrow", "Brookside Modteam", "Marten Modding",
  "Westfold Agri", "Copperleaf", "Stubblefield Mods", "OakRidge Modding", "Haymaker Collective",
  "Northbank Mods", "Elm and Ash", "Grainline", "Fenwick Modding", "Redgate Agri", "Moorland Mods",
  "Sundale Works", "Plough and Pixel", "Thistle Modding",
];

// GitHub owners for the handful of mods published there (invented handles).
const GH_OWNERS = ["fieldworkfox-mods", "prairieworks-fs", "ironfurrow-dev", "copperleaf-fs", "grainline-mods", "plough-and-pixel"];

const VERSIONS = ["1.0.0.0", "1.0.0.0", "1.0.0.0", "1.0.1.0", "1.0.2.0", "1.1.0.0", "1.1.0.0", "1.2.0.0", "1.2.1.0", "1.3.0.0", "2.0.0.0", "2.1.0.0"];

/**
 * The library's groups: how many of each, its FS store category, glyph and title maker.
 * `t(brand, r)` returns a title; `r` is the group's random source.
 */
const GROUPS = [
  { n: 34, store: "tractorsS", glyph: "tractor", t: (b, r) => `${b} ${pick(r, ["3", "4", "5"])}${pick(r, ["R", "M", "E", "S"])} ${pick(r, [75, 85, 90, 100, 110, 115])}` },
  { n: 36, store: "tractorsM", glyph: "tractor", t: (b, r) => `${b} ${pick(r, ["6", "7"])}${pick(r, ["R", "M", "V", "T"])} ${pick(r, [140, 155, 165, 175, 195, 215])}` },
  { n: 26, store: "tractorsL", glyph: "tractor", t: (b, r) => `${b} ${pick(r, ["8", "9", "X"])}${pick(r, ["R", "T", "RX", "Vario"])} ${pick(r, [310, 370, 410, 470, 540, 620])}` },
  { n: 24, store: "harvesters", glyph: "harvester", t: (b, r) => `${b} ${pick(r, ["CR", "TX", "S", "Lexo"])} ${pick(r, [760, 790, 8.9, 9.8, 10.9, 7300])}` },
  { n: 10, store: "forageHarvesters", glyph: "harvester", t: (b, r) => `${b} Forage ${pick(r, [850, 900, 980, 1100])}` },
  { n: 8, store: "beetVehicles", glyph: "harvester", t: (b, r) => `${b} Beet Harvester ${pick(r, ["T4", "R6", "Rexor"])}` },
  { n: 22, store: "cutters", glyph: "implement", t: (b, r) => `${b} ${pick(r, ["Draper Header", "Corn Header", "Grain Header"])} ${pick(r, [9, 10.5, 12, 13.5])}m` },
  { n: 48, store: "trailers", glyph: "trailer", t: (b, r) => `${b} ${pick(r, ["Tipper", "Grain Trailer", "Dump Trailer", "Silage Trailer"])} ${pick(r, ["TX", "TDK", "ST", "HD"])} ${pick(r, [14, 18, 22, 24, 30, 36])}` },
  { n: 12, store: "augerWagons", glyph: "trailer", t: (b, r) => `${b} Grain Cart ${pick(r, [1100, 1300, 1500, 2000])}` },
  { n: 16, store: "plows", glyph: "implement", t: (b, r) => `${b} ${pick(r, [4, 5, 6, 7, 8])}-Furrow Plough` },
  { n: 30, store: "cultivators", glyph: "implement", t: (b, r) => `${b} ${pick(r, ["Cultivator", "Disc Harrow", "Power Harrow", "Subsoiler"])} ${pick(r, [3, 4, 4.5, 6, 8])}m` },
  { n: 24, store: "sowingMachines", glyph: "implement", t: (b, r) => `${b} ${pick(r, ["Seed Drill", "Direct Drill", "Precision Planter"])} ${pick(r, [3, 4, 6, 9, 12])}m` },
  { n: 16, store: "sprayers", glyph: "trailer", t: (b, r) => `${b} ${pick(r, ["Trailed Sprayer", "Mounted Sprayer", "Self-Propelled Sprayer"])} ${pick(r, [3000, 4200, 5000, 6000])}` },
  { n: 18, store: "slurryTanks", glyph: "trailer", t: (b, r) => `${b} ${pick(r, ["Slurry Tanker", "Manure Spreader", "Fertilizer Spreader"])} ${pick(r, [12, 16, 18, 21, 25])}` },
  { n: 22, store: "mowers", glyph: "implement", t: (b, r) => `${b} ${pick(r, ["Mower Combination", "Rotary Rake", "Tedder", "Front Mower"])} ${pick(r, [3, 6, 8.3, 9, 10])}m` },
  { n: 16, store: "balers", glyph: "trailer", t: (b, r) => `${b} ${pick(r, ["Round Baler", "Square Baler", "Bale Wrapper"])} ${pick(r, [125, 155, 160, 1290, 4300])}` },
  { n: 20, store: "frontLoaderTools", glyph: "implement", t: (b, r) => `${b} ${pick(r, ["Bale Spike", "Pallet Fork", "Grain Bucket", "Silage Fork", "Weight"])} ${pick(r, ["", "XL", "800", "1200", "Pro"])}`.trim() },
  { n: 26, store: "telehandlers", glyph: "loader", t: (b, r) => `${b} ${pick(r, ["Telehandler", "Wheel Loader", "Skid Steer"])} ${pick(r, [430, 542, 650, 742, 950])}` },
  { n: 26, store: "trucks", glyph: "vehicle", t: (b, r) => `${b} ${pick(r, ["Semi", "Tipper Truck", "Hooklift Truck", "Flatbed"])} ${pick(r, [420, 480, 530, 640])}` },
  { n: 18, store: "pickups", glyph: "vehicle", t: (b, r) => `${b} ${pick(r, ["Pickup", "Service Truck", "Farm Van"])} ${pick(r, [2018, 2021, 2024, 1985, 1972])}` },
  { n: 22, store: "productionPoints", glyph: "building", t: (_b, r) => `${pick(r, ["Grain Mill", "Cheese Dairy", "Sawmill", "Bakery", "Oil Mill", "Feed Mixer Station", "Cider Press", "Brewery", "Spinnery", "Sugar Mill", "Pellet Plant"])} ${pick(r, ["", "Small", "Large", "Pack", "Deluxe"])}`.trim() },
  { n: 14, store: "sellingStations", glyph: "building", t: (_b, r) => `${pick(r, ["Farm Shop", "Grain Elevator", "Livestock Market", "Roadside Stand", "Wood Yard", "Biogas Buyer"])} ${pick(r, ["", "North", "Riverside", "Compact"])}`.trim() },
  { n: 22, store: "animalPens", glyph: "building", t: (_b, r) => `${pick(r, ["Cow Barn", "Sheep Pasture", "Chicken Coop", "Pig Barn", "Horse Stable", "Goat Shed", "Calf Hutches"])} ${pick(r, [40, 60, 80, 120, 200, "Large", "Small"])}` },
  { n: 22, store: "silos", glyph: "storage", t: (_b, r) => `${pick(r, ["Grain Silo", "Bunker Silo", "Silo Pack", "Liquid Tank", "Hay Barn", "Fertilizer Store"])} ${pick(r, ["2000", "5000", "12000", "Small", "Large", "Set"])}` },
  { n: 22, store: "sheds", glyph: "building", t: (_b, r) => `${pick(r, ["Machine Shed", "Open Shed", "Workshop", "Pole Barn", "Garage", "Equipment Hall"])} ${pick(r, ["12m", "18m", "24m", "36m", "Old", "Modern"])}` },
  { n: 8, store: "greenhouses", glyph: "building", t: (_b, r) => `${pick(r, ["Greenhouse", "Polytunnel", "Hydroponic Hall"])} ${pick(r, ["Small", "Large", "Row", "Set"])}` },
  { n: 10, store: "generators", glyph: "storage", t: (_b, r) => `${pick(r, ["Solar Field", "Wind Turbine", "Biogas Plant", "Rooftop Solar"])} ${pick(r, ["Small", "Medium", "Large", "Pack"])}` },
  { n: 26, store: "decoration", glyph: "decoration", t: (_b, r) => `${pick(r, ["Farm Fence", "Field Signs", "Flag Pole", "Picnic Bench", "Street Lamp", "Hay Bale Decor", "Gate Pack", "Hedge Pack", "Stone Wall"])} ${pick(r, ["", "Pack", "Set", "Vol. 2", "Old Style"])}`.trim() },
  { n: 24, store: "pallets", glyph: "object", t: (_b, r) => `${pick(r, ["Pallet Pack", "Big Bags", "Seed Pallets", "IBC Tanks", "Diesel Tank", "Water Tank", "Lime Bags", "Barrel Set"])} ${pick(r, ["", "Mini", "XL", "Set", "1000"])}`.trim() },
];

// Script/gameplay mods: no store items, keyword-categorized by Silo.
const SCRIPT_TITLES = [
  "Field Lease Contracts", "Work Hours Tracker", "Fuel Price Board", "Daylight Length Presets", "Realistic Wages",
  "Auto Headlights", "Crop Rotation Planner", "Soil Moisture Readout", "Economy Tuning", "Speed Limit Signs Script",
  "Seasonal Grain Prices", "Contract Payout Boost", "Weather Forecast Extended", "Hired Worker Breaks", "Loan Interest Rates",
  "Field Status HUD", "Parking Brake Assist", "Cruise Control Steps", "Silo Fill Levels Display", "Animal Health Overview",
  "Production Overview", "Vehicle Wear Tuning", "Tire Pressure Control", "Camera Zoom Extended", "Map Hotspots Extra",
  "Shop Filter Tools", "Bale Counter", "Straw Harvest Tweaks", "Realistic Fuel Usage", "Hardcore Breakdowns",
  "Difficulty Presets", "Fertilizer Rate Control", "Wheel Track Toggle", "Implement Lift Speed", "Workshop Anywhere",
  "Field Price Calculator", "Rain Delay Contracts", "Fieldwork Statistics", "Dynamic Fuel Prices", "Mission Cooldowns",
  "Easy Coupling Assist", "Selling Price Alerts", "Driver Seat Camera", "Night Shift Wages", "Snow Plough Contracts",
  "Season Length Options", "Vehicle Inspector", "Farm Finance Report",
];
const TEXTURE_TITLES = ["Torvik Color Pack", "Halvard Classic Liveries", "Field Ground Textures", "Rust and Wear Skins", "Brannock Decal Set", "Vintage Paint Pack", "Weathered Barn Textures", "Kestrel Heritage Skins", "Dirt Texture Overhaul", "Matte Paint Pack"];
const SOUND_TITLES = ["Torvik V8 Engine Sound", "Classic Tractor Sound Pack", "Air Brake Sound Pack", "Horn Sound Pack"];

const MAPS = [
  { title: "Hollow Creek Valley", id: "HollowCreek", author: "Rivermouth Farms" },
  { title: "Marlow Flats", id: "MarlowFlats", author: "Westfold Agri" },
  { title: "Kestrel Ridge 4x", id: "KestrelRidge", author: "Ironfurrow" },
  { title: "Nordheide", id: "Nordheide", author: "Nordlicht Mods" },
];

function pick(r, arr) {
  return arr[Math.floor(r() * arr.length)];
}

/** "Brannock Disc Harrow 4.5m" -> "FS25_BrannockDiscHarrow45m". */
export function techNameFor(title) {
  const words = title.replace(/&/g, "And").replace(/[^A-Za-z0-9 ]+/g, "").split(/\s+/).filter(Boolean);
  return "FS25_" + words.map((w) => w[0].toUpperCase() + w.slice(1)).join("");
}

/** Group store category -> a Silo sources mix: most ModHub-style, some GitHub, a few elsewhere. */
function sourceFor(r) {
  const x = r();
  return x < 0.06 ? "github" : x < 0.86 ? "modhub" : "other";
}

/**
 * Build the demo library.
 * @param {number} [seed]
 * @returns {{ mods: object[], roles: Record<string, string> }}
 */
export function buildCatalog(seed = 25) {
  const r = rng(seed);
  const mods = [];
  const used = new Set();
  const add = (m) => {
    let title = m.title;
    for (let i = 2; used.has(techNameFor(title)); i++) title = `${m.title} ${["", "", "II", "III", "IV", "V", "VI"][i] ?? i}`.trim();
    const techName = techNameFor(title);
    used.add(techName);
    const author = m.author ?? pick(r, AUTHORS);
    const source = m.source ?? sourceFor(r);
    const gh = source === "github" ? { owner: pick(r, GH_OWNERS), repo: techName } : null;
    mods.push({
      techName,
      title,
      author,
      version: m.version ?? pick(r, VERSIONS),
      descVersion: pick(r, [104, 104, 105, 105, 105, 106]),
      glyph: m.glyph,
      hue: m.hue ?? Math.floor(r() * 360),
      store: m.store ?? null,
      isMap: !!m.isMap,
      mapId: m.mapId ?? null,
      uniqueType: m.uniqueType ?? null,
      mp: r() > 0.08,
      source,
      gh,
      sizeKb: m.sizeKb ?? 200,
      active: false,
    });
    return techName;
  };

  // The specials' titles are taken first, so no generated mod can collide with them.
  for (const t of ["Field Helper AI", "Smart Worker Routes", "Hollow Creek Grain Dryer", "Torvik 7R 215", "Kestrel Round Baler 155"]) used.add(techNameFor(t));
  for (const g of GROUPS) {
    for (let i = 0; i < g.n; i++) {
      const [brand, hue] = pick(r, BRANDS);
      const big = g.glyph === "tractor" || g.glyph === "harvester";
      // Re-roll a taken title a few times before falling back to a numbered one.
      let title = g.t(brand, r);
      for (let k = 0; k < 12 && used.has(techNameFor(title)); k++) title = g.t(brand, r);
      add({ title, glyph: g.glyph, hue, store: g.store, sizeKb: Math.round((big ? 2400 : 700) * (0.4 + r() * 1.6)) });
    }
  }
  for (const t of ["Field Helper AI", "Smart Worker Routes", "Hollow Creek Grain Dryer", "Torvik 7R 215", "Kestrel Round Baler 155"]) used.delete(techNameFor(t));
  for (const title of SCRIPT_TITLES) add({ title, glyph: "script", sizeKb: Math.round(40 + r() * 260) });
  for (const title of TEXTURE_TITLES) add({ title, glyph: "texture", sizeKb: Math.round(900 + r() * 3000) });
  for (const title of SOUND_TITLES) add({ title, glyph: "script", sizeKb: Math.round(1500 + r() * 3000) });
  const mapNames = MAPS.map((m) => add({ title: m.title, glyph: "map", hue: 110 + Math.floor(r() * 40), isMap: true, mapId: m.id, author: m.author, source: "modhub", sizeKb: Math.round(14000 + r() * 9000) }));

  // The one real conflict: two AI-helper mods by different authors claiming the same exclusive
  // slot (<uniqueType>). The first is active; the capture switches the second on for its shot.
  const conflictA = add({ title: "Field Helper AI", glyph: "script", author: "Kaskade Modding", uniqueType: "FIELD_HELPER_AI", version: "2.1.0.0", source: "modhub", sizeKb: 420 });
  const conflictB = add({ title: "Smart Worker Routes", glyph: "script", author: "Sundale Works", uniqueType: "FIELD_HELPER_AI", version: "1.4.0.0", source: "github", sizeKb: 380 });
  // The crash culprit named by the synthetic log (fictional, active).
  const culprit = add({ title: "Hollow Creek Grain Dryer", glyph: "storage", store: "productionPoints", author: "Moorland Mods", version: "1.0.1.0", source: "other", sizeKb: 2100 });
  // Provenance: one build matching the catalog's, one a user re-packed.
  const verified = add({ title: "Torvik 7R 215", glyph: "tractor", hue: 205, store: "tractorsM", author: "PrairieWorks", version: "1.3.0.0", source: "github", sizeKb: 5200 });
  const modified = add({ title: "Kestrel Round Baler 155", glyph: "trailer", hue: 28, store: "balers", author: "Haymaker Collective", version: "1.2.0.0", source: "modhub", sizeKb: 1900 });

  // The active set: the map, the specials (not the second conflict mod) and ~170 others.
  const always = new Set([mapNames[0], conflictA, culprit, verified, modified]);
  for (const m of mods) {
    if (always.has(m.techName)) m.active = true;
    else if (m.isMap || m.techName === conflictB) m.active = false;
    else m.active = r() < 0.25;
  }
  // When each mod was added: recent downloads are mostly in play, older ones mostly parked, and
  // the showcase's special mods sit a few months back (never at the top of Recently added).
  const specials = new Set([mapNames[0], mapNames[1], conflictA, conflictB, culprit, verified, modified]);
  for (const m of mods) {
    m.addedDaysAgo = specials.has(m.techName) ? 40 + r() * 80 : m.active ? 1 + r() ** 1.5 * 300 : 3 + r() * 330;
  }

  // Mod sets: the active set is "Harvest 2026"; two more differ from it.
  const sr = rng(seed + 1);
  const activeNames = mods.filter((m) => m.active).map((m) => m.techName);
  const sets = [
    { name: "Harvest 2026", mods: activeNames },
    { name: "Marlow Flats Grain", mods: [mapNames[1], ...mods.filter((m) => !m.isMap && m.techName !== culprit && sr() < 0.11).map((m) => m.techName)] },
    { name: "Multiplayer night", mods: [mapNames[0], ...mods.filter((m) => !m.isMap && m.techName !== culprit && (m.active ? sr() < 0.6 : sr() < 0.04)).map((m) => m.techName)] },
  ];
  return {
    mods,
    sets,
    roles: { map: mapNames[0], otherMap: mapNames[1], conflictA, conflictB, culprit, verified, modified },
  };
}
