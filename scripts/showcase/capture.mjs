#!/usr/bin/env node
// Capture the showcase screenshots from the real app, running against the staged demo profile.
// Dark theme, a 1600×1000 page at 2× (3200×2000 PNGs), no window frame. See docs/SHOWCASE.md.
//
//   node scripts/showcase/capture.mjs                  # restage, then showcase-out/<id>@2x.png + alt.json
//   node scripts/showcase/capture.mjs --only library,updates --keep-open
//   node scripts/showcase/capture.mjs --no-stage       # reuse the profile as it is (iterating)
//
// Safety: the debug build runs with SILO_PROFILE_DIR, so every folder it touches is inside the
// profile and every online command gets a canned answer. This script also fingerprints the real
// FS25 folder and Silo's real data before and after (any change fails the run), fails on any
// request leaving the app, and refuses to save a shot whose text shows your home folder or user
// name.
import fs from "node:fs";
import path from "node:path";
import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { connectToApp, sleep } from "./lib/cdp.mjs";
import { fingerprintRealProfile, assertUnchanged, personalTextPattern } from "./lib/guard.mjs";
import { SHOTS, ORDER } from "./lib/shots.mjs";
import { stage } from "./stage.mjs";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "..", "..");
const exe = path.join(repo, "src-tauri", "target", "debug", process.platform === "win32" ? "silo.exe" : "silo");

function parseArgs(argv) {
  const o = { profile: path.join(path.parse(process.cwd()).root, "silo-demo"), out: path.join(repo, "showcase-out"), only: null, keepOpen: false, build: false, restage: true, port: 9333 };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--profile") o.profile = path.resolve(argv[++i]);
    else if (a === "--out") o.out = path.resolve(argv[++i]);
    else if (a === "--only") o.only = argv[++i].split(",").map((s) => s.trim()).filter(Boolean);
    else if (a === "--keep-open") o.keepOpen = true;
    else if (a === "--build") o.build = true;
    else if (a === "--no-stage") o.restage = false;
    else if (a === "--port") o.port = Number(argv[++i]);
    else throw new Error(`unknown argument: ${a}`);
  }
  return o;
}

/** Helpers injected into the page: find by visible text or label, click, type, wait. */
const PAGE_HELPERS = `(() => {
  if (window.__sc) return;
  const vis = (e) => !!(e && (e.offsetWidth || e.offsetHeight || e.getClientRects().length));
  const label = (e) => (e.innerText || e.value || e.getAttribute("aria-label") || "").trim();
  const CLICKABLE = "button, a, [role=button], [role=tab], [role=menuitem], [role=option], [role=radio], label";
  const find = (t, sel = CLICKABLE, root = document) => {
    const els = [...root.querySelectorAll(sel)].filter(vis);
    return els.find((e) => label(e) === t || e.getAttribute("aria-label") === t) ?? els.find((e) => label(e).startsWith(t));
  };
  const setValue = (input, v) => {
    const proto = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value").set.call(input, v);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  };
  window.__sc = {
    has: (t, sel) => !!find(t, sel),
    text: (t) => document.body.innerText.includes(t),
    click(t, sel, root) {
      const e = find(t, sel, root ? document.querySelector(root) : document);
      if (!e) throw new Error("nothing to click: " + t);
      e.click();
      return true;
    },
    clickTitle(title) {
      const e = [...document.querySelectorAll("main .row .title")].find((x) => x.textContent.trim() === title);
      if (!e) throw new Error("no library row: " + title);
      e.click();
      return true;
    },
    search(v) {
      const i = document.querySelector('input[aria-label="Search your mods"]');
      if (!i) throw new Error("no library search box");
      setValue(i, v);
      i.blur();
      return true;
    },
    fill(sel, v) {
      const i = document.querySelector(sel);
      if (!i) throw new Error("no field: " + sel);
      setValue(i, v);
      return true;
    },
    tidy() {
      // Only a toast's Dismiss: its other button is Undo, which would revert the step just taken.
      for (const b of document.querySelectorAll('.toast button[aria-label="Dismiss"]')) b.click();
      document.activeElement?.blur?.();
      window.getSelection()?.removeAllRanges();
      return true;
    },
    pageText() {
      const parts = [document.body.innerText, document.title];
      for (const i of document.querySelectorAll("input, textarea")) parts.push(i.value, i.placeholder);
      for (const e of document.querySelectorAll("[title], [aria-label]")) parts.push(e.getAttribute("title"), e.getAttribute("aria-label"));
      return parts.filter(Boolean).join("\\n");
    },
  };
})()`;

