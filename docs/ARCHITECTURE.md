# Silo — Architecture

## Principles

1. **The UI thread never does heavy work.** Archive parsing, hashing, image
   decode, and tree walks live in Rust on a worker pool.
2. **Library originals are sacred and untouched.** The game folder is a
   *projection* of a chosen profile, always reversible.
3. **Parse, don't regex.** XML via `quick-xml`.
4. **Cache honestly.** `path + mtime + size` invalidation; never nuke the whole
   cache on a version bump.

## High-level shape (Tauri v2)

```
┌─────────────────────────────────────────────┐
│  Frontend  (src/)  — web, virtualized UI     │
│  • library view, profiles, conflict panel    │
│  • talks to core via invoke + event streams  │
└───────────────▲───────────────┬──────────────┘
                │ events         │ invoke commands
                │ (progress,     │ (scan, build_profile,
                │  scan results) │  project, detect_conflicts…)
┌───────────────┴───────────────▼──────────────┐
│  Rust core  (src-tauri/)                      │
│                                               │
│  commands/        thin Tauri wrappers         │
│  scan/            walk + parse + hash (rayon)  │
│  moddesc/         quick-xml modDesc.xml model  │
│  library/         source library + tags/folders│
│  profile/         loadout model                │
│  projection/      symlink/junction/copy engine │
│  conflict/        static conflict analysis     │
│  savegame/        careerSavegame.xml reader     │
│  settings/        XML → form-schema ↔ XML writer │
│  db/              rusqlite, migrations, cache   │
│  fsgame/          path discovery, launch        │
└───────────────────────────────────────────────┘
```

Keep `scan`, `moddesc`, `library`, `profile`, `projection`, `conflict`, and
`settings` as **pure-ish logic modules** with their own unit tests, behind thin
`commands/` wrappers. They should be testable against a fixture folder without a
running app.

> **The shipped module set** has grown well beyond the original sketch above — the same
> pure-logic-behind-thin-command pattern held (230+ Rust unit tests). The sketch's
> `library`/`profile`/`projection` became `organize` + `db`, and the command wrappers live
> in `lib.rs`. Feature-level release gate: `docs/SMOKE_TEST.md`.

### Shipped modules (`src-tauri/src/`)

