# Silo — Cross-platform support (Windows / macOS / Linux)

Silo targets all three desktop OSes. Tauri builds native binaries for each; the
hard part isn't the runtime, it's **finding the game's files** and **projecting the
active set** correctly per platform. Items marked (?) need on-device verification
before we hardcode them.

## Where FS25 lives, per platform

FS25 officially ships on **Windows** and **macOS** (GIANTS). There is **no native
Linux build** — Linux players run the Windows build via **Steam Proton**, so on
Linux the game's files live inside a Proton prefix. Silo itself runs natively on
Linux and reaches into that prefix.

### User data dir (mods, savegames, gameSettings.xml)
| OS | Path |
|----|------|
| Windows | `%USERPROFILE%\Documents\My Games\FarmingSimulator2025\` |
| macOS — Mac App Store | `~/Library/Containers/com.giants-software.fs25/Data/Library/Application Support/FarmingSimulator2025/` (sandboxed; verified from game logs, #171) |
| macOS — Steam / GIANTS | `~/Library/Application Support/FarmingSimulator2025/` (Steam verified 2026-10-27 from a tester's `log.txt` "Mod Directory" line, Apple Silicon, macOS 26 Tahoe, #322; GIANTS-direct unconfirmed) |
| Linux/Proton | `<prefix>/drive_c/users/steamuser/Documents/My Games/FarmingSimulator2025/`, `<prefix>` = `<steam library>/steamapps/compatdata/2300320/pfx` |

`fsgame::locate()` tries every candidate: on Linux, each Steam library listed in
`libraryfolders.vdf` under the native (`~/.steam/steam`, `~/.local/share/Steam`) and Flatpak
(`~/.var/app/com.valvesoftware.Steam/.local/share/Steam`) roots; on macOS, the App Store container
first. Among existing candidates it prefers one with `log.txt` (the game ran there), then one with
`mods/`.

**Mods folder.** FS25's `gameSettings.xml` `<modsDirectoryOverride active="…" directory="…"/>` can
redirect the mods folder anywhere; Silo follows it (`modsroot`), lets the user choose another folder,
and can write the override back (with a `.bak`). FS25 writes the directory with forward slashes. On
Linux the value is a Wine path, translated through the prefix's `dosdevices/` drive links (`Z:` →
`/`).

### Game install dir (SDK schemas, pdlc)
| OS | Path (Steam) |
|----|------|
| Windows | `<lib>\steamapps\common\Farming Simulator 25\` |
| macOS (?) | `~/Library/Application Support/Steam/steamapps/common/Farming Simulator 25/` |
| Linux (?) | `~/.steam/steam/steamapps/common/Farming Simulator 25/` |

`<lib>` is any Steam **library root** — games can live on other drives.

On **macOS** the game is a `.app` bundle (a folder), not a bare binary. `gamelaunch.rs`
takes any `*.app` directly inside the Steam install folder (the exact bundle name is
unverified (?)), else an FS25-named bundle in `/Applications` or `~/Applications`
(Mac App Store / GIANTS-direct), else Spotlight (`mdfind` on bundle id
`com.giants-software.fs25`). Steam installs launch via `steam://rungameid/2300320`;
the others launch with `open -a <bundle>`.

When the game sits in a Steam library on a removable drive, macOS asks once whether Silo may
access that drive (seen on Tahoe, #322); after the user allows it, Silo works normally. Silo
should handle a refusal gracefully, the same as any unreadable folder.

## Store / launcher discovery (don't hardcode drives)

FS25 is sold via **Steam**, **Epic**, and the **GIANTS** launcher. Detect, don't guess:

- **Steam:** parse `steamapps/libraryfolders.vdf` to enumerate all library roots,
  then look for the FS25 app (Steam AppID **2300320** (?) — verify) under each
  `common/`. libraryfolders.vdf locations:
  - Windows: `C:\Program Files (x86)\Steam\steamapps\libraryfolders.vdf`
  - macOS: `~/Library/Application Support/Steam/steamapps/libraryfolders.vdf`
  - Linux: `~/.steam/steam/steamapps/libraryfolders.vdf` (or `~/.local/share/Steam/…`)
- **Epic (?):** `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests\*.item` (JSON)
  lists install locations.
- **GIANTS launcher (?):** its own install path + registry entry on Windows.
- **Always allow manual override** — the user points Silo at the folder; detection
  is a convenience, never a hard dependency. `gameSettings.xml`'s
  `<modsDirectoryOverride>` can also redirect the mods folder anywhere.

## Projection strategy per OS (the delicate part)

Goal: make the game's flat `mods/` folder show exactly the active profile, without
mutating library originals, fully reversibly. Capability differs by OS:

| OS | Dir link | File link (.zip mods) | Notes |
|----|----------|----------------------|-------|
| Windows | **junction** — no admin needed | **symlink** — needs Developer Mode or admin | hardlink works same-volume only; **copy** is the universal fallback |
| macOS | symlink — no privilege | symlink — no privilege | straightforward |
| Linux | symlink — no privilege | symlink — no privilege | if projecting into a Proton prefix, keep paths inside the prefix drive |

**Rules:**
1. Probe link capability per game-root **once** (create+delete a test link). Record
   the mode; surface it to the user ("Linked" vs "Copied").
2. `.zip` mods are the risky case on Windows (file symlink). If file symlinks
   aren't permitted, fall back to **copy** for zips (junctions still handle unpacked
   dirs). Copy-mode must be first-class, not a footnote.
3. Track every link/copy Silo creates (tag/manifest) so cleanup only ever removes
   **Silo-owned** entries — never a user's real file.