/** Wrap the CDP page with the shot-writing helpers the shot list uses. */
function driver(page, { profile, lib, out, personal }) {
  const js = (s) => JSON.stringify(s);
  const d = {
    page,
    lib,
    profile,
    async helpers() {
      await page.eval(PAGE_HELPERS);
    },
    async click(text, opts = {}) {
      await d.helpers();
      await page.waitFor(`(${PAGE_HELPERS}, __sc.has(${js(text)}${opts.sel ? `, ${js(opts.sel)}` : ""}))`, opts.timeout ?? 15_000, `button "${text}"`);
      await page.eval(`__sc.click(${js(text)}, ${js(opts.sel ?? undefined)}, ${js(opts.within ?? undefined)})`);
      await sleep(opts.settle ?? 350);
    },
    async waitText(text, timeout = 20_000) {
      await d.helpers();
      await page.waitFor(`document.body.innerText.includes(${js(text)})`, timeout, `text "${text}"`);
    },
    async waitGone(text, timeout = 20_000) {
      await page.waitFor(`!document.body.innerText.includes(${js(text)})`, timeout, `text gone "${text}"`);
    },
    async search(v) {
      await d.helpers();
      await page.eval(`__sc.search(${js(v)})`);
      await sleep(400);
    },
    async openMod(title) {
      await d.helpers();
      await page.waitFor(`[...document.querySelectorAll("main .row .title")].some((x) => x.textContent.trim() === ${js(title)})`, 10_000, `row ${title}`);
      await page.eval(`__sc.clickTitle(${js(title)})`);
      await sleep(800);
    },
    async invoke(cmd, args = {}) {
      return page.eval(`window.__TAURI_INTERNALS__.invoke(${js(cmd)}, ${js(args)})`);
    },
    async reload() {
      await page.eval("location.reload()");
      await sleep(1500);
      await page.waitFor(`document.readyState === "complete" && !!document.querySelector("nav, main")`, 30_000, "app after reload");
      await sleep(1500);
    },
    async go(view) {
      await d.click(view);
      await sleep(600);
    },
    /** Tidy, check for personal text, then write `<id>@2x.png`. */
    async shot(id) {
      await d.helpers();
      // The real pointer may sit over the window: move it to a blank spot so nothing shows hover.
      await page.send("Input.dispatchMouseEvent", { type: "mouseMoved", x: 1180, y: 150 });
      await page.eval("__sc.tidy()");
      await sleep(500);
      const text = await page.eval("__sc.pageText()");
      const m = text.match(personal);
      if (m) throw new Error(`shot ${id} shows personal text (${JSON.stringify(m[0])}); not saved`);
      const png = await page.screenshot();
      const file = path.join(out, `${id}@2x.png`);
      fs.writeFileSync(file, png);
      console.log(`  ✓ ${id}@2x.png`);
    },
  };
  return d;
}

