#!/usr/bin/env node
// Stage the showcase demo: an isolated Silo profile holding a fictional ~700-mod FS25 library, a
// clean and a crashed log.txt, savegames and the facts the capture script needs. Nothing here
// reads or writes the real FS25 user folder or Silo's own data. See docs/SHOWCASE.md.
//
//   node scripts/showcase/stage.mjs                 # -> <drive>:\silo-demo (restaged if it exists)
//   node scripts/showcase/stage.mjs --out D:\demo   # somewhere else (never inside your home folder)
//
// Layout of the profile (SILO_PROFILE_DIR for a debug build):
//   silo-profile.marker            the debug build refuses a profile without it
//   FarmingSimulator2025/          the demo FS25 user folder: gameSettings.xml, log.txt, mods/, savegames
//   fixtures/log-crash.txt         the crashed run the capture swaps in for the crash shots
//   showcase.json                  the library and its roles (culprit, conflict pair, …)
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import crypto from "node:crypto";
import { fileURLToPath } from "node:url";
import { buildCatalog, rng } from "./lib/catalog.mjs";
import { iconPng } from "./lib/png.mjs";
import { writeZip } from "./lib/zip.mjs";
import { buildStubs } from "./lib/stubs.mjs";

const MARKER = "silo-profile.marker";
const USER_DIR = "FarmingSimulator2025";

/** Parse `--out <dir>`. */
function parseArgs(argv) {
  const out = { dir: path.join(path.parse(process.cwd()).root, "silo-demo") };
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === "--out") out.dir = path.resolve(argv[++i] ?? "");
    else if (argv[i] === "--help" || argv[i] === "-h") {
      console.log("usage: node scripts/showcase/stage.mjs [--out <dir>]");
      process.exit(0);
    } else throw new Error(`unknown argument: ${argv[i]}`);
  }
  return out;
}

/**
 * Refuse a destination that could hold real data or leak a user name into screenshots: anything
 * inside the home folder (Documents, AppData, the real FS25 folder all live there), and a
 * non-empty folder that isn't a profile this script made.
 */
export function checkDestination(dir, home = os.homedir()) {
  const rel = path.relative(home, dir);
  if (!rel || (!rel.startsWith("..") && !path.isAbsolute(rel))) {
    throw new Error(`refusing ${dir}: it is inside your home folder (${home}). Pick a folder like D:\\silo-demo.`);
  }
  if (fs.existsSync(dir) && fs.readdirSync(dir).length > 0 && !fs.existsSync(path.join(dir, MARKER))) {
    throw new Error(`refusing ${dir}: it isn't empty and isn't a staged profile (no ${MARKER}).`);
  }
}

const xmlEscape = (s) => String(s).replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");

function describe(m) {
  if (m.isMap) return `A ${m.title.includes("4x") ? "4x " : ""}farming map with rolling fields, a river valley and room for large equipment.`;
  if (m.store) return `${m.title}. Fully animated, with configurations for colour, tyres and work width.`;
  return `${m.title}. Adds a small gameplay script; settings can be changed in the in-game menu.`;
}

function modDescXml(m) {
  const base = m.techName.replace(/^FS25_/, "");
  const parts = [
    `<?xml version="1.0" encoding="utf-8" standalone="no"?>`,
    `<modDesc descVersion="${m.descVersion}">`,
    `    <author>${xmlEscape(m.author)}</author>`,
    `    <version>${m.version}</version>`,
    `    <title><en>${xmlEscape(m.title)}</en></title>`,
    `    <description><en><![CDATA[${describe(m)}${m.gh ? `\n\nSource and issues: https://github.com/${m.gh.owner}/${m.gh.repo}` : ""}]]></en></description>`,
    `    <iconFilename>icon_${base}.png</iconFilename>`,
    `    <multiplayer supported="${m.mp}"/>`,
  ];
  if (m.uniqueType) parts.push(`    <uniqueType>${m.uniqueType}</uniqueType>`);
  if (m.isMap) {
    parts.push(`    <maps>`, `        <map id="${m.mapId}" className="Mission00" filename="$dataS/scripts/missions/mission00.lua" defaultVehiclesXMLFilename="maps/vehicles.xml">`, `            <title><en>${xmlEscape(m.title)}</en></title>`, `        </map>`, `    </maps>`);
  } else if (m.store) {
    parts.push(`    <storeItems>`, `        <storeItem xmlFilename="xml/${base}.xml"/>`, `    </storeItems>`);
  } else {
    parts.push(`    <extraSourceFiles>`, `        <sourceFile filename="scripts/${base}.lua"/>`, `    </extraSourceFiles>`);
  }
  parts.push(`</modDesc>`, ``);
  return parts.join("\n");
}

