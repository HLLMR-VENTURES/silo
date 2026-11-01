# Silo — Claude project notes

Working title: **Silo**. A Tauri desktop app to manage the Farming Simulator 25
mod library. This is a standalone **application**, not an in-game mod — so it does
NOT follow the `FS25_<ModName>` / Lua-mod conventions in the workspace root
`CLAUDE.md`. Those apply to game mods; the shared "never invent GIANTS API" rule
is irrelevant here because Silo never runs inside the game engine.

## What this app is

The management layer FS25 lacks: organize a curated **source library**, define
**profiles/loadouts**, detect **conflicts**, track **updates**, and bind mods to
**savegames** — projecting the active set into the game's flat `mods/` folder only
at launch (via symlink/junction, with a copy fallback). See [docs/](docs/):
`MVP.md`, `ARCHITECTURE.md`, `SMOKE_TEST.md`, and `reference/`.

### Feature set (shipped as of 2026-07)

Beyond the MVP management layer above:

- **Browse tab** — the SiloAPI canonical catalog (GitHub + ModHub), search,
  category filter, pagination, per-source buttons w/ versions, in-app GitHub install
  with a streaming progress bar, detail drawer. ModHub is index-only
  (open-page; their CDNs gate direct download).
- **Catalog-routed updates** — "⟳ Updates" checks the whole library against the
  catalog's latest-across-sources (fixes the GitHub-vs-ModHub false-"outdated" bug);
  per-mod update status also in the detail drawer.
- **Crash & log triage** (`◆ diagnose`) — parses `log.txt`, names the culprit mod,
  separates real errors from cosmetic noise.
- **Guided bisection** — automates "disable half, relaunch" to isolate a crash the log
  can't name; crash-safe (snapshots + restores the active set).
