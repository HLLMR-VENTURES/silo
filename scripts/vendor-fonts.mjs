#!/usr/bin/env node
// Copies the UI fonts from their @fontsource-variable packages into src/assets/fonts/, so the
// app ships them itself (no network font loads; CSP allows `font-src 'self'`). Run after
// bumping either font package, then commit the outputs:  node scripts/vendor-fonts.mjs
//
// Picks, per family: the variable woff2 with the most axes the package offers, normal style,
// latin + latin-ext subsets, plus the package's SIL OFL licence text.

import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const outDir = join(root, "src", "assets", "fonts");

/** Each family: package dir name, file prefix, axis variants in preference order, licence name. */
const FAMILIES = [
  {
    pkg: "fraunces",
    prefix: "fraunces",
    // "full" = opsz + wght + SOFT + WONK; fall back to fewer axes if a release drops it.
    variants: ["full", "opsz", "wght"],
    license: "OFL-Fraunces.txt",
  },
  {
    pkg: "hanken-grotesk",
    prefix: "hanken-grotesk",
    variants: ["wght"],
    license: "OFL-HankenGrotesk.txt",
  },
];
const SUBSETS = ["latin", "latin-ext"];

mkdirSync(outDir, { recursive: true });

let total = 0;
function copy(src, name) {
  const dest = join(outDir, name);
  copyFileSync(src, dest);
  const size = statSync(dest).size;
  total += size;
  console.log(`  ${name}  ${size.toLocaleString("en-US")} B`);
}

for (const fam of FAMILIES) {
  const pkgDir = join(root, "node_modules", "@fontsource-variable", fam.pkg);
  const filesDir = join(pkgDir, "files");
  if (!existsSync(filesDir)) {
    console.error(`missing ${filesDir} — run npm ci first`);
    process.exit(1);
  }
  const available = new Set(readdirSync(filesDir));
  const variant = fam.variants.find((v) =>
    SUBSETS.every((s) => available.has(`${fam.prefix}-${s}-${v}-normal.woff2`)),
  );
  if (!variant) {
    console.error(`${fam.pkg}: none of [${fam.variants.join(", ")}] found for ${SUBSETS.join(" + ")}`);
    process.exit(1);
  }
  console.log(`${fam.pkg} (${variant}):`);
  for (const s of SUBSETS) {
    const name = `${fam.prefix}-${s}-${variant}-normal.woff2`;
    copy(join(filesDir, name), name);
  }
  copy(join(pkgDir, "LICENSE"), fam.license);
}

console.log(`total ${total.toLocaleString("en-US")} B -> ${outDir}`);