function storeItemXml(m) {
  return `<?xml version="1.0" encoding="utf-8" standalone="no"?>
<vehicle type="${m.glyph === "building" || m.glyph === "storage" ? "placeable" : "vehicle"}">
    <storeData>
        <name>${xmlEscape(m.title)}</name>
        <image>icon_${m.techName.replace(/^FS25_/, "")}.png</image>
        <price>${10000 + (m.sizeKb % 97) * 2500}</price>
        <category>${m.store}</category>
    </storeData>
</vehicle>
`;
}

/** Write one mod archive. The filler stands in for textures and models, so file sizes look real. */
function writeMod(dir, m) {
  const base = m.techName.replace(/^FS25_/, "");
  const entries = [
    { name: "modDesc.xml", data: Buffer.from(modDescXml(m)) },
    { name: `icon_${base}.png`, data: iconPng(m.glyph, m.hue), store: true },
  ];
  if (m.store && !m.isMap) entries.push({ name: `xml/${base}.xml`, data: Buffer.from(storeItemXml(m)) });
  if (!m.store && !m.isMap) entries.push({ name: `scripts/${base}.lua`, data: Buffer.from(`-- ${m.title} (Silo showcase demo: a placeholder, not a working mod)\n${base} = {}\n`) });
  entries.push({ name: m.isMap ? "maps/data/terrain.dds" : `textures/${base}_diffuse.dds`, data: Buffer.alloc(Math.max(1, m.sizeKb - 20) * 1024), store: true });
  writeZip(path.join(dir, `${m.techName}.zip`), entries);
}

const md5 = (s) => crypto.createHash("md5").update(s).digest("hex");

/** One `Available mod:` line per active mod, as FS25 writes them. */
function inventory(mods, stamp) {
  return mods.filter((m) => m.active).map((m) => `${stamp()} Available mod: (Hash: ${md5(m.techName + m.version)}) (Version: ${m.version}) ${m.techName}`);
}

/** A clock that ticks forward a few milliseconds per line, in the log's timestamp format. */
function clock(start) {
  let t = start.getTime();
  const pad = (n, w = 2) => String(n).padStart(w, "0");
  return (stepMs = 7) => {
    t += stepMs;
    const d = new Date(t);
    return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}.${pad(d.getMilliseconds(), 3)}`;
  };
}

function header(stamp) {
  return [
    `${stamp()} GIANTS Engine Runtime 10.0.7 (14311) (Nov  4 2025)`,
    `${stamp()} Copyright GIANTS Software GmbH. All rights reserved.`,
    `${stamp()} Farming Simulator 25 1.14.0.0 (14310)`,
    `${stamp()} Main System`,
    `${stamp()}   CPU: AMD Ryzen 7 7800X3D 8-Core Processor`,
    `${stamp()}   Memory: 32 GB`,
    `${stamp()} Loading mods...`,
  ];
}

/** A run that ended normally: a few cosmetic warnings, then the quit markers. */
export function cleanLog(lib, start = new Date()) {
  const stamp = clock(start);
  const active = lib.mods.filter((m) => m.active && !m.isMap);
  return [
    ...header(stamp),
    ...inventory(lib.mods, stamp),
    `${stamp(40)} Warning (l10n): Missing text 'input_HOLLOW_TOGGLE' in mod '${active[3].techName}'`,
    `${stamp()} Warning (l10n): Missing text 'shop_configuration_rims' in mod '${active[9].techName}'`,
    `${stamp(900)} Started savegame 'savegame1'`,
    `${stamp(3_600_000)} Saving game 'savegame1'`,
    `${stamp(4000)} Application quit`,
    `${stamp()} #End.`,
    ``,
  ].join("\n");
}

