<p align="center">
  <img src="https://silo.hllmr.com/og.png" alt="Silo. More farming. Less fixing." width="720" />
</p>

<h1 align="center">Silo</h1>

<p align="center"><em>More farming, less fixing. A mod manager for Farming Simulator 25.</em></p>

<p align="center">
  <a href="https://github.com/HLLMR-VENTURES/silo/releases/latest"><img alt="Latest release" src="https://img.shields.io/github/v/release/HLLMR-VENTURES/silo?sort=semver&color=4a7330&label=release"></a>
  <a href="https://github.com/HLLMR-VENTURES/silo/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/HLLMR-VENTURES/silo/actions/workflows/ci.yml/badge.svg"></a>
  <a href="docs/VERIFY_DOWNLOAD.md"><img alt="Windows: code signed" src="https://img.shields.io/badge/Windows-code%20signed-4a7330"></a>
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-4a7330"></a>
  <img alt="Platforms: Windows · macOS and Linux preview" src="https://img.shields.io/badge/platforms-Windows%20%C2%B7%20macOS%20and%20Linux%20preview-4a7330">
  <a href="https://silo.hllmr.com"><img alt="Website" src="https://img.shields.io/badge/web-silo.hllmr.com-4a90c2"></a>
</p>

Silo reads your mods and your game log, points to the likely culprit when the game crashes,
and flags conflicts before you launch. It keeps hundreds of mods in a tidy library, never
edits inside one, and gives the game exactly the set you chose. **Free and open source.
Windows (tested) · macOS and Linux (preview).**

<p align="center">
  <b><a href="https://github.com/HLLMR-VENTURES/silo/releases/latest">⬇ Download for Windows</a></b>
  &nbsp;·&nbsp; <a href="https://github.com/HLLMR-VENTURES/silo/releases/latest">macOS and Linux preview</a>
  &nbsp;·&nbsp; <a href="https://silo.hllmr.com">silo.hllmr.com</a>
  &nbsp;·&nbsp; <a href="#install">Build from source</a>
</p>

> **Status: 1.0.** Windows installers are [code-signed](docs/VERIFY_DOWNLOAD.md) by
> David Hellmer, Silo's developer. SmartScreen may still prompt on new releases while
> reputation builds, but it shows the publisher instead of "unknown". macOS/Linux builds are
> OS-unsigned, so macOS blocks the first launch until you click **Open Anyway** in System
> Settings → Privacy & Security.
>
> **Platforms: Windows is the tested, supported platform.** macOS (Intel & Apple Silicon)
> and Linux builds are **a preview and largely untested on real machines**. We'd love
> testers on those platforms, but expect rough edges. Silo runs natively on both; on Linux it
> finds FS25's folder inside the Steam Proton prefix (native or Flatpak Steam), and on macOS it
> handles Steam, GIANTS and Mac App Store installs. See [`docs/CROSS-PLATFORM.md`](docs/CROSS-PLATFORM.md).

## Why

FS25 reads one `mods/` folder, so everything in it is active at once. With hundreds of mods
installed, finding the one that broke your game can take the whole evening. Silo adds a
management layer on top: crash triage, conflict checks before launch, update tracking,
and mod sets you can switch between playthroughs.

## What it does

- **Crash & log triage** — parses `log.txt`, separates real errors from cosmetic noise,
  and ranks mods by the errors they logged, so you can see the likely culprit.
- **Conflict detection** — flags more than one active map before launch, plus colliding
  filltypes, vehicle types and scripts across your active set.
