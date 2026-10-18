#!/usr/bin/env node
// Contrast guard (#258): reads src/lib/styles/tokens.css and checks the WCAG 2.2 AA pairs from the
// P2 design (§4.1 / §5.3) in both themes. Text on the app's surfaces needs 4.5:1; control edges,
// the focus ring and fills that carry meaning need 3:1. Run by `npm run check`, so a token edit
// that drops a pair below AA fails CI. Exits 1 with one line per failing pair.
//
//   node scripts/check-contrast.mjs            # check
//   node scripts/check-contrast.mjs --report   # also print every pair and its ratio
import { readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const TOKENS = join(here, "..", "src", "lib", "styles", "tokens.css");

/** tokens.css as text. */
export function readTokens() {
  return readFileSync(TOKENS, "utf8");
}

// ── Parsing ─────────────────────────────────────────────────────────────────────────────────

/** Strips comments so a commented-out token never counts. */
export function stripComments(css) {
  return css.replace(/\/\*[\s\S]*?\*\//g, "");
}

/** The custom properties declared directly inside the first `{…}` after `selector`. */
export function blockVars(css, selector) {
  const at = css.indexOf(selector);
  if (at === -1) throw new Error(`tokens.css: no block for ${selector}`);
  const open = css.indexOf("{", at);
  let depth = 0;
  let end = open;
  for (let i = open; i < css.length; i++) {
    if (css[i] === "{") depth++;
    else if (css[i] === "}" && --depth === 0) {
      end = i;
      break;
    }
  }
  const body = css.slice(open + 1, end);
  const vars = {};
  for (const m of body.matchAll(/(--[\w-]+)\s*:\s*([^;]+);/g)) vars[m[1]] = m[2].trim();
  return vars;
}

/** `#rgb` / `#rrggbb` → [r, g, b] (0–255), or null for anything else. */
export function parseHex(v) {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(v.trim());
  if (!m) return null;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join("") : m[1];
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16));
}

/**
 * Resolves a token to an sRGB colour: hex, `var(--x)`, or
 * `color-mix(in srgb, <a> N%, <b>)` (the soft tints).
 * @param name token name, e.g. "--info-soft"
 * @param vars the theme's tokens merged over the shared :root ones
 */
export function resolve(name, vars, seen = new Set()) {
  if (seen.has(name)) throw new Error(`tokens.css: ${name} refers to itself`);
  seen.add(name);
  const raw = vars[name];
  if (raw === undefined) throw new Error(`tokens.css: ${name} is not defined`);
  return resolveValue(raw, vars, seen);
}

function resolveValue(raw, vars, seen) {
  const v = raw.trim();
  const hex = parseHex(v);
  if (hex) return hex;
  const ref = /^var\((--[\w-]+)\)$/.exec(v);
  if (ref) return resolve(ref[1], vars, new Set(seen));
  const mix = /^color-mix\(in srgb,\s*(.+?)\s+(\d+(?:\.\d+)?)%\s*,\s*(.+)\)$/.exec(v);
  if (mix) {
    const a = resolveValue(mix[1], vars, seen);
    const b = resolveValue(mix[3], vars, seen);
    const p = Number(mix[2]) / 100;
    return a.map((c, i) => Math.round(c * p + b[i] * (1 - p)));
  }
  throw new Error(`tokens.css: can't resolve "${v}"`);
}

// ── WCAG ────────────────────────────────────────────────────────────────────────────────────