- **Conflict detection** — uniqueType/specialization/script collisions, **cross-mod fillType
  override** (same-name last-wins; SDK-verified true-positive-only), and a Note when more than one
  map is active (FS25 loads only the savegame's map: in-game test 2026-10-18, so not a problem).
- **Bindings map** (`⌨ bindings`) — the full control map from `inputBinding.xml`,
  grouped by device, reused inputs highlighted (a view, not a verdict).
- **Multiplayer sync** — export a hashed manifest of the active set; a joiner verifies
  theirs against it (fix-list: missing / wrong version / different file / extra).
- **Filltype bridge** (`⛓ bridge`) — GENERATES a companion mod that adds a stubborn map
  filltype into the categories your equipment accepts (the "sugar beet" fix), no vehicle
  edits. Generator lives in Silo; output is per-user, no separate repo.
- **Mod integrity / provenance** (detail drawer) — hashes an installed mod and verifies
  it against the canonical build SiloAPI hashed from source: **Verified** / **Modified**
  (with the exact changed files) / **Unverified**. Byte-exact via a cross-language-ratified
  manifest format (`provenance.rs`, `docs/PROVENANCE.md` + SiloAPI `PROVENANCE-CANONICALIZATION.md`).
  Provenance, not antivirus — the moat, since no competitor holds a cross-source canonical-hash DB.
- **Organize preview** — the manual Organize shows a read-only dry run (what moves where)
  before touching a file; dismissible for power users.
- **Mods folder selection** (`modsroot`, #162) — follows FS25's `modsDirectoryOverride`, lets the
  user pick any folder (optionally writing the override back, with a `.bak`), and keeps organize
  state per folder (`root` key on `organized` / `projection`). `allowed_roots()` returns the
  resolved active root, so every write command follows the chosen folder. Linux/Proton prefix
  and Mac App Store discovery live in `fsgame::locate()`.
- **Library archive location** (`relocate`, #166) — a mods folder's archive can live on any drive
  (`archive_root:<mods key>`). Moving it is a resumable job with a DB marker: copy to a
  `.silo-move-tmp`, verify, place without ever replacing an existing file, then delete the original
  by rename-aside; Pause / Resume / Cancel (cancel moves back what was moved), refused — and
  auto-paused — while FS25 runs (`gamelaunch::fs25_running`). Pointing at a folder that already
  holds mods offers to adopt them via the Organize preview. Across drives, projection is
  hardlink → symlink → copy (`organize::LinkPolicy`).

Backend: **SiloAPI** (separate repo `HLLMR-VENTURES/silo-api`, live at `https://silo-api.hllmr.com`).

## Stack & structure

Rust core modules (`src-tauri/src/`): `scan` `moddesc` `category` `icons` `db` `store`
`fsgame` `modsroot` (mods-folder resolution) `organize` `relocate` (archive move job) `savegame` `conflicts` `settings_form`
`xmlconfig` `gamelaunch` `gameprofile` (FS25 `-profile` folders, #387) `github` `siloapi` (catalog client) · `logscan` (crash triage) ·
`bisect` (guided bisection) · `bindings` (input map) · `mpsync` (MP manifest) · `bridge`
(filltype companion generator) · `provenance` (integrity/manifest hashing) · `collection`
(shareable mod sets) · `net` (outbound-URL SSRF guard) · `paths` `secrets` `xmltext` (safety/util). Each is
pure logic behind thin `#[tauri::command]` wrappers with unit tests (230+ Rust tests; some are
`#[cfg(unix)]` and only run on Linux/macOS CI). Frontend panels in `src/lib/components/`.

- **Tauri v2**: Rust core in `src-tauri/`, web frontend in `src/`.
- **Frontend: Svelte 5 (runes) + Vite + TypeScript**, hand-authored CSS design
  tokens, no heavyweight UI kit. Virtualized lists mandatory. Design language in
  `docs/DESIGN.md`.
- Rust does ALL heavy work (zip parsing, hashing, DDS/image decode, tree walks)
  on a thread pool — never block the UI.
- SQLite (rusqlite/sqlx) with real indexes; cache keyed by path+mtime+size.
- **Cross-platform: Windows + macOS + Linux** (see `docs/CROSS-PLATFORM.md`).
  Per-OS game-file discovery and projection; never hardcode Windows paths.
- **No source obfuscation in builds.**

## Working principles

- **Never touch the user's game files destructively without a reversible path.**
  Prefer symlink/junction projection over moving/copying originals. Every
  write to `Documents/My Games/FarmingSimulator2025/` must be undoable.
- **Off-thread by construction.** If it parses an archive, hashes, decodes an
  image, or walks a tree, it lives in Rust on a worker — not in an `invoke`
  handler that blocks, and never in the frontend.
- **Parse, don't regex.** `modDesc.xml` and savegame XML go through `quick-xml`,
  not string scraping.
- **Windows-first, but symlink-safe.** File symlinks need Developer Mode/admin;
  dir junctions don't; hardlinks fail across volumes. Detect capability and fall
  back to copy-projection.
- **Cache aggressively, invalidate honestly** (path+mtime+size). Never wipe the
  whole cache on version bumps.
- Small, testable Rust modules; keep the scan/library/projection/conflict
  engines as pure logic behind thin Tauri command wrappers so they unit-test
  without a running app.
- **Worktrees live under one root, `F:\Worktrees\<repo>\<topic>`** (estate rule,
  hllmr-infra AGENTS.md), never inside or beside a canonical checkout. Remove your
  own worktree with `git worktree remove` once its PR merges or closes; a dirty
  worktree is a stop-and-ask.

## Reference material

- FS25 SDK / game source: see workspace root `CLAUDE.md` and the memory index
  (`fs25-sdk-resource-locations`). Authoritative schemas live at
  `<game install>/shared/xml/schema/*.xsd` (88 of them; `modDesc.xsd`,
  `gameSettings.xsd`, `savegame_*.xsd` are the ones we use).
- **Local reference mirror:** `reference/` (gitignored) holds `schema/modDesc.xsd`,
  `schema/gameSettings.xsd`, and `samples/` — GIANTS' copyrighted files + the user's
  real save/mod samples, kept OUT of the public repo. Distilled facts are committed
  in `docs/reference/fs25-modding-notes.md`.
- Reference mod corpus: `Documents/My Games/FarmingSimulator2025/mods/` (**729**
  real mods on this machine) — the scan/conflict engine's test fixtures. NOTE:
  `modManagerTemplates/` and `modManagerArchives/` there are folders used by another
  mod manager — do not reuse those names.

## Per-change validation

1. Does any file-mutating path stay reversible? State how to undo it.
2. Did heavy work stay off the UI thread (Rust worker, not blocking invoke)?
3. Which real mod folder / savegame did you test against, and what was observed?
4. XML handled by a parser, not regex?