async function main() {
  const opts = parseArgs(process.argv.slice(2));
  // A fresh stage every run (about half a minute) keeps the shots reproducible: the crash and
  // bisection shots leave the profile mid-diagnosis.
  if (opts.restage) stage(opts.profile);
  if (!fs.existsSync(path.join(opts.profile, "silo-profile.marker"))) throw new Error(`${opts.profile} isn't a staged profile: run scripts/showcase/stage.mjs first`);
  const lib = JSON.parse(fs.readFileSync(path.join(opts.profile, "showcase.json"), "utf8"));
  const ids = opts.only ?? ORDER;
  for (const id of ids) if (!SHOTS[id]) throw new Error(`unknown shot: ${id} (known: ${ORDER.join(", ")})`);

  if (opts.build || !fs.existsSync(exe)) {
    console.log("building the debug app (npx tauri build --debug --no-bundle)…");
    const b = spawnSync("npx tauri build --debug --no-bundle", { cwd: repo, stdio: "inherit", shell: true });
    if (b.status !== 0) throw new Error("debug build failed");
  }
  fs.mkdirSync(opts.out, { recursive: true });

  const before = fingerprintRealProfile();
  fs.mkdirSync(path.join(opts.profile, "logs"), { recursive: true });
  const log = fs.openSync(path.join(opts.profile, "logs", "capture-app.log"), "w");
  const app = spawn(exe, [], {
    cwd: opts.profile,
    env: { ...process.env, SILO_PROFILE_DIR: opts.profile, SILO_PROFILE_CDP_PORT: String(opts.port) },
    stdio: ["ignore", log, log],
    detached: opts.keepOpen,
  });
  let exited = false;
  app.on("exit", () => (exited = true));

  const outside = [];
  let page;
  try {
    page = await connectToApp(opts.port);
    // Any request that leaves the app (not its own assets, IPC or inline data) fails the run.
    await page.send("Network.enable");
    page.on("Network.requestWillBeSent", ({ request }) => {
      if (!/^(https?:\/\/(tauri|ipc|asset)\.localhost|data:|blob:|ipc:|tauri:)/.test(request.url)) outside.push(request.url);
    });
    await page.send("Emulation.setDeviceMetricsOverride", { width: 1600, height: 1000, deviceScaleFactor: 2, mobile: false });
    await page.send("Emulation.setEmulatedMedia", { features: [{ name: "prefers-color-scheme", value: "dark" }] });
    await page.waitFor(`location.hostname === "tauri.localhost" && document.readyState === "complete"`, 60_000, "the app page");
    await page.eval(`localStorage.setItem("silo.theme", "dark"); localStorage.setItem("silo.uiScale", "1"); true`);
    const d = driver(page, { profile: opts.profile, lib, out: opts.out, personal: personalTextPattern() });
    await d.reload();

    await SHOTS.prime.run(d);
    console.log(`capturing ${ids.length} shots into ${opts.out}`);
    const alt = fs.existsSync(path.join(opts.out, "alt.json")) ? JSON.parse(fs.readFileSync(path.join(opts.out, "alt.json"), "utf8")) : {};
    for (const id of ids) {
      await SHOTS[id].run(d);
      alt[`${id}@2x.png`] = SHOTS[id].alt;
    }
    const ordered = Object.fromEntries(ORDER.map((id) => [`${id}@2x.png`, alt[`${id}@2x.png`]]).filter(([, v]) => v));
    fs.writeFileSync(path.join(opts.out, "alt.json"), JSON.stringify(ordered, null, 2) + "\n");

    if (opts.keepOpen) {
      console.log(`--keep-open: Silo stays open on DevTools port ${opts.port}; close it when done`);
      return;
    }
  } finally {
    page?.close();
    if (opts.keepOpen) {
      // Left running (also after a failure, to look at what went wrong).
      app.unref();
      fs.closeSync(log);
    } else {
      if (!exited) app.kill();
      await sleep(1500);
      fs.closeSync(log);
    }
  }
  assertUnchanged(before);
  console.log("real profile unchanged");
  if (outside.length) throw new Error(`requests left the app:\n  ${[...new Set(outside)].join("\n  ")}`);
  console.log("no network requests left the app");
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((e) => {
    console.error(e.message ?? e);
    process.exit(1);
  });
}