- **Cross-source catalog** — one record per mod aggregating ModHub + GitHub,
  with the latest version found across both. See the
  [verified catalog page](https://silo.hllmr.com/trust/) or
  [browse it live](https://silo.hllmr.com/browse/); it's served by the public
  [SiloAPI](https://silo-api.hllmr.com) read API.
- **Integrity check** — hashes an installed mod on your computer and compares it to the
  build its source published: a clean match, or the exact files that changed. Provenance, not
  antivirus — it confirms what a mod is, not whether it means well. It covers mods from
  GitHub, and ModHub mods the catalog has already hashed.
- **Find the problem mod** — when the log can't name the culprit, a guided search parks half
  your mods per test launch to isolate it, and safely restores your active set afterward. Maps
  stay on every round.
- **Mod sets** — save your active mods as a set and switch between sets. Switching a mod on or off adds or removes its link in the
  game's flat folder right away (hardlinks for zips, junctions/symlinks for folder mods, copies
  where links aren't allowed, such as cloud-synced folders). Launch just starts the game.
  Organize files your zips into a local archive and shows every move before it touches a
  file; *Undo organizing* (Settings → Maintenance) puts every mod back in your mods folder,
  where the game loads all of them again.
- **Any mods folder** — follows FS25's own mods-folder override, or pick any folder in
  Settings; Silo can point the game at it too (backing up `gameSettings.xml`) and keeps each
  folder's organized library separately.
- **Library archive on any drive** — move your parked mods to a bigger drive
  while the mods folder stays where the game reads it. Every mod is verified before the
  original is removed; the move can be paused to play, or cancelled to put everything back.
- **Shared sets** — share a mod set as a link (an unlisted gist on your GitHub), a `.silo` file,
  or a Multiplayer check file. Others preview it against their library in *Mod sets → Open a
  shared set*, then use it: Silo downloads the GitHub mods, checks them against the pinned
  build and saves it as a mod set.
- **Installs and updates** — GitHub releases install in one click, checked against the
  catalog's hash when it has one. Updates keep the previous version in `backups/` (newest
  three per mod) and finish or roll back if Silo stops mid-update. ModHub mods download from
  ModHub; Silo picks them up from your mods folder.
- **Per-source actions** — star a repo on GitHub through *your own* account, or open the
  ModHub page to rate a mod. Your GitHub token stays on your machine.
- **Multiplayer check** (does your active set match the server's?), a **fill type bridge**
  generator, **savegame & graphics settings editing** (with `.bak` backups), a **mod-settings
  editor**, **savegame backup**, and a full **controls map** (Tools).

## Install

**Download:** grab the installer for your OS from
[Releases](https://github.com/HLLMR-VENTURES/silo/releases). Windows builds are code-signed — you can
[verify the signature, checksum, and build provenance](docs/VERIFY_DOWNLOAD.md). macOS/Linux
builds are OS-unsigned, so Gatekeeper may warn on first launch. On Windows, the setup `.exe`
installs per user with no admin rights (recommended); the `.msi` installs for all users and
needs admin rights. Linux ships as `.AppImage`, `.deb` and `.rpm`.

**Build from source:** needs [Rust](https://rustup.rs) + [Node 22](https://nodejs.org)
and the [Tauri v2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS.

```bash
npm ci
npm run tauri:dev     # run in dev
npm run tauri:build   # produce an installer in src-tauri/target/release/bundle/
```

## Trust

- **Open source** — the desktop app's source is here under MIT; audit, fork, or build it
  yourself. The hosted catalog (SiloAPI) is a separate service.
- **No account, no ads, no telemetry** — Silo contacts its catalog only when you ask:
  searching Browse, opening a mod's details, checking for updates, or verifying a mod. Each
  request carries what it needs (search terms, or the tech names of the mods being checked,
  plus the version for an integrity check). It also checks GitHub for its own app updates at
  launch, which sends no library data. Your saves, mod files and notes are never uploaded.
  The [privacy policy](https://silo.hllmr.com/legal/privacy-policy.html) lists every connection.
- **Undoable, and it owns what it touches** — it files your mods into a local archive and
  links the active set into the game. Organize shows every move before it touches a file;
  *Undo organizing* puts every mod back in your mods folder. It only ever removes files
  it put there itself (a copied folder mod you edited goes to `backups/` instead), and it backs
  up settings and savegames before editing them.
- **Verifiable** — the integrity check compares an installed mod to the build its source
  published, file by file.

## Stack

Tauri v2 (Rust core, all heavy work off the UI thread) + Svelte 5 + TypeScript, SQLite
cache (path + mtime + size), quick-xml parsing. See [`docs/`](docs/) —
`ARCHITECTURE.md`, `DESIGN.md`, and per-feature notes.

The website at [silo.hllmr.com](https://silo.hllmr.com), including the help pages and the legal
pages (privacy policy, terms, disclaimer, takedown), lives in its own repository,
[HLLMR-VENTURES/web](https://github.com/HLLMR-VENTURES/web).

## Community

Questions and ideas go to [Discussions](https://github.com/HLLMR-VENTURES/silo/discussions). For quick
help and release news, join the [Silo Discord](https://discord.gg/NpxcYdGHrW).

## Contributing

Issues and PRs welcome. `npm test`, `npm run build` (svelte-check + Vite) and, in
`src-tauri/`, `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings` and
`cargo test` must pass — CI runs them all. Please keep heavy work off the UI thread and file
writes reversible (see [`CONTRIBUTING.md`](CONTRIBUTING.md) and [`CLAUDE.md`](CLAUDE.md)).

## Built with AI assistance

Silo was built with heavy use of AI-assisted engineering (Claude Code). The
architecture, the security and reliability decisions, the testing, and every release are
human-owned and human-reviewed — including acting on rounds of external code critique
(informal outside review, not a formal third-party security audit) before each release. The desktop source is fully open here so you don't have to take that on faith: read
it, audit it, build it yourself.

## License

[MIT](LICENSE) © HLLMR Ventures LLC (Texas, USA). Use it, fork it, ship it.

---

_Not affiliated with GIANTS Software. Farming Simulator is a trademark of its respective
owner._