4. Cross-volume: hardlinks and some symlinks fail across drives → copy-mode.

## Library archive (any drive)

The archive — `<archive>/<Category>/<file>`, where parked mods live — defaults to
`<mods folder>/archive` but can be moved to any drive (#166), **per mods folder** (setting
`archive_root:<mods key>`; absent means the default). The mods folder itself never moves: it stays
where FS25 reads it, so only the *active* set has to cross drives.

**Projection of an active mod** (`organize::make_link` + `LinkPolicy`), tried in order:
1. **Hardlink** — same volume as the archive. No privilege, no duplicated disk space.
2. **Symlink** — only when the archive is on a **different volume**, the mods folder is **not
   inside a Mac App Store container**, and a one-time probe succeeds (it creates a file symlink in
   the mods folder pointing into the archive, then removes it). The probe runs lazily — only after a
   hardlink actually failed — so the common same-volume case never probes.
3. **Copy** — everything else: Windows without Developer Mode (file symlinks need it), a filesystem
   that refuses links, a cloud-synced folder that rejects hardlinks.

Folder (unzipped) mods keep the symlink-else-copy logic, but inside a **Mac App Store container**
they are always copied: the sandboxed game can't follow a link out of its container (matched on
`/library/containers/` in the mods path, case-insensitively).

**A drive Silo can't identify** (a network share, a mount the disk list doesn't show) is
**probed** for linking instead of guessed: `organize::same_volume_for_links` uses the disk list
when it knows both mounts, and otherwise `hardlinks_between` (a temp file in the archive,
hardlinked into the mods folder, both removed again). Where the probe link works it's the same
volume; where it fails it's another one, so Reclaim disk space doesn't try a hardlink per mod and a
move keeps an active mod's hardlink rather than copying it onto the mods drive. Where no probe
file can be made at all (a missing or read-only folder) the answer is inconclusive and counts as
the same volume: hardlink first, which falls back to a copy. Probe names
(`.silo-hardlink-probe-*`, `.silo-symlink-probe-*`) are unique per process and attempt, skip a
name a stale probe holds, are ignored by scans, and stale ones (over ten minutes old) are swept
from the mods folder and archive under the library lock. `LinkPolicy`,
Reclaim disk space (Settings → Maintenance; `relink_mode`) and the move preflight's `sameDrive` all decide this way. Only the
move's **free-space check** keeps "unknown = another drive" (`known_same_mount`), so the whole
archive must fit there.

When the archive is on another drive and links aren't available, **Reclaim disk space** reports it
(`RelinkReport.cross_drive_copies`) instead of doing anything. Whether FS25 actually loads symlinked
zips on each OS is a release gate — see `SMOKE_TEST.md` §1c.

**Is FS25 running?** An archive move refuses to start or resume — and pauses itself — while the game
is up. `gamelaunch::fs25_running()` uses the `sysinfo` crate: a name-only process snapshot, then
(only for plausible names) each process's command line and exe path, matched by
`is_fs25_process`: `FarmingSimulator2025.exe` (launcher), `FarmingSimulator2025Game.exe` (game),
`dedicatedServer.exe` when it belongs to an FS25 install, a Wine/Proton host whose command line names
`FarmingSimulator2025Game`, and any process with `FarmingSimulator2025` / `Farming Simulator 25` in
its name (the macOS bundle). Linux truncates process names to 15 characters, so those fall back to
the command line and exe path. Free space and same-drive detection use `sysinfo`'s disk list too.

## Path / filesystem hygiene (all platforms)

- **Use Rust `PathBuf`** everywhere; never string-concatenate separators.
- **Linux is case-sensitive.** Mod tech names and `modDesc.xml` internal paths must
  be matched with real case; don't lowercase paths for comparison (lowercase only a
  *copy* for search/index keys).
- Normalize the digit-prefix rule and reserved-name checks per OS.
- Long paths on Windows (>260) — use extended-length prefixes / `dunce` if needed.

## Silo's own app-data location (via Tauri path API)

Tauri names these folders after the bundle identifier, `com.hllmr.silo`.

| OS | Data (`silo.db`, `db-snapshots/`) | Cache (icons) | Log (`silo.log`) |
|----|------|------|------|
| Windows | `%APPDATA%\com.hllmr.silo\` | `%LOCALAPPDATA%\com.hllmr.silo\` | `%LOCALAPPDATA%\com.hllmr.silo\logs\` |
| macOS | `~/Library/Application Support/com.hllmr.silo/` | `~/Library/Caches/com.hllmr.silo/` | `~/Library/Logs/com.hllmr.silo/` |
| Linux | `~/.local/share/com.hllmr.silo/` | `~/.cache/com.hllmr.silo/` | `~/.local/share/com.hllmr.silo/logs/` |

Never write Silo's DB/cache into the game folder. The game folder only ever
receives projected links/copies.

## Build & CI

- Tauri v2 cross-compiles per target: `.msi`/`.exe` (Win), `.dmg`/`.app` (mac,
  signed+notarized eventually), `.AppImage`/`.deb`/`.rpm` (Linux).
- GitHub Actions: CI runs on `ubuntu-latest`, `macos-latest` and `windows-latest`; the
  release matrix builds on `windows-latest`, `macos-latest` (arm64 + x64) and `ubuntu-22.04`.

## Verification checklist (do on-device before shipping detection)
- [ ] Confirm FS25 Steam AppID and macOS user-dir path on a Mac.
- [ ] Confirm Proton prefix layout + compatdata AppID on a Linux box.
- [ ] Confirm Epic + GIANTS-launcher install manifests.
- [ ] Test file-symlink projection on Windows without Developer Mode (expect
      fallback to copy).