/** A run that died: the culprit throws Lua errors while the save loads, and the log just stops. */
export function crashLog(lib, start = new Date()) {
  const stamp = clock(start);
  const c = lib.roles.culprit;
  const active = lib.mods.filter((m) => m.active && !m.isMap && m.techName !== c);
  const warnA = active[5].techName;
  const warnB = active[17].techName;
  const cosmetic = active[3].techName;
  const dryer = (n) => [
    `${stamp()} Error: Running LUA method 'update'.`,
    `${stamp()}   mods/${c}/scripts/GrainDryer.lua:${212 + n}: attempt to index nil with 'fillLevel'`,
    `${stamp()}   mods/${c}/scripts/GrainDryer.lua:98: in function 'updateDrying'`,
    `${stamp()}   dataS/scripts/placeables/Placeable.lua:1450: in function 'update'`,
  ];
  return [
    ...header(stamp),
    ...inventory(lib.mods, stamp),
    `${stamp(40)} Warning (l10n): Missing text 'input_HOLLOW_TOGGLE' in mod '${cosmetic}'`,
    `${stamp()} Warning (l10n): Missing text 'shop_configuration_rims' in mod '${cosmetic}'`,
    `${stamp()} Warning: Texture 'textures/decal_normal.dds' not found in mod '${warnA}'`,
    `${stamp()} Warning: Unknown fill type 'SUNFLOWER_OIL' in mod '${warnB}'`,
    `${stamp(900)} Loading savegame 'savegame1'`,
    `${stamp()} Error: Could not load placeable xml 'mods/${c}/xml/HollowCreekGrainDryer.xml', missing fill unit 'dryerInput'`,
    ...dryer(0),
    ...dryer(6),
    ...dryer(14),
    `${stamp()} Error: Running LUA method 'draw'.`,
    `${stamp()}   mods/${c}/scripts/GrainDryerHud.lua:41: attempt to perform arithmetic on a nil value`,
    `${stamp()} Warning: Unknown fill type 'SUNFLOWER_OIL' in mod '${warnB}'`,
    ``,
  ].join("\n");
}

