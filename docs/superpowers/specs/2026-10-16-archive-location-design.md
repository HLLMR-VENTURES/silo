# Library archive location — design

- **Date:** 2026-10-16
- **Issue:** #166 (configurable library/archive location). Builds on mods-folder selection (#162, #176).
- **Status:** approved design, not yet implemented

## Problem

Silo's archive is always `<mods folder>/archive/<Category>/`, on the same drive as the mods folder. Users with a small SSD and a large HDD want the parked library on the HDD and only the active set on the SSD. The #166 reporter (CachyOS) symlinks the archive to another drive by hand today, and already keeps an "overflow" collection of mods there.

In code the assumption is concentrated: `organize.rs` builds every archive path from `root.join(ARCHIVE)` via `archive_dir`/`archive_path`, and `scan.rs` reads `root.join("archive")`. Moving files across drives is already handled (`move_file`: rename, else copy + swap), and projection already falls back from hardlink to copy.

## Goals

1. Let the user put a mods folder's archive anywhere, on any drive.
2. Move an existing archive to the new place safely: verified, resumable, and pausable so the game can be played mid-move.
3. Never move mods while Farming Simulator 25 is running.
4. Adopt a folder that already contains mods (the reporter's HDD collection) into the library.
5. Avoid duplicating active mods across drives where the OS allows links.

## Non-goals

- One archive shared by several mods folders (organize state is per folder since #176).
- Moving `backups/` (adopt/restore copies) — stays in the mods folder; rare and small.
- Junctions/symlinks for the whole archive folder (rejected approach B: cloud-sync and backup tools follow them — on the owner's machine Proton Drive would start syncing the HDD archive — and Restore vanilla would need special cases).

## Decisions taken

| Question | Decision |
|---|---|
| Existing archive when the location changes | Move it: background job with progress, verify-then-delete per mod, resumable. |
| Projection when the archive is on another drive | Hardlink (same drive) → symlink (Linux/macOS always; Windows only when permitted, i.e. Developer Mode) → copy. |
| Chosen folder already holds mods | Adopt them through the Organize preview: filed into `<archive>/<Category>/` on that drive (renames), parked; techName duplicates left untouched. |
| Structure | Approach A: an explicit `Layout { mods, archive }` threaded through `organize` and `scan`. |
| Game running | Refuse to start or resume a move while FS25 runs; auto-pause if it starts mid-move. Pausing is non-destructive and the game can be launched while paused. |

## Backend

### `organize::Layout`

```rust
pub struct Layout {
    pub mods: PathBuf,     // the flat folder the game reads
    pub archive: PathBuf,  // where parked mods live, <archive>/<Category>/<file>
}
impl Layout {
    pub fn default_for(mods: &Path) -> Layout; // archive = mods/archive
}
```

- Every public organize function takes `layout: &Layout` instead of `root: &Path`. `archive_dir` / `archive_path` read `layout.archive`.
- DB keys stay derived from `layout.mods` (`modsroot::canonical_key`), so #176's per-folder state is unchanged.
- `scan` takes the archive path(s) per mods root instead of `root.join("archive")`; during a paused move it reads the new archive, then the old one.
- The flat-root scan keeps skipping a folder named `archive` (the default location) and `backups`.

### Settings

- `app_setting` `archive_root:<mods key>` — absolute path of a custom archive. Absent ⇒ default (`<mods>/archive`). Existing users see no change.
- `app_setting` `archive_move:<mods key>` — JSON `{ from, to, state: "running" | "paused" | "cancelling", errors: [techName…] }`, present only while a move is unfinished.

### Trusted write roots

`modsroot`'s published active root becomes the active **layout**. `allowed_roots()` returns both `layout.mods` and `layout.archive` (and, during a move, the other archive), so installs/updates into the archive pass `paths::ensure_write_under`. Rust remains the only authority on where Silo may write.

### Projection

`make_link` for zips: `hard_link` → `symlink` (when `supports_symlinks(layout)` says yes) → `copy`. `supports_symlinks` is a one-time probe (create a file symlink in the mods folder pointing into the archive, then remove it); on Linux/macOS it normally succeeds, on Windows only with Developer Mode. Folder mods keep the existing link-or-copy logic. Ownership checks already treat "link resolves to the archived file" as Silo's own; the size+mtime `projection` record keeps deactivation O(1).

"Optimize links" (`relink_projections`): when the archive is on another drive, upgrade copy projections to symlinks where permitted (today it only upgrades to hardlinks).

**Release gate:** confirm in-game that FS25 loads a symlinked `.zip` (Windows with Developer Mode). If it doesn't, ship with the symlink step disabled (copy only).

### Game-running detection

`gamelaunch::fs25_running() -> bool` using the `sysinfo` crate (new dependency). Pure, unit-tested matcher `is_fs25_process(name, cmdline)`, case-insensitive:

- Windows: `FarmingSimulator2025.exe` (launcher), `FarmingSimulator2025Game.exe` (game, under `x64\`), `dedicatedServer.exe` when its path is inside an FS25 install.
- Linux / Proton: the same `.exe` names in the process name or command line (Linux truncates process names to 15 chars, so match the command line too).
- macOS: names containing `FarmingSimulator2025` or `Farming Simulator 25`.

### The move job

`organize::relocate_archive(conn, layout, new_archive, control, progress) -> MoveReport`, run on a blocking worker; `control` carries pause/cancel requests; progress is emitted as `archive-move-progress { done, total, bytesDone, bytesTotal, current }`.

1. **Preflight** (`archive_move_preflight`, also used by the UI dialog): the new folder exists and is writable; it is not the current archive; it is not inside the flat mods root (other than the default `archive/`); it is not another known folder's archive; the target drive has free space ≥ archive size + 1 GB. Reports `{ modCount, bytes, freeBytes, sameDrive, symlinksAvailable, looseModsFound, problems[] }`. Any problem ⇒ refuse before writing anything.
2. **Game check:** refuse if `fs25_running()`.
3. **Marker:** write `archive_move:<key>` (`state: "running"`) before the first byte moves.
4. **Per organized mod** (idempotent):
   - only the old copy exists ⇒ copy to `<new>/<Category>/<file>.silo-move-tmp`, verify (size; SHA-256 for zips; recursive size+count for folders), rename into place, then delete the old copy;
   - both exist and the new one verifies ⇒ delete the old copy;
   - only the new one exists ⇒ already done.
   - An **active folder mod** has its link in the mods folder re-pointed to the new location as part of its step. An active zip hardlinked to the old archive keeps its data after the old copy is deleted (it becomes a standalone copy; its `projection` record still matches).
   - Before each mod: if `fs25_running()` ⇒ pause (reason: game started). If pause/cancel requested ⇒ abandon the in-flight copy (delete its temp file; the old copy is untouched) and stop at this boundary.
5. **Finish:** only when every mod is in the new archive — set `archive_root:<key>` (or remove it when the new location is the default), clear the marker, remove now-empty old category folders (never anything else), rescan.

**Safe points:** after every mod the mods folder is complete and valid (moved mods re-linked, unmoved ones untouched), so pausing — and launching FS25 while paused — is safe.

**Paused state:** the marker (`state: "paused"`) survives restarts. The library shows mods from both archives. Organize, activate, park, install, loadout apply, Restore vanilla, relink and folder switching are disabled; Launch is allowed. **Resume** re-runs the game check and continues (finished mods are skipped).

**Cancel move:** sets `state: "cancelling"` and runs the same idempotent step in reverse (new → old), then clears the marker. The library always ends up in exactly one archive.

**Per-mod failure** (read error, verify mismatch, disk full): leave the original in place, record the techName in `errors`, continue with the rest, then stop as **paused with errors** (Retry / Cancel move). The setting never switches while any mod remains behind.

**Leftovers:** on resume, delete stale `*.silo-move-tmp` files in the new archive.

### Adopting an existing collection

If the chosen folder has loose `.zip` files or mod folders at its top level, after the move (or right away when there is nothing to move) Silo opens the Organize preview with **source = the archive folder**: they are filed into `<archive>/<Category>/` (same-drive renames) as parked mods. A techName already in the library is left untouched and listed as "already in your library". `plan_organize` / `apply_organize` gain a `source: &Path` parameter (today they always read the flat mods root).

### Commands

- `archive_status()` — `{ path, isDefault, available, move: null | { from, to, state, done, total, errors } }` for the active mods folder.
- `archive_move_preflight(path)` — see above.
- `archive_move_start(path)`, `archive_move_pause()`, `archive_move_resume()`, `archive_move_cancel()`.
- `archive_use_default()` — starts a move back to `<mods>/archive`.
- All `async` + `spawn_blocking`.

## Frontend

### Settings → Library archive

Under "Mods folder": the path, labeled *Inside your mods folder (default)* or *Custom location*, with **Open ↗**, **Change…** and **Use default** (custom only).

**Change…** → folder picker → preflight → `confirmDialog` (#177):
*"Move your library (736 mods, 11.2 GB) to D:\FS25 Archive? Needs 11.2 GB free on D: (840 GB free). Active mods stay playable — Silo links them into your mods folder (or copies them if links aren't available)."* — **Move library** / **Cancel**. On Windows without symlink permission and a different drive, add: *"Turn on Windows Developer Mode to link active mods instead of copying them."* Preflight problems are shown instead of the dialog.

### Banners

- Moving: *"Moving your library to D:\… — 212/736 mods · 3.1/11.2 GB"* — **Pause** · **Cancel move**.
- Paused: *"Archive move paused — 412 of 736 moved."* (+ *"Farming Simulator 25 started."* when that was the reason) — **Resume** · **Cancel move**.
- Paused with errors: *"3 mods couldn't be moved"* (expandable list) — **Retry** · **Cancel move**.
- Custom archive unavailable (no move): *"Your library archive D:\… isn't available."* — parked mods hidden, organize actions disabled; if active mods are symlinked, add *"Active mods linked from it won't load in-game until the drive is back."* — **Retry** · **Choose another**.

### Disabled while a move is running or paused

Organize, activate/park toggles, loadout apply, install/update, Restore vanilla, Optimize links, mods-folder switching — tooltip *"Finish or cancel the archive move first."* Launch is disabled while running and enabled while paused.

## Error handling summary

- Preflight failure ⇒ nothing written.
- Game running ⇒ refuse start/resume; auto-pause mid-move.
- Per-mod failure ⇒ original kept, move ends paused-with-errors.
- New archive's drive gone while paused ⇒ banner; nothing deleted; Resume/Cancel wait for the drive.
- Old archive's drive gone ⇒ the unmoved mods are unavailable; pause with a message.
- Crash ⇒ the marker survives; temp files cleaned on resume; idempotent steps make resume correct.

## Testing

Rust unit tests (temp dirs, no app):

- `is_fs25_process` over Windows / Proton (truncated name + command line) / macOS samples, plus non-matches.
- `Layout::default_for`; organize functions honor a custom archive.
- Per-mod relocation states: old only, both (new verified), both (new corrupt ⇒ recopy), new only.
- Interrupted move: stop after N mods, resume ⇒ same final tree as an uninterrupted move.
- Pause safe point: an active folder mod is re-linked to the new archive; an active hardlinked zip is still readable after its old copy is deleted.
- Cancel ⇒ the original tree is restored exactly (paths + sizes).
- Per-mod failure (unreadable source) ⇒ original kept, error recorded, setting not switched.
- Preflight free-space check with injected sizes; refuses the current archive and paths inside the mods root.
- Symlink projection is recognized as Silo's own; a foreign file at the same name is not.
- Scan reads two archives during a paused move without duplicating mods.
- Adopt: loose mods filed into categories; techName duplicates untouched.

Manual (RC rules — never on the real library):

- Use #176's folder switcher on a **disposable mods folder** with ~20 copied mods. Move its archive to another drive; forensic before/after diff (`RC_TESTING.md`) — every change explained.
- Pause mid-move; launch FS25 from Silo; mods load. With FS25 running, Resume is refused. Close FS25, resume, finish.
- Start a move, then cancel ⇒ tree restored.
- **Release gate:** FS25 loads a symlinked `.zip` (Windows, Developer Mode on).
- Linux: ask the #166 reporter to verify on CachyOS.

## Per-change validation (Silo `CLAUDE.md`)

1. **Reversible:** every move step is verify-then-delete; Cancel move reverses it; "Use default" moves back.
2. **Off-thread:** the move runs on a blocking worker with progress events; all commands are `async` + `spawn_blocking`.
3. **Tested against:** a disposable mods folder (never the real library) for destructive steps.
4. **XML:** not involved.