function luminance([r, g, b]) {
  const lin = (c) => {
    const s = c / 255;
    return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

/** WCAG contrast ratio between two sRGB colours (1–21). */
export function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

// ── The pairs ───────────────────────────────────────────────────────────────────────────────

const SURFACES = ["--bg", "--surface", "--raised"];
const TEXT_ROLES = [
  "--text",
  "--muted",
  "--primary-text",
  "--accent-text",
  "--info",
  "--success",
  "--warn-text",
  "--danger",
  "--earth-text",
];

/**
 * Every pair to check: [foreground, background, minimum, why].
 * Text 4.5 (1.4.3); control edges, focus ring and meaningful fills 3 (1.4.11).
 */
export function pairs() {
  const out = [];
  for (const fg of TEXT_ROLES) for (const bg of SURFACES) out.push([fg, bg, 4.5, "text"]);
  // Inputs, code and the status strip sit on --sunken.
  for (const fg of ["--text", "--muted"]) out.push([fg, "--sunken", 4.5, "text on sunken"]);
  // Text on a filled control.
  out.push(["--on-primary", "--primary", 4.5, "text on primary fill"]);
  out.push(["--on-primary", "--primary-hover", 4.5, "text on primary hover"]);
  out.push(["--on-danger", "--danger", 4.5, "text on danger fill"]);
  out.push(["--on-accent", "--accent", 4.5, "text on gold fill"]);
  // Notices and badges: the role's text on its soft tint.
  for (const [fg, bg] of [
    ["--info", "--info-soft"],
    ["--success", "--success-soft"],
    ["--warn-text", "--warn-soft"],
    ["--danger", "--danger-soft"],
    ["--accent-text", "--accent-soft"],
    ["--text", "--info-soft"],
    ["--text", "--success-soft"],
    ["--text", "--warn-soft"],
    ["--text", "--danger-soft"],
    ["--text", "--accent-soft"],
  ])
    out.push([fg, bg, 4.5, "text on soft tint"]);
  // Control edges and the focus ring.
  for (const bg of [...SURFACES, "--sunken"]) {
    out.push(["--border-strong", bg, 3, "control edge"]);
    out.push(["--focus", bg, 3, "focus ring"]);
  }
  // Filled controls (switch on, primary button) against what they sit on.
  for (const bg of SURFACES) out.push(["--primary", bg, 3, "primary fill"]);
  return out;
}

/** Light, dark (media query) and dark (data-theme) token sets, each over the shared :root. */
export function themes(css) {
  const clean = stripComments(css);
  // The shared block is the first `:root {` (ramps, soft tints, spacing).
  const base = blockVars(clean, ":root {");
  const light = blockVars(clean, ':root[data-theme="light"]');
  const darkMedia = blockVars(clean, ':root:not([data-theme="light"])');
  const darkAttr = blockVars(clean, ':root[data-theme="dark"]');
  return {
    light: { ...base, ...light },
    "dark (system)": { ...base, ...darkMedia },
    "dark (chosen)": { ...base, ...darkAttr },
  };
}

/**
 * Checks every pair in every theme, plus that the two dark blocks agree.
 * @returns failures (strings) and the full table
 */
export function check(css) {
  const failures = [];
  const rows = [];
  const t = themes(css);
  for (const [theme, vars] of Object.entries(t)) {
    for (const [fg, bg, min, why] of pairs()) {
      const ratio = contrast(resolve(fg, vars), resolve(bg, vars));
      rows.push({ theme, fg, bg, min, why, ratio });
      if (ratio + 1e-9 < min)
        failures.push(`${theme}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1, needs ${min}:1 (${why})`);
    }
  }
  // prefers-contrast: more turns the decorative hairlines into control-strength edges.
  const clean = stripComments(css);
  const moreAt = clean.indexOf("@media (prefers-contrast: more)");
  const moreVars = moreAt === -1 ? {} : blockVars(clean.slice(moreAt), ":root");
  if (moreVars["--border"] !== "var(--border-strong)")
    failures.push("prefers-contrast: more must set --border to var(--border-strong)");
  // The dark theme is written twice (system preference and the explicit choice); they must match.
  const a = t["dark (system)"];
  const b = t["dark (chosen)"];
  for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
    if (a[k] !== b[k]) failures.push(`dark themes differ: ${k} is ${a[k]} (system) but ${b[k]} (chosen)`);
  }
  return { failures, rows };
}

// ── CLI ─────────────────────────────────────────────────────────────────────────────────────

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const { failures, rows } = check(readTokens());
  if (process.argv.includes("--report")) {
    for (const r of rows)
      console.log(
        `${r.ratio + 1e-9 < r.min ? "FAIL" : "ok  "} ${r.theme.padEnd(14)} ${r.fg.padEnd(16)} on ${r.bg.padEnd(14)} ${r.ratio.toFixed(2).padStart(5)} (≥ ${r.min}, ${r.why})`,
      );
  }
  if (failures.length) {
    console.error(`check-contrast: ${failures.length} pair(s) below WCAG AA in tokens.css:`);
    for (const f of failures) console.error(`  ${f}`);
    process.exit(1);
  }
  console.log(`check-contrast: ${rows.length} pairs pass WCAG AA in both themes.`);
}