/** A control map like FS25 writes: keyboard, mouse and gamepad, with a couple of keys shared. */
export function inputBindingXml() {
  const kb = [
    ["AXIS_ACCELERATE_VEHICLE", "KEY_w"], ["AXIS_BRAKE_VEHICLE", "KEY_s"], ["AXIS_MOVE_SIDE_VEHICLE", "KEY_a"], ["AXIS_MOVE_SIDE_VEHICLE", "KEY_d"],
    ["ENTER", "KEY_e"], ["SWITCH_VEHICLE", "KEY_tab"], ["SWITCH_VEHICLE_BACK", "KEY_lshift KEY_tab"], ["ATTACH", "KEY_q"],
    ["LOWER_IMPLEMENT", "KEY_v"], ["IMPLEMENT_EXTRA", "KEY_b"], ["IMPLEMENT_EXTRA2", "KEY_x"], ["TOGGLE_LIGHTS", "KEY_f"],
    ["TOGGLE_BEACON_LIGHTS", "KEY_h"], ["TOGGLE_CRUISE_CONTROL", "KEY_3"], ["TOGGLE_HELPER", "KEY_h"], ["RADIO_TOGGLE", "KEY_r"],
    ["CAMERA_SWITCH", "KEY_c"], ["CAMERA_ZOOM_IN", "MOUSE_BUTTON_WHEEL_UP"], ["CAMERA_ZOOM_OUT", "MOUSE_BUTTON_WHEEL_DOWN"],
    ["MENU", "KEY_esc"], ["TOGGLE_MAP_SIZE", "KEY_m"], ["CHAT", "KEY_t"], ["JUMP", "KEY_space"], ["CROUCH", "KEY_lctrl"],
    ["ACTIVATE_HANDTOOL", "MOUSE_BUTTON_LEFT"], ["INTERACT", "KEY_r"], ["TOGGLE_PIPE", "KEY_o"], ["TOGGLE_COVER", "KEY_c"],
    ["AXIS_FRONTLOADER_ARM", "KEY_KP_8"], ["AXIS_FRONTLOADER_TOOL", "KEY_KP_4"], ["TOGGLE_MOTOR_STATE", "KEY_z"], ["SHOW_HELP", "KEY_F1"],
  ];
  const pad = [
    ["AXIS_ACCELERATE_VEHICLE", "BUTTON_RT"], ["AXIS_BRAKE_VEHICLE", "BUTTON_LT"], ["AXIS_MOVE_SIDE_VEHICLE", "AXIS_LX"],
    ["ENTER", "BUTTON_Y"], ["ATTACH", "BUTTON_X"], ["LOWER_IMPLEMENT", "BUTTON_A"], ["TOGGLE_LIGHTS", "BUTTON_DPAD_UP"],
    ["TOGGLE_CRUISE_CONTROL", "BUTTON_DPAD_RIGHT"], ["CAMERA_SWITCH", "BUTTON_RS"], ["MENU", "BUTTON_START"], ["JUMP", "BUTTON_A"],
  ];
  const byAction = new Map();
  for (const [device, list] of [["KB_MOUSE_DEFAULT", kb], ["GAMEPAD", pad]]) {
    for (const [action, input] of list) {
      if (!byAction.has(action)) byAction.set(action, []);
      byAction.get(action).push(`    <binding device="${device}" input="${input}" index="1"/>`);
    }
  }
  const body = [...byAction].map(([a, rows]) => `  <actionBinding action="${a}">\n${rows.join("\n")}\n  </actionBinding>`);
  return `<?xml version="1.0" encoding="utf-8" standalone="no"?>\n<inputBinding version="24">\n${body.join("\n")}\n</inputBinding>\n`;
}

function careerSavegame(name, mapTitle, date, mods) {
  const rows = mods.map((m) => `    <mod modName="${m.techName}" title="${xmlEscape(m.title)}" version="${m.version}" required="${m.isMap || m.store ? "true" : "false"}" fileHash="${md5(m.techName)}"/>`);
  return `<?xml version="1.0" encoding="utf-8" standalone="no"?>
<careerSavegame revision="2" valid="true">
    <settings>
        <savegameName>${xmlEscape(name)}</savegameName>
        <mapTitle>${xmlEscape(mapTitle)}</mapTitle>
        <saveDate>${date}</saveDate>
    </settings>
${rows.join("\n")}
</careerSavegame>
`;
}

/**
 * Stage (or restage) the demo profile at `dir`.
 * @param {string} dir the profile directory (outside your home folder)
 */
