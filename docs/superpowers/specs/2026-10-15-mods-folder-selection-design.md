# Mods folder selection — design

- **Date:** 2026-10-15
- **Issues:** #162 (mod folder other than the default); the macOS discovery part of #171. Foundation for #166 (configurable archive location), which gets its own spec.
- **Status:** approved design, not yet implemented

## Problem

Silo only ever manages `<FS25 user dir>/mods`:

- `fsgame::default_mods_paths()` is the sole source of the mods root; the frontend calls it once at startup (`App.svelte`, `roots = await defaultModsPaths()`). There is no way to choose another folder.
- FS25's own redirect — `<modsDirectoryOverride active="…" directory="…"/>` in `gameSettings.xml` — is ignored. A user who has it on sees Silo manage a folder the game never reads.
- On Linux, `fsgame::user_dir()` returns `None`, so Silo finds **no** mods folder at all. Linux support was documented (`docs/CROSS-PLATFORM.md`) but never implemented.
- Silo's organize records — the `organized` and `projection` tables — are global, not per folder. Pointing Silo at a second folder today would apply folder A's records to folder B.

## Goals

1. Silo follows the folder the game actually reads (FS25's override when active, else the default).
2. The user can choose any folder; when it differs from what the game reads, Silo offers to point the game at it too.
3. Each folder keeps its own organize state; switching never disturbs the folder left behind, and switching back restores it.
4. Linux/Proton: auto-detect the FS25 user dir inside the Proton prefix and translate paths between the game's Windows view and the host. macOS: also find the sandboxed Mac App Store data folder (#171).

## Non-goals

- Moving the archive out of the mods folder (#166, spec 2).
- Moving or copying a library between folders.
- Managing several folders *simultaneously* (one live folder at a time).
- Detecting whether FS25 is running.

## Decisions taken

| Question | Decision |
|---|---|
| Split #162 / #166 | Two specs; this one first. |
| Game setting on a manual pick | Offer to update FS25's `modsDirectoryOverride` (with `.bak`); declining leaves a persistent mismatch warning. |
| Switching an organized folder | Leave it intact; organize state is remembered per folder. |
| Linux scope | Full Proton-prefix detection + two-way path translation; manual picker as fallback. |
| State structure | One DB; `root` column on `organized` and `projection` (rejected: a DB per folder — fragments shared curation/tags/loadouts; a manifest file inside each folder — writable state in cloud-synced game folders + a consistency problem; revisit for #166). |

## Backend

### `fsgame` — Linux and macOS discovery

`user_dir()` gains a Linux branch returning the FS25 user dir and the Proton prefix it lives in:

1. Steam roots, in order: `~/.steam/steam`, `~/.local/share/Steam`, `~/.var/app/com.valvesoftware.Steam/.local/share/Steam` (Flatpak).
2. For each root, parse `steamapps/libraryfolders.vdf` to list every library folder (the root itself included).
3. In each library, check `steamapps/compatdata/2300320/pfx/drive_c/users/steamuser/Documents/My Games/FarmingSimulator2025`. First existing match wins.

macOS checks two candidates (#171). The Mac App Store build is sandboxed, and its own `log.txt` shows its data under the app container:

1. `~/Library/Containers/com.giants-software.fs25/Data/Library/Application Support/FarmingSimulator2025` (Mac App Store; verified from game logs in GIANTS forum threads t=211655 and t=212401).
2. `~/Library/Application Support/FarmingSimulator2025` (Steam / GIANTS direct; community-consistent, still to confirm on a real Mac).

On any platform, when several candidates exist, prefer the one containing `log.txt` (the game has actually run there), then the one containing `mods/`, then the first in order. A user dir whose `mods/` folder doesn't exist yet is still valid: Silo offers to create `mods/` rather than reporting no folder (Mac users often have no `mods/` until their first download).

New return shape (all platforms): `GameLocation { user_dir: PathBuf, prefix: Option<PathBuf> }`. Windows keeps its current path; Windows and macOS have `prefix: None`. Candidate selection is a pure function over "which of these paths exist" so it unit-tests without real installs. The VDF parse is a small pure function (the format is nested `"key" "value"` / `"key" { … }`); only `path` values are needed.

### New module `modsroot` (pure logic, unit-tested)

- **`read_override(xml: &str) -> Option<Override>`** — `Override { active: bool, directory: String }`, read via `xmlconfig::get_values` (`gameSettings.modsDirectoryOverride@active`, `@directory`). Missing element ⇒ `None`.
- **`PathMap`** — translates between the game's view of a path and the host path.
  - Windows / macOS: identity.
  - Linux: built from the prefix's `dosdevices/` entries (`c:` → `../drive_c`, `z:` → `/`, any other letter → its link target).
  - `to_host("Z:\\mnt\\hdd\\mods") -> Some("/mnt/hdd/mods")`; unknown drive letter ⇒ `None` (reported as unresolvable — never guessed).
  - `to_game(host) -> String` — longest matching drive mapping wins; no match ⇒ `Z:` + the host path (Wine always maps `Z:` to `/`).
  - Separators normalized; the game's value is written with forward slashes, as FS25 itself writes it (`directory="C:/Temp"`).
- **`resolve(choice: Option<PathBuf>, ov: Option<Override>, default: Option<PathBuf>, map: &PathMap) -> Resolved`**
  - `game_reads` = `map.to_host(ov.directory)` if `ov.active`, else `default`.
  - `path` = `choice` if set, else `game_reads`.
  - `source` = `User` | `GameOverride` | `Default`.
  - `mismatch` = `choice` set and not the same folder as `game_reads`.
- **`write_override(xml: &str, game_path: &str) -> Result<String, String>`** — sets `@active="true"` and `@directory` via `xmlconfig::set_values`. The command layer writes `gameSettings.xml.bak` first, then the new file. Never creates `gameSettings.xml` if it is missing.
- **Folder identity** — `canonical_key(path) -> String`: canonicalized (on Windows without the `\\?\` prefix), separators normalized, compared case-insensitively on Windows and case-sensitively elsewhere. Used for the `root` column and every "same folder?" comparison.

### Settings storage

`app_setting` keys:

- `mods_root_choice` — the user's chosen folder (absent ⇒ follow the game).
- `mods_root_known` — JSON array of folders Silo has managed, for the switcher (also derivable from `DISTINCT root` in `organized`; the setting keeps empty folders the user picked).

### Commands

- **`mods_root_status() -> ModsRootStatus`** — `{ path, source, gameReads, mismatch, available, modsFolderMissing, known: [{ path, modCount, current }], canWriteGameSetting, gameSettingProblem?, unresolvedOverride? }`. Replaces the frontend's `defaultModsPaths()` call. `async` + `spawn_blocking` (it reads files and probes folders).
- **`set_mods_root(path)`** — validates (exists, writable), stores the choice, adds it to `known`, returns the new status. It never writes the game setting; that's its own command, so the UI can switch first and then ask.
- **`point_game_at_mods_root()`** — writes FS25's override to the current folder (`.bak` kept), returns the new status.
- **`clear_mods_root()`** — removes the choice (back to following the game), returns the new status.
- **`create_mods_folder()`** — creates `<user dir>/mods` when FS25 hasn't yet; **`folder_looks_empty(path)`** — backs the "no mods yet" confirmation.

`default_mods_paths` stays for compatibility but is no longer called by the app.

### Database migration

`organized` and `projection` gain `root TEXT NOT NULL`; their primary keys become `(root, tech_name)` and `(root, file_name)`. SQLite cannot alter a primary key in place, so the migration rebuilds each table — create `*_new`, copy, drop, rename — inside one transaction.

- Existing rows are stamped with the folder resolved on the first launch after upgrade. Before this release Silo only ever managed the default folder, so stamping uses the **default** folder, not a newly-followed override.
- Idempotent: skipped when the `root` column already exists.
- A failure rolls back to the old schema intact; the app reports the error rather than continuing on a half-migrated DB.
- Every query in `db.rs` / `organize.rs` touching these tables filters or writes by `root`. The functions already take `root: &Path`; they derive the key with `canonical_key`.

Shared, per-mod data stays global (keyed by techName): `curation`, `category_override`, `mod_tag`, `mod_repo`, `loadout`, `loadout_mod`, `mod_cache` (already keyed by full path).

## Frontend

### Settings → "Mods folder"

- The current path plus a source label: *Following FS25's setting* / *FS25 default* / *Chosen in Silo*.
- **Change…** — system folder picker (`@tauri-apps/plugin-dialog`, `directory: true`).
- **Use the game's folder** — shown only when a choice exists; calls `clear_mods_root`.
- **Other folders Silo knows** — each with its mod count; click to switch.

### Pick flow

1. Validate: must exist and be writable. If it has no `.zip` files or mod folders, confirm: *"This folder has no mods yet — use it anyway?"*
2. Switch to it, then, if it differs from `gameReads`, ask (via the awaited `confirmDialog`, see #177): **"Point Farming Simulator at this folder too?"** — *Update FS25's setting* / *Only in Silo*. The dialog's note says the change applies on the next game launch, so close FS25 first (the game rewrites `gameSettings.xml` on exit). If `canWriteGameSetting` is false, only *Only in Silo* is offered, with the reason.
3. Switch: clear library state, `runScan()` on the new path, re-run the cloud-sync / hardlink probe, reload savegames (they don't move, but they're cross-checked against the new library).

### Mismatch banner

Shown whenever `mismatch` is true, including when someone changes the folder in-game after picking one in Silo: *"FS25 is reading `<gameReads>` — mods you activate here won't load in-game."* Actions: **Point FS25 here** / **Switch Silo to the game's folder**.

### Switching is blocked

- During a guided bisection (its snapshot belongs to the current folder).
- While an organize, activate, park, relink, or install job is running.

The control is disabled with a tooltip giving the reason.

### First launch after upgrade

No visible change unless FS25's override is already active. In that case Silo now follows it and shows a one-time notice: *"Silo now uses the mods folder FS25 is set to: `<path>`."* (Records were stamped to the default folder, so that folder's organize state is intact and reachable from the switcher.)

## Error handling

- **Chosen folder unavailable at launch** (unplugged drive, deleted folder): no silent fallback. Silo shows *"Your mods folder `<path>` isn't available"* with **Retry** / **Choose another** / **Use the game's folder**, and scans nothing until resolved.
- **`gameSettings.xml` missing or unparsable:** treated as no override; never created or rewritten; *Point FS25 here* is disabled with a tooltip.
- **Override write fails:** Silo's choice is still saved; the mismatch banner shows with the error. `.bak` is written before the edit.
- **Untranslatable Linux override** (unknown drive letter): reported as unresolvable in `unresolvedOverride`; the manual picker still works.
- **Migration failure:** single transaction ⇒ old schema intact; surfaced as a startup error.
- **Cross-folder safety:** projection removal keeps its existing ownership proof (`is_silo_projection` / `matches_provenance`) **and** only consults records whose `root` matches the current folder, so folder A's records can never authorize a delete in folder B.

## Testing

Rust unit tests (pure logic, no running app):

- `libraryfolders.vdf` parsing from a real-shaped sample (several libraries).
- Proton prefix discovery in a fake directory tree (native and Flatpak roots; game in a secondary library).
- macOS candidate selection: App Store container vs Application Support, `log.txt` beats `mods/` beats order, a user dir without `mods/` still resolves.
- `PathMap`: `C:` ↔ `drive_c`, `Z:` ↔ `/`, a custom `d:` link, unknown letter ⇒ `None`, `to_game` with no mapping ⇒ `Z:` fallback, separator normalization.
- `resolve`: precedence (choice > active override > default), an inactive override ignored, `mismatch` true/false.
- `read_override` / `write_override` against a real-shaped `gameSettings.xml`; other content preserved; missing element.
- `canonical_key`: Windows case-insensitivity, `\\?\` stripping, trailing separators.
- Migration on a pre-upgrade DB fixture: rows stamped, composite keys enforced, second run is a no-op.
- Isolation: activate a set in folder A, switch to B, assert A's records are untouched and B's operations cannot remove A's projections.

Manual, before release (`docs/SMOKE_TEST.md` gets these items):

- Windows: set FS25's override to a scratch folder ⇒ Silo follows it with the one-time notice; pick another folder ⇒ *Update FS25's setting* writes `gameSettings.xml` + `.bak`; the game loads mods from it; switch back ⇒ the original folder's active set is intact.
- Unplug / rename the chosen folder ⇒ the unavailable prompt appears, nothing is scanned.
- Linux: ask the #166 reporter (CachyOS) to verify prefix detection and override round-tripping on a test build.

## Per-change validation (Silo `CLAUDE.md`)

1. **Reversible:** writing the game override keeps `gameSettings.xml.bak`; *Use the game's folder* / the switcher restore any previous choice; switching never moves or deletes files.
2. **Off-thread:** `mods_root_status` and `set_mods_root` are `async` + `spawn_blocking`.
3. **Tested against:** this machine's real `gameSettings.xml` (override present, `active="false"`) and the 729-mod library; Linux by the reporter.
4. **XML:** read and written via `xmlconfig` (quick-xml), not regex.
