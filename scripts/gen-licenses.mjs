#!/usr/bin/env node
// Writes src/lib/help/thirdParty.generated.json: the npm packages bundled into Silo's window
// (package.json `dependencies`, plus the build-time packages whose code ships in the bundle),
// each with its license id and text, for Help → About → Third-party licenses. The fonts' OFL
// texts are read from src/assets/fonts/ by licenses.ts itself (vendor-fonts.mjs copies them).
// Run after adding or bumping one of these packages, then commit the output:
//   node scripts/gen-licenses.mjs

import { existsSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));

/** Other packages whose code ends up in the bundle: devDependencies that aren't only build tools,
 *  and the runtime packages Svelte's client code imports. */
const BUNDLED_DEV = ["svelte", "@lucide/svelte", "clsx", "esm-env"];

/** A package's license text: LICENSE*, else (Tauri plugins ship only an SPDX line) the MIT text
 *  of the project they belong to. */
function licenseText(dir, name) {
  const files = readdirSync(dir).filter((f) => /^licen[cs]e/i.test(f) && !/\.spdx$/i.test(f));
  const pick = files.find((f) => /mit/i.test(f)) ?? files[0];
  if (pick) return readFileSync(join(dir, pick), "utf8").trim();
  if (name.startsWith("@tauri-apps/")) {
    return licenseText(join(root, "node_modules", "@tauri-apps", "api"), "@tauri-apps/api");
  }
  throw new Error(`${name}: no license file`);
}

const names = [...Object.keys(pkg.dependencies ?? {}), ...BUNDLED_DEV].sort();
const out = names.map((name) => {
  const dir = join(root, "node_modules", ...name.split("/"));
  if (!existsSync(dir)) throw new Error(`${name} isn't installed; run npm ci first`);
  const meta = JSON.parse(readFileSync(join(dir, "package.json"), "utf8"));
  const homepage =
    meta.homepage ??
    (typeof meta.repository === "string" ? meta.repository : meta.repository?.url ?? null);
  return {
    name,
    version: meta.version,
    license: meta.license,
    homepage: homepage ? String(homepage).replace(/^git\+/, "").replace(/\.git$/, "") : null,
    text: licenseText(dir, name),
  };
});

const file = join(root, "src", "lib", "help", "thirdParty.generated.json");
writeFileSync(file, JSON.stringify(out, null, 2) + "\n");
console.log(`Wrote ${out.length} packages to ${file}`);