| Area | Modules |
|------|---------|
| Scan & parse | `scan` (walk + parse + cache), `moddesc` (modDesc.xml model), `category` (two-level categorization), `store` (storeItem category), `icons` (DDS/PNG thumbnails) |
| Game & folders | `fsgame` (per-OS user-dir discovery: Steam libraries, Proton prefix, Mac App Store), `modsroot` (which mods folder is live: FS25's `modsDirectoryOverride`, the user's choice, per-folder state — #162), `gamelaunch` (launch, `fs25_running`) |
| Library on disk | `organize` (archive layout, projection — hardlink/symlink/junction/copy — and Restore vanilla), `relocate` (moving the library archive to another folder/drive as a resumable, verified job — #166, landed in #185) |
| Analysis | `conflicts`, `logscan` (crash triage), `bisect` (guided bisection), `bindings` (input map) |
| Saves & config | `savegame` (saves + bound mod lists), `xmlconfig` (game.xml / careerSavegame.xml fields), `settings_form` (mod settings XML → form ↔ XML), `xmltext` |
| Network & sources | `siloapi` (catalog client), `github`, `collection` (shareable mod-set lists), `net` (outbound-URL guard) |
| Sharing & generators | `mpsync` (multiplayer manifest), `bridge` (filltype companion mod), `provenance` (integrity hashing) |
| Persistence & safety | `db` (SQLite), `paths` (write confinement + `.bak` writes), `secrets` (OS keychain) |

## Key crates (proposed — confirm at scaffold time)

| Concern | Crate |
|---------|-------|
| Archive read | `zip` (read central directory without full extraction) |
| XML | `quick-xml` |
| Hashing | `blake3` (fast) or `md5` if we must match GIANTS `fileHash` |
| Parallelism | `rayon` for the scan fan-out; `tokio` for async IO if needed |
| Image decode | `image` + a DDS decoder crate (e.g. `ddsfile` / `image-dds`) |
| DB | `rusqlite` (bundled SQLite) with a small migration runner |
| Tree walk | `walkdir` |
| Windows links | `std::os::windows::fs` (symlink_dir/junction), `std::fs::hard_link` |

> Note on hashing: FS/GIANTS uses an MD5 `fileHash` in some places. For Silo's
> *own* change-detection, prefer `blake3`. Only compute MD5 if/when we need to
> match a GIANTS-visible value — decide per feature, don't default to MD5.

## Data model (SQLite — first pass)

Real tables with real indexes:

- `mod_file(path PK, root, mtime, size, hash, kind)` — physical presence + cache key.
- `mod_meta(hash PK, mod_id, title, author, version, icon_ref, raw_moddesc_json)` —
  parsed metadata keyed by content hash so identical mods dedupe.
- `mod_dependency(hash, dep_ref)` / `mod_script(hash, script_basename)` /
  `mod_specialization(hash, spec_name)` — normalized, indexed for conflict queries.
- `folder(id, name, parent_id)` + `mod_tag(hash, tag)` — organization.
- `profile(id, name)` + `profile_mod(profile_id, hash, order_index)` — loadouts.
- `curation(hash, state, note)` — favorite/hidden/broken.
- `mod_registration(hash, kind, name)` — the six namespace surfaces
  (specialization / placeableSpecialization / handToolSpecialization / vehicleType /
  placeableType / handToolType) + actions/brands/storeCategories, normalized so a
  conflict is a single indexed `GROUP BY name HAVING count>1` over the active set.
- `mod_unique_type(hash, unique_type)` — GIANTS' `<uniqueType>` conflict primitive.
- Index every column we filter/join on (`hash`, `name`, `script_basename`,
  `unique_type`, `mod_name`, and a collated/generated lowercased title column — not
  runtime `LOWER()`).

**Conflict engine** consumes `mod_registration` + `mod_unique_type` +
`mod_script` for the active loadout: duplicate registration `name`, shared
`unique_type`, and overlapping script basenames each become a conflict with
severity and the exact two mods + element cited. See
`reference/fs25-modding-notes.md` for the full surface list and severities.

**Savegame model:** `careerSavegame.xml` `<mod modName version required fileHash>`.
A loadout generated from a save must include every `required="true"` mod; `fileHash`
(MD5) verifies the library copy matches what the save was built on. This is the one
place we compute **MD5** (to match GIANTS' value) rather than blake3.

Decoded icons cached as PNG files under the app data dir (referenced by
`icon_ref`), **not** as base64 blobs in SQLite rows (it bloats rows and the IPC
payload).

## The projection engine (the delicate part)

At launch, project the active profile into the primary game `mods/` folder:

1. Determine link capability once: try creating a test symlink; if it fails, mark
   the root **copy-mode**. Dir junctions generally work without Dev Mode; file
   symlinks (for `.zip`s) may not — so `.zip` mods are the risky case.
2. For each active mod, ensure a link/copy in the game folder pointing at the
   library original; verify existing links point at the right target.
3. Remove stale projections that Silo created and are no longer active. **Only
   ever remove links/copies Silo owns** — track them; never delete user files we
   didn't create. (Tag Silo-created entries so cleanup is unambiguous.)
4. Everything reversible: "clear projection" restores the folder to empty/known
   state without touching the library.

Cross-volume note: hardlinks fail across drives and symlinks-to-another-drive may
be blocked — copy-mode is the universal fallback and must be first-class.

**As shipped:** the archive lives at `<archive>/<Category>/<file>`, by default
`<mods folder>/archive`. Zips project as **hardlinks** (same volume), folder mods as
junctions/symlinks, and anything that can't be linked (e.g. a cloud-synced folder) as a
marked **copy**. Every projection is recorded so cleanup only touches Silo-owned entries.

### Mods folder and library archive location

- **Which mods folder** (`modsroot`, #162): FS25's `gameSettings.xml`
  `<modsDirectoryOverride>` is followed; the user can choose another folder and optionally
  write the override back (with a `.bak`). Organize state is keyed per folder, and
  `allowed_roots()` follows the active folder so every write command does too.
- **Where the archive lives** (`relocate`, #166 — landed in #185, unreleased as of v0.8.0):
  per mods folder, the archive can be moved to any drive while the mods folder stays put.
  The move is a resumable job with a DB marker — copy to a temp name, verify, place without
  replacing an existing file, then remove the original — with Pause / Resume / Cancel
  (cancel moves back what was moved), refused and auto-paused while FS25 runs. Across
  drives, active zips project **hardlink → symlink → copy** (`organize::LinkPolicy`). Details
  and per-OS caveats: `CROSS-PLATFORM.md` ("Library archive").

## Settings form generator

The flagship tinkerer feature: turn a mod's hand-edited settings XML into a clean,
editable form, and write changes back safely. Lives in the `settings_form` Rust module
(sketched below as `settings/`) plus a generic Svelte form renderer.

**Where a mod's settings live** (probe in order, all present-or-absent):
- `Documents/My Games/FarmingSimulator2025/modSettings/<modName>/…xml` — the common
  location the game persists per-mod user settings to.
- A config XML shipped inside the mod archive (referenced from `modDesc.xml`, e.g.
  `<map configFilename>` or a mod-specific `*Config.xml` / `settings.xml`).
- Savegame-scoped mod settings under a `savegameN/` folder (some mods store state
  per save).
Silo shows the tinkerer which file(s) it found and which it's editing.

**Pipeline (`settings/`):**
1. **Parse** the XML with `quick-xml` into a lossless tree (preserve element order,
   attributes, comments, CDATA, and whitespace so the rewrite is a minimal diff).
2. **Infer a field schema** for each leaf value/attribute:
   - `true|false` → **toggle**; integer → **number/stepper**; float → **number** (or
     **slider** when a plausible range is known); constrained set → **select**;
     otherwise **text**. Vectors (`g_vector_*`) → grouped numeric inputs.
   - **Use the XSD when one exists** (`shared/xml/schema/*.xsd`) to get real types,
     enums, defaults, ranges, and the `typeStr`/documentation annotations for
     labels/help — far better than value-only inference. Fall back to inference when
     no schema covers the file (most mod-specific configs).
   - Derive a human label from the tag/attr name (de-camelCase) + XSD docs; keep the
     raw XPath as the stable key.
3. **Render** via one generic, stylized Svelte form component driven by the schema
   (grouped by element, collapsible sections, search) — no per-mod code.
4. **Write back**: apply edits onto the preserved tree and re-serialize, changing
   **only** touched values. Always **snapshot/back up the original** first
   (reuses the projection engine's reversibility guarantee); validate against the
   XSD before save when available; offer "reset to default."

**Safety & scope:** never invent elements the schema/mod didn't have; round-trip
untouched files to a byte-identical (or minimal-diff) result in tests; treat unknown
/ freeform XML as read-only-with-raw-editor rather than guessing. A "raw XML" escape
hatch is always available beside the generated form.

## Frontend

- **Svelte 5 (runes) + Vite + TypeScript** (committed — see `DECISIONS.md`).
  Fine-grained reactivity means no whole-store re-render storms by construction; the
  small runtime fits "native, fast, tiny." Design language in `DESIGN.md`.
- **Virtualized lists mandatory** — the library view must render 700+ rows without
  materializing 700 DOM subtrees.
- Derive, don't broadcast: `$derived` for computed views; precompute lookups (status
  by tech-name as a `Map`) once, not per row. No global store that every card
  subscribes to wholesale.
- Progress via Tauri events, not polling. Heavy work is always a Rust command.
- Styling: hand-authored CSS design tokens (`tokens.css`), in-house primitives — no
  heavyweight UI kit.

## Explicitly rejected

- Synchronous archive/image work anywhere near the UI thread.
- Regex XML parsing.
- Base64 icons in DB rows / whole-list JSON round-trips over IPC.
- Build-time obfuscation (`controlFlowFlattening`, `deadCodeInjection`).
- Scraping GIANTS' site with bot-evasion as the data backbone.
- Wiping the cache on version migrations.
