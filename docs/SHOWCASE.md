# Showcase kit

The screenshots on silo.hllmr.com come from the real app, running a **debug build** against a
staged demo library. Nothing in the kit reads or writes your real FS25 folder, your Silo data or
your accounts. This page explains how to make a fresh set.

## Quick start (Windows)

```bash
node scripts/showcase/capture.mjs
```

That one command:

1. stages a fresh demo profile in `<drive>:\silo-demo` (about 0.9 GB, half a minute);
2. builds the debug app if `src-tauri/target/debug/silo.exe` is missing (`--build` forces it);
3. starts it on the demo profile, finishes setup (onboarding, organizing, three mod sets);
4. puts the app in each state and saves `showcase-out/<id>@2x.png` plus `showcase-out/alt.json`;
5. quits it, then checks that your real profile didn't change and that no request left the app.

Shots are 3200×2000 PNGs: the page laid out at 1600×1000 and rendered at 2×, dark theme, with no
window frame (the site draws its own). `showcase-out/` is gitignored.

| Flag | What it does |
|---|---|
| `--only library,updates` | Capture just these shots (setup still runs first). |
| `--no-stage` | Reuse the profile as it is, for quick iteration. The crash and bisection shots leave it mid-diagnosis, so restage for a clean full set. |
| `--keep-open` | Leave Silo running afterwards (also after a failure), with DevTools on port 9333. |
| `--profile D:\demo` | Stage and run somewhere else. Never inside your home folder: the script refuses. |
| `--out dir` | Where the PNGs and `alt.json` go. |
| `--build` | Rebuild the debug app first (`npx tauri build --debug --no-bundle`). |

To stage without capturing, for example to click around yourself:

```bash
node scripts/showcase/stage.mjs
```

Then start the debug build on it. In PowerShell:

```powershell
$env:SILO_PROFILE_DIR = "F:\silo-demo"; .\src-tauri\target\debug\silo.exe
```

## The shots

Ids match the site's shot list. `alt.json` has a short alt line for each.

| Id | Shows |
|---|---|
| `library` | Library, status strip all good, nothing selected (hero). |
| `crash-notice` | The crash notice naming the likely mod, with **Park**. |
| `diagnose-last-run` | Diagnose › Last run, mods ranked by logged errors. |
| `find-problem-mod` | Find the problem mod, round 1, half the mods parked. |
| `conflicts` | Diagnose › Conflicts: the kit's one real conflict (a `uniqueType` clash) and **Park one…**. |
| `verified-build` / `modified-build` | The mod drawer with **Verified build** / **Modified (3 files)**. |
| `mod-sets` | My sets: three sets, Harvest 2026 active. |
| `share-set` | The Share dialog after **Create link**. |
| `updates` | Updates: GitHub releases ready with **Update all**, newer ModHub versions. |
| `browse` | Browse: sources spelled out, install states. |
| `help-*` | Help: Mods folder settings, Organize preview, **Undo organizing** confirm, Savegames, Multiplayer check file, Fill type bridge, Controls map, Settings › Accounts, Copy diagnostics. |

Not staged: **Library archive move (in progress)**. It needs a live move slow enough to pause on
camera.

The order is deliberate. The conflict, crash and bisection shots run last because they change the
library's state. To add a shot, add an entry to `scripts/showcase/lib/shots.mjs` (its steps and
alt text) and put its id in `ORDER`.

## The demo library

`scripts/showcase/lib/catalog.mjs` builds it from a fixed seed, so every run is the same:

- **693 fictional mods** across real FS store categories, with invented brands, authors, maps and
  GitHub owners. No real mod or modder appears, least of all as a crash culprit. Icons are
  generated shapes. Each zip has a real `modDesc.xml`, a store item where it has one, and filler
  so sizes look plausible.
- **One real conflict:** *Field Helper AI* and *Smart Worker Routes* share
  `<uniqueType>FIELD_HELPER_AI</uniqueType>` (different authors, so critical). Only the first is
  active; the `conflicts` shot switches the second on, then off again. Only one map is active, so
  there's no multi-map Note.
- **Logs:** a clean `log.txt` from two hours earlier, and `fixtures/log-crash.txt`, which stops
  mid-load with Lua errors from *Hollow Creek Grain Dryer*. The crash shot copies it over
  `log.txt` and reloads, as if you were coming back from the game.
- **Three savegames**, a control map (`inputBinding.xml`) and three new downloads (in
  `fixtures/new-downloads/`) for the Organize preview.
- **Mod sets:** Harvest 2026 (the active mods), Marlow Flats Grain and Multiplayer night.

Avoid `&` in demo titles and authors: Silo currently cuts modDesc text at an escaped ampersand.

## How the isolation works

**Debug builds only.** When `SILO_PROFILE_DIR` names a directory holding
`silo-profile.marker`, `src-tauri/src/profile.rs` moves everything under it:

| What | Normally | In a profile |
|---|---|---|
| FS25 user folder | `Documents\My Games\FarmingSimulator2025` | `<profile>\FarmingSimulator2025` |
| `silo.db` and snapshots | `%APPDATA%\com.hllmr.silo` | `<profile>\app-data` |
| Icon cache, log | `%LOCALAPPDATA%\com.hllmr.silo` | `<profile>\cache`, `<profile>\logs` |
| Webview storage | `%LOCALAPPDATA%\com.hllmr.silo\EBWebView` | `<profile>\webview` |
| Account tokens | OS keychain | the profile's `silo.db` (the keychain is never opened) |

It also:

- runs under the identifier `com.hllmr.silo.profile`, so an installed Silo that's open is left
  alone (separate single-instance lock);
- refuses **Play**: the game would load your real mods, not the demo's;
- never goes online. The updater plugin isn't loaded, and commands named in `<profile>\stubs.json`
  return its canned answer instead of running. The stage writes answers for every command that
  would reach SiloAPI or GitHub: catalog, Browse, Updates, provenance, the GitHub account
  (`silo-demo`, made up, no token anywhere) and sharing. Downloads and sign-in fail with a plain
  "offline" message.

A set but invalid `SILO_PROFILE_DIR` (relative, missing, no marker) or a malformed `stubs.json`
stops Silo at startup rather than falling back to your real profile. Release builds ignore the
variable entirely. `SILO_PROFILE_CDP_PORT` opens the webview's DevTools protocol on that port;
the capture script drives the app through it.

The capture script adds three checks of its own:

- it fingerprints (path, size, modified time) your real FS25 folder and Silo's real data folders
  before and after, and fails if anything changed;
- it fails if the webview requested anything outside the app;
- it refuses to save a shot whose text contains your home folder or user name.

Still look through the images before publishing them.

## Requirements

Windows 10/11 with WebView2 (the DevTools hook is WebView2's), Node 22 or newer (built-in
`WebSocket`, `zlib.crc32`), and the usual Tauri build tools.
