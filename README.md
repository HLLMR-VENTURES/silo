<p align="center">
  <img src="landing/og.png" alt="Silo — the Farming Simulator 25 mod manager" width="720" />
</p>

<h1 align="center">Silo</h1>

<p align="center"><em>The mod manager Farming Simulator 25 should have shipped with.</em></p>

<p align="center">
  <a href="https://github.com/HLLMR/silo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/HLLMR/silo?sort=semver&color=4a7330&label=release"></a>
  <a href="https://github.com/HLLMR/silo/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/HLLMR/silo/actions/workflows/ci.yml/badge.svg"></a>
  <a href="docs/VERIFY_DOWNLOAD.md"><img alt="Windows: code signed" src="https://img.shields.io/badge/Windows-code%20signed-4a7330"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-4a7330"></a>
  <img alt="Platforms" src="https://img.shields.io/badge/platforms-Windows%20%C2%B7%20macOS%20%C2%B7%20Linux-4a7330">
  <a href="https://github.com/HLLMR/silo/releases"><img alt="Downloads" src="https://img.shields.io/github/downloads/HLLMR/silo/total?color=e3b23c&label=downloads"></a>
  <a href="https://silo.hllmr.com"><img alt="Website" src="https://img.shields.io/badge/web-silo.hllmr.com-4a90c2"></a>
</p>

A desktop app for your FS25 mod library that stays quick at 700+ mods. Silo reads the
game log to name the mod that crashed you, flags conflicts before you launch, and shows
the latest version it can find across ModHub and GitHub. Everything it changes on
disk is reversible. **Free and open source. Windows (tested) · macOS & Linux (experimental).**

<p align="center">
  <b><a href="https://github.com/HLLMR/silo/releases/latest">⬇ Download the beta</a></b>
  &nbsp;·&nbsp; <a href="https://silo.hllmr.com">silo.hllmr.com</a>
  &nbsp;·&nbsp; <a href="#install">Build from source</a>
</p>

> **Status: public beta.** Windows installers are [code-signed](docs/VERIFY_DOWNLOAD.md) by a
> verified publisher (SmartScreen may still prompt on new releases while reputation builds, but
> shows the publisher, not "unknown"). macOS/Linux builds are OS-unsigned — Gatekeeper may warn.
>
> **Platforms: Windows is the tested, supported platform.** macOS (Intel & Apple Silicon)
> and Linux builds are **experimental and largely untested on real machines** — we'd love
> testers on those platforms, but expect rough edges. Silo runs natively on both; on Linux it
> finds FS25's folder inside the Steam Proton prefix (native or Flatpak Steam), and on macOS it
> handles Steam, GIANTS and Mac App Store installs. See [`docs/CROSS-PLATFORM.md`](docs/CROSS-PLATFORM.md).

## Why

FS25 reads one flat `mods/` folder, so everything you own is always active at once. When
something breaks, the game won't tell you what. There's no conflict detection, no
organization, no update tracking, no way to bind a loadout to a savegame. Silo
is the management layer the game left out.

## What it does

- **Crash & log triage** — parses `log.txt`, separates real errors from cosmetic noise,
  and names the mod at fault.
- **Conflict detection** — duplicate active maps (an instant crash), plus colliding
  filltypes, vehicle types and scripts across your active set.
- **Cross-source catalog** — one record per mod aggregating ModHub + GitHub,
  with the latest version found across all of them. See the
  [verified catalog page](https://silo.hllmr.com/trust/) or
  [browse it live](https://silo.hllmr.com/browse/); it's served by the public
  [SiloAPI](https://silo-api.hllmr.com) read API.
- **Integrity check** — hashes an installed mod and compares it to the trusted build its
  source published: a clean match, or the exact files that changed. Provenance, not
  antivirus — it confirms what a mod is, not whether it means well. It works across
  ModHub and GitHub — a cross-source integrity check that's rare among mod tools.
- **Guided bisection** — when the log can't name the culprit, automates "disable half,
  relaunch" to isolate it, and safely restores your active set afterward.
- **Loadouts & projection** — curate profiles and project only the active set into the
  game's flat folder at launch (hardlinks for zips, junctions/symlinks for folder mods, copies
  where links aren't allowed). Organizing files your zips into a reversible local archive;
  *Restore vanilla layout* puts everything back.
- **Any mods folder** — follows FS25's own mods-folder override, or pick any folder in
  Settings; Silo can point the game at it too (backing up `gameSettings.xml`) and keeps each
  folder's organized library separately.
- **Library archive on any drive** _(after v0.8.0)_ — move your parked mods to a bigger drive
  while the mods folder stays where the game reads it. Every mod is verified before the
  original is removed; the move can be paused to play, or cancelled to put everything back.
- **Collections** — share a mod-set as a link (secret gist or public repo); others preview it
  against their library before importing.
- **Per-source actions** — star a repo on GitHub, rate on ModHub, all
  through *your own* accounts. Silo just opens the door; your credentials stay yours.
- **Multiplayer sync**, a **filltype-compatibility bridge generator**, **savegame &
  graphics settings editing** (with `.bak` backups), a **mod-settings editor**, **savegame
  backup**, and a full **control-binding map**.

## Install

**Download:** grab the installer for your OS from
[Releases](https://github.com/HLLMR/silo/releases). Windows builds are code-signed — you can
[verify the signature, checksum, and build provenance](docs/VERIFY_DOWNLOAD.md). macOS/Linux
builds are OS-unsigned, so Gatekeeper may warn on first launch.

**Build from source:** needs [Rust](https://rustup.rs) + [Node 20+](https://nodejs.org)
and the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm ci
npm run tauri:dev     # run in dev
npm run tauri:build   # produce an installer in src-tauri/target/release/bundle/
```

## Trust

- **Open source** — the desktop app is fully open here; audit, fork, or build it yourself.
  The hosted catalog (SiloAPI) is a separate service.
- **No telemetry, no account** — Silo hits the network only for catalog search, update and
  integrity checks, cover images, and the source actions you choose, sending just what those
  need (search terms; mod IDs and versions). No telemetry, no analytics; your saves, mod
  files and notes never leave your machine.
- **Reversible, and it owns what it touches** — it files your mods into a local archive
  and links the active set into the game; every move is recorded and undoable, it only
  ever removes files it put there itself (your own edits are left alone), and it backs up
  before any overwrite.
- **Verifiable** — the integrity check proves an installed mod matches the build its
  source published, byte for byte.

## Stack

Tauri v2 (Rust core, all heavy work off the UI thread) + Svelte 5 + TypeScript, SQLite
cache (path + mtime + size), quick-xml parsing. See [`docs/`](docs/) —
`ARCHITECTURE.md`, `MVP.md`, and per-feature notes.

## Contributing

Issues and PRs welcome. `npm run check` (svelte-check) and `cargo test` (in `src-tauri/`)
must pass — CI runs both. Please keep heavy work off the UI thread and file writes
reversible (see [`CLAUDE.md`](CLAUDE.md) for conventions).

## Built with AI assistance

Silo was built with heavy use of AI-assisted engineering (Claude Code). The
architecture, the security and reliability decisions, the testing, and every release are
human-owned and human-reviewed — including acting on rounds of external code critique
(informal outside review, not a formal third-party security audit) before promoting the
beta. The desktop source is fully open here so you don't have to take that on faith: read
it, audit it, build it yourself.

## License

[MIT](LICENSE) © HLLMR. Use it, fork it, ship it.

---

_Not affiliated with GIANTS Software. Farming Simulator is a trademark of its respective
owner._