export function stage(dir) {
  checkDestination(dir);
  if (fs.existsSync(dir)) fs.rmSync(dir, { recursive: true, force: true });
  const user = path.join(dir, USER_DIR);
  const mods = path.join(user, "mods");
  fs.mkdirSync(mods, { recursive: true });
  fs.mkdirSync(path.join(dir, "fixtures"), { recursive: true });
  fs.writeFileSync(path.join(dir, MARKER), "Silo showcase demo profile. Safe to delete.\n");

  const lib = buildCatalog();
  const t0 = Date.now();
  // File dates drive Recently added (see addedDaysAgo in the catalog).
  for (const m of lib.mods) {
    writeMod(mods, m);
    const t = new Date(t0 - m.addedDaysAgo * 86400_000);
    fs.utimesSync(path.join(mods, `${m.techName}.zip`), t, t);
  }
  const bytes = lib.mods.reduce((n, m) => n + m.sizeKb * 1024, 0);
  console.log(`staged ${lib.mods.length} mods (${(bytes / 2 ** 30).toFixed(2)} GiB) in ${((Date.now() - t0) / 1000).toFixed(1)} s`);

  fs.writeFileSync(path.join(user, "gameSettings.xml"), `<?xml version="1.0" encoding="utf-8" standalone="no"?>
<gameSettings revision="94">
    <modsDirectoryOverride active="false" directory=""/>
</gameSettings>
`);
  // The clean run happened two hours ago, so Silo opens on a library that played fine.
  const cleanAt = new Date(Date.now() - 2 * 3600_000);
  fs.writeFileSync(path.join(user, "log.txt"), cleanLog(lib, new Date(cleanAt.getTime() - 3700_000)));
  fs.utimesSync(path.join(user, "log.txt"), cleanAt, cleanAt);
  fs.writeFileSync(path.join(dir, "fixtures", "log-crash.txt"), crashLog(lib));
  fs.writeFileSync(path.join(user, "inputBinding.xml"), inputBindingXml());
  // Three fresh downloads, kept aside: the Organize preview shot drops them into the mods folder.
  const downloads = path.join(dir, "fixtures", "new-downloads");
  fs.mkdirSync(downloads, { recursive: true });
  const fresh = [
    { title: "Aldervik 7T 190", glyph: "tractor", hue: 75, store: "tractorsM", sizeKb: 3900 },
    { title: "Riverside Grain Elevator", glyph: "building", hue: 30, store: "sellingStations", sizeKb: 2600 },
    { title: "Fuel Station Pack", glyph: "storage", hue: 200, store: "silos", sizeKb: 1200 },
  ].map((m) => ({ ...m, techName: `FS25_${m.title.replace(/[^A-Za-z0-9]/g, "")}`, author: "Copperleaf", version: "1.0.0.0", descVersion: 105, mp: true, gh: null, isMap: false }));
  for (const m of fresh) writeMod(downloads, m);

  const r = rng(7);
  const activeMods = lib.mods.filter((m) => m.active);
  const saves = [
    ["Hollow Creek Dairy", "Hollow Creek Valley", "2026-10-17", activeMods],
    ["Marlow Flats Grain", "Marlow Flats", "2026-10-12", lib.mods.filter((m) => !m.isMap && r() < 0.2).concat(lib.mods.filter((m) => m.techName === lib.roles.otherMap))],
    ["Riverbend Starter", "Riverbend Springs", "2026-09-28", lib.mods.filter((m) => !m.isMap && r() < 0.08)],
  ];
  saves.forEach(([name, map, date, list], i) => {
    const sd = path.join(user, `savegame${i + 1}`);
    fs.mkdirSync(sd, { recursive: true });
    fs.writeFileSync(path.join(sd, "careerSavegame.xml"), careerSavegame(name, map, date, list));
  });

  fs.writeFileSync(path.join(dir, "stubs.json"), JSON.stringify(buildStubs(lib)));
  fs.writeFileSync(path.join(dir, "showcase.json"), JSON.stringify({ stagedAt: new Date().toISOString(), userDir: USER_DIR, roles: lib.roles, sets: lib.sets, mods: lib.mods }, null, 1));
  console.log(`profile ready: ${dir}`);
  console.log(`  active: ${activeMods.length} of ${lib.mods.length}; culprit ${lib.roles.culprit}; conflict ${lib.roles.conflictA} + ${lib.roles.conflictB}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    stage(parseArgs(process.argv.slice(2)).dir);
  } catch (e) {
    console.error(e.message ?? e);
    process.exit(1);
  }
}
