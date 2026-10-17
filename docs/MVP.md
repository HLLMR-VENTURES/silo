# Silo — MVP Scope

The MVP focuses on speed, stability, and real management, and
**deliberately excludes the ModHub scraper.** Everything here works offline against
a local library.

## In scope (v0.1)

### 1. Fast library scan
- Discover mod folders: parse `gameSettings.xml` for `modsDirectoryOverride`, plus
  the default `Documents/My Games/FarmingSimulator2025/mods`. Support multiple roots.
- For each `.zip` or unpacked mod dir: read `modDesc.xml`, extract title/author/
  version/icon/dependencies/scripts/specializations, hash the file.
- **All in Rust on a thread pool**, streamed to the UI with progress.
- Cache in SQLite keyed by `path + mtime + size`; unchanged mods skip re-parsing.
- **Success bar:** a 700-mod library scans without freezing the UI, cold scan in
  seconds-not-minutes, warm scan near-instant.

### 2. Organization hierarchy
- Source library decoupled from the game folder.
- Folders/collections + tags + free-text search/filter.
- Mark mods: favorite, hidden, "broken/unfinished" (curation trail).

### 3. Profiles / loadouts
- Named mod sets. A profile = an ordered list of mods (by stable id/hash).
- **Project** a profile into the game's flat `mods/` at launch via symlink/junction;
  **copy fallback** when symlinks aren't permitted (no Dev Mode / cross-volume).
- Fully reversible: projecting or clearing never mutates library originals.
- Detect symlink capability up front and tell the user which mode they're in.

### 4. Conflict detection
- Static analysis from `modDesc.xml` across the active set:
  - duplicate `<specialization>` names (**critical**),
  - duplicate script basenames in `<extraSourceFiles>` (**warning**),
  - l10n / store-item / input-binding collisions (**info/warning**),
  - same mod present twice / version clash.
- Surface conflicts **before launch**, grouped by severity, with the offending mods.
- (Real modDesc `<conflicts>`/`<dependency>` declarations parsed properly.)

### 5. Update & health tracking (offline-honest)
- Track local versions with **real** version comparison (semver-ish, dotted).
- Flag: missing dependencies, duplicate installs, corrupt/unreadable zips,
  maps in the wrong place, mods whose folder name starts with a digit (FS ignores
  those — a silent footgun).
- No network source-of-truth yet; groundwork for a later ModHub/update feed.

### 6. Launch integration
- Launch the game with the projected active set.
- Read a savegame's bound mod list (`careerSavegame.xml`) to inform/build a profile.

### 7. XML settings editor (tinkerer feature)
- In each mod's detail/tinkerer area, locate the mod's settings XML (`modSettings/
  <modName>/`, in-archive config, or savegame-scoped) and **generate a stylized,
  editable form** from it — toggles/sliders/number fields/dropdowns inferred from
  values, and from the XSD where one applies.
- Write edits back with a minimal diff, **backing up the original first**; validate
  against the XSD when available; "reset to default"; always offer a raw-XML escape
  hatch. Design: `ARCHITECTURE.md#settings-form-generator`.
- Turns "edit the file in Notepad" into a first-class UI — a headline differentiator.
  (May ship as an early fast-follow if v0.1 core runs heavy, but it's planned in.)

## Explicitly out of scope for v0.1
- ModHub browsing/scraping/downloading.
- In-app editing of mod **content/assets** (models, scripts, balancing) — that's
  GIANTS Editor/Studio territory. (Editing a mod's **settings** XML *is* in scope —
  item 7.)
- Radio, system/graphics "optimization," cloud sync, DLC management (out of scope
  for v0.1).
- Auto-update of the app itself (add once there's something to update to).

## Risks / open questions
- Symlink permissions on Windows: quantify how many users lack Dev Mode; make copy
  fallback first-class, not an afterthought.
- Icon/preview extraction: `.dds` decoding — use a Rust crate, decode off-thread,
  cache the decoded PNG. Never a hand-rolled pixel loop on the UI thread.
- Huge map mods (100 MB+ zips): stream/partial-read the central directory; don't
  load whole archives into memory just for `modDesc.xml` + icon.
- Conflict detection accuracy: validate against the 700-mod reference corpus; a
  false "critical" that's actually fine will erode trust fast.

## Definition of done for v0.1
A user points Silo at their mods folder, sees their whole library organized and
searchable within seconds, builds two profiles, gets warned about a real conflict,
switches profiles, and launches — with their original files untouched and every
action reversible.

## Post-MVP — shipped (2026-07-14, iteration pass)
- ✅ **Conflict false-positive refinement** — same-author + byte-identical-script
  clashes down-ranked (new "info" severity); critical 22→11 on the real library.
- ✅ **Mod detail drawer** — metadata, dependencies (in-library/missing), uniqueType,
  conflicts involving the mod, and all per-mod actions.
- ✅ **Per-mod notes + star rating**; ✅ **user-defined tags** (mod_tag table, chips,
  tag filter) — completes the filter bar.
- ✅ **Loadout export/import** (`.silo` files via dialog).
- ✅ **Library statistics dashboard** (totals, disk by category, largest mods).
- ✅ **Reveal-in-folder / open mods+game folders** (opener plugin).
- ✅ **Savegame backup** (safe copy).
- ✅ **Theme toggle** (system/light/dark); ✅ **diagnostics report export**.

## Post-MVP — still open (need the user / network / infra)

> _Status update (2026-10): the first three items below have since shipped — ModHub is
> indexed by the SiloAPI catalog (update checks + browsing; downloads stay on ModHub's
> site), `game.xml` graphics presets are editable with a `.bak` backup, and the app
> self-updates from signed GitHub releases. Kept as written for history._

- **ModHub integration** — update-checking + optional browsing/downloading. Fragile
  HTML scraping, ToS-gray; only if we deliberately take it on (not done autonomously).
- **System/graphics "optimization"** (game.xml tuning) — risky config edits; validate
  with the user before touching.
- **App self-update** — needs GitHub releases + signing.
- **Interactive dependency graph** — nice-to-have beyond the drawer's dep list.
- **Bulk curate/tag** the filtered set; toasts; keyboard shortcuts.
