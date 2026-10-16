# Mods Folder Selection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let Silo manage the mods folder FS25 actually reads (its `modsDirectoryOverride`, or the default), let the user pick any folder (optionally pointing the game at it), keep organize state per folder, and discover the FS25 user dir on Linux/Proton and the macOS App Store sandbox.

**Architecture:** A new pure-logic module `modsroot` resolves the live folder (user choice > active game override > default), translates Wine paths, and holds the single trusted "active root" that `allowed_roots()` already funnels every write through, so the 13 existing organize/install commands pick up the new folder with no signature change. `fsgame` gains candidate-based user-dir discovery. The `organized` and `projection` tables gain a `root` key (table-rebuild migration); every DB call that touches them takes the folder's canonical key.

**Tech Stack:** Rust (Tauri v2, rusqlite, quick-xml via the existing `xmlconfig`, serde_json), Svelte 5 runes + TypeScript, `@tauri-apps/plugin-dialog`.

**Spec:** `docs/superpowers/specs/2026-10-15-mods-folder-selection-design.md` (issues #162, #171).

---

## Conventions for every task

- Work on a feature branch (`feat/mods-folder-selection`), one commit per task. Commit messages end with the `Co-Authored-By` trailer the session specifies.
- **Run cargo from PowerShell, not Git Bash.** In Git Bash, `/usr/bin/link.exe` (a Unix tool) shadows the MSVC linker and every build fails with "linking with `link.exe` failed".
- Rust test command (from `src-tauri/`): `cargo test <filter>`. Full gate before the PR: `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, then `npm run build` from the repo root.
- Clippy runs with `-D warnings`, so a function used only on another OS (or only in tests) must carry `#[allow(dead_code)]` or it fails the Windows build.

## File map

| File | Change | Responsibility |
|---|---|---|
| `src-tauri/src/fsgame.rs` | modify | FS25 user-dir discovery: `GameLocation`, `locate()`, VDF parse, candidate lists, `pick_user_dir`. |
| `src-tauri/src/modsroot.rs` | **create** | Folder keys, `modsDirectoryOverride` read/write, `PathMap`, `resolve`, active-root global, known-folders list, folder validation. |
| `src-tauri/src/db.rs` | modify | Root-scoped schema + migration; `root` parameter on all `organized`/`projection` functions; `claim_unscoped`, `count_organized`. |
| `src-tauri/src/organize.rs` | modify | Pass the folder key to every DB call; tests updated; cross-folder isolation test. |
| `src-tauri/src/lib.rs` | modify | `allowed_roots()` reads the active root; scan reconcile + `get_organized` scoped; new commands. |
| `src/lib/api.ts` | modify | `ModsRootStatus` types + command wrappers + `pickFolder`. |
| `src/App.svelte` | modify | Startup via `modsRootStatus`, switching, banners. |
| `src/lib/components/panels/SettingsPanel.svelte` | modify | Mods-folder section (source label, Change…, Use the game's folder, known folders). |
| `docs/…`, `CHANGELOG.md`, `CLAUDE.md` | modify | Smoke test items, cross-platform notes, changelog, feature list, spec corrections. |

---

### Task 1: User-dir discovery in `fsgame`

**Files:**
- Modify: `src-tauri/src/fsgame.rs` (whole file, currently 41 lines)

- [ ] **Step 1: Write the failing tests** — append to `src-tauri/src/fsgame.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn parses_library_folder_paths_in_order() {
        let vdf = r#""libraryfolders"
{
	"0"
	{
		"path"		"/home/u/.local/share/Steam"
		"label"		""
		"apps"
		{
			"228980"		"1"
		}
	}
	"1"
	{
		"path"		"/mnt/games/SteamLibrary"
	}
	"2"
	{
		"path"		"E:\\SteamLibrary"
	}
}"#;
        assert_eq!(
            parse_library_folders(vdf),
            vec![
                PathBuf::from("/home/u/.local/share/Steam"),
                PathBuf::from("/mnt/games/SteamLibrary"),
                PathBuf::from(r"E:\SteamLibrary"),
            ]
        );
    }

    #[test]
    fn pick_prefers_log_then_mods_then_existing() {
        let a = PathBuf::from("/a");
        let b = PathBuf::from("/b");
        let c = vec![a.clone(), b.clone()];

        let has = |set: &[&str]| -> HashSet<PathBuf> { set.iter().map(PathBuf::from).collect() };

        // b has log.txt (game ran there) → b wins even though a comes first and has mods/.
        let s = has(&["/a", "/a/mods", "/b", "/b/log.txt"]);
        assert_eq!(pick_user_dir(&c, |p| s.contains(p)), Some(b.clone()));
        // neither has log.txt; b has mods/ → b.
        let s = has(&["/a", "/b", "/b/mods"]);
        assert_eq!(pick_user_dir(&c, |p| s.contains(p)), Some(b.clone()));
        // only existence → first existing.
        let s = has(&["/b"]);
        assert_eq!(pick_user_dir(&c, |p| s.contains(p)), Some(b));
        // nothing exists → None.
        assert_eq!(pick_user_dir(&c, |_| false), None);
    }

    #[test]
    fn mac_candidates_put_the_app_store_container_first() {
        let c = mac_candidates(Path::new("/Users/u"));
        assert_eq!(
            c[0],
            PathBuf::from("/Users/u/Library/Containers/com.giants-software.fs25/Data/Library/Application Support/FarmingSimulator2025")
        );
        assert_eq!(c[1], PathBuf::from("/Users/u/Library/Application Support/FarmingSimulator2025"));
    }

    #[test]
    fn linux_candidates_cover_every_library_and_flatpak() {
        let home = Path::new("/home/u");
        let native = home.join(".local/share/Steam");
        let vdf_path = native.join("steamapps/libraryfolders.vdf");
        let vdf = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/games\"\n\t}\n}";
        let read = |p: &Path| (p == vdf_path).then(|| vdf.to_string());
        let c = linux_candidates(home, read);

        let fs25 = "steamapps/compatdata/2300320/pfx/drive_c/users/steamuser/Documents/My Games/FarmingSimulator2025";
        let pfx = "steamapps/compatdata/2300320/pfx";
        // Secondary library from the VDF is included, with its prefix.
        assert!(c.contains(&(PathBuf::from("/mnt/games").join(fs25), PathBuf::from("/mnt/games").join(pfx))));
        // Flatpak root is tried even with no VDF.
        let flat = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        assert!(c.contains(&(flat.join(fs25), flat.join(pfx))));
        // No duplicates (the native root appears both as a root and in its own VDF).
        let unique: HashSet<_> = c.iter().collect();
        assert_eq!(unique.len(), c.len());
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run (PowerShell, in `src-tauri/`): `cargo test fsgame::tests`
Expected: compile errors — `parse_library_folders`, `pick_user_dir`, `mac_candidates`, `linux_candidates` not found.

- [ ] **Step 3: Implement** — replace the body of `src-tauri/src/fsgame.rs` above the test module with:

```rust
//! FS25 game-file discovery. Cross-platform; see docs/CROSS-PLATFORM.md.
//! Finds the FS25 user data dir (parent of `mods/`, `savegameN/`, `gameSettings.xml`) per OS,
//! including the Proton prefix on Linux and the sandboxed Mac App Store container.

use std::path::{Path, PathBuf};

/// Steam app id of Farming Simulator 25.
const FS25_APP_ID: &str = "2300320";

/// A located FS25 user data dir, plus the Proton prefix it lives in (Linux only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLocation {
    pub user_dir: PathBuf,
    pub prefix: Option<PathBuf>,
}

/// Library folder paths from Steam's `libraryfolders.vdf` (`"path"` values, in file order).
/// The format is nested `"key" "value"` / `"key" { … }`; only `path` values matter here.
pub fn parse_library_folders(vdf: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for line in vdf.lines() {
        let mut parts = line.split('"').filter(|s| !s.trim().is_empty());
        if parts.next() == Some("path") {
            if let Some(v) = parts.next() {
                out.push(PathBuf::from(v.replace("\\\\", "\\")));
            }
        }
    }
    out
}

/// Choose among candidate user dirs: prefer one where the game has run (`log.txt`), then one
/// with a `mods/` folder, then the first that exists. `exists` is injected so this is testable.
pub fn pick_user_dir(cands: &[PathBuf], exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    cands
        .iter()
        .find(|c| exists(&c.join("log.txt")))
        .or_else(|| cands.iter().find(|c| exists(&c.join("mods"))))
        .or_else(|| cands.iter().find(|c| exists(c)))
        .cloned()
}

/// macOS candidates, most specific first: the sandboxed Mac App Store build keeps its data in
/// its container (verified from game logs, issue #171); Steam / GIANTS-direct builds use
/// Application Support.
#[allow(dead_code)] // used on macOS and in tests
fn mac_candidates(home: &Path) -> Vec<PathBuf> {
    vec![
        home.join("Library/Containers/com.giants-software.fs25/Data/Library/Application Support/FarmingSimulator2025"),
        home.join("Library/Application Support/FarmingSimulator2025"),
    ]
}

/// Linux (Proton) candidates as `(user_dir, prefix)` pairs: every Steam library under the
/// native and Flatpak Steam roots, each with FS25's compatdata prefix. `read` loads a file's
/// text (injected for tests).
#[allow(dead_code)] // used on Linux and in tests
fn linux_candidates(home: &Path, read: impl Fn(&Path) -> Option<String>) -> Vec<(PathBuf, PathBuf)> {
    let roots = [
        home.join(".steam/steam"),
        home.join(".local/share/Steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    let mut libs: Vec<PathBuf> = Vec::new();
    for root in &roots {
        let mut found = vec![root.clone()];
        if let Some(vdf) = read(&root.join("steamapps/libraryfolders.vdf")) {
            found.extend(parse_library_folders(&vdf));
        }
        for l in found {
            if !libs.contains(&l) {
                libs.push(l);
            }
        }
    }
    libs.into_iter()
        .map(|lib| {
            let prefix = lib.join("steamapps/compatdata").join(FS25_APP_ID).join("pfx");
            let user = prefix.join("drive_c/users/steamuser/Documents/My Games/FarmingSimulator2025");
            (user, prefix)
        })
        .collect()
}

/// Locate the FS25 user data dir for this OS.
pub fn locate() -> Option<GameLocation> {
    #[cfg(target_os = "windows")]
    {
        let user_dir = dirs::document_dir()?.join("My Games").join("FarmingSimulator2025");
        Some(GameLocation { user_dir, prefix: None })
    }
    #[cfg(target_os = "macos")]
    {
        let c = mac_candidates(&dirs::home_dir()?);
        // Nothing exists yet → keep the historical default (Application Support).
        let user_dir = pick_user_dir(&c, |p| p.exists()).or_else(|| c.last().cloned())?;
        Some(GameLocation { user_dir, prefix: None })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let pairs = linux_candidates(&dirs::home_dir()?, |p| std::fs::read_to_string(p).ok());
        let dirs_only: Vec<PathBuf> = pairs.iter().map(|(u, _)| u.clone()).collect();
        let user_dir = pick_user_dir(&dirs_only, |p| p.exists())?;
        let prefix = pairs.into_iter().find(|(u, _)| *u == user_dir).map(|(_, p)| p);
        Some(GameLocation { user_dir, prefix })
    }
}

/// The FS25 user data directory, per platform.
pub fn user_dir() -> Option<PathBuf> {
    locate().map(|l| l.user_dir)
}

/// Default mod root(s) the game reads when no override is set: `<user dir>/mods`, if present.
pub fn default_mods_paths() -> Vec<PathBuf> {
    match user_dir() {
        Some(dir) => {
            let mods = dir.join("mods");
            if mods.is_dir() {
                vec![mods]
            } else {
                vec![]
            }
        }
        None => vec![],
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test fsgame::tests`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 5: Check the whole crate still builds clean**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: `Finished` with no warnings (callers of `user_dir()`/`default_mods_paths()` are unchanged).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/fsgame.rs
git commit -m "feat(fsgame): candidate-based FS25 user-dir discovery (Linux/Proton, Mac App Store)"
```

---

### Task 2: `modsroot` — folder keys

**Files:**
- Create: `src-tauri/src/modsroot.rs`
- Modify: `src-tauri/src/lib.rs:4-29` (module list)

- [ ] **Step 1: Register the module** — in `src-tauri/src/lib.rs`, add `pub mod modsroot;` to the alphabetical `pub mod` list (between `pub mod moddesc;` and `pub mod mpsync;`).

- [ ] **Step 2: Write the failing tests** — create `src-tauri/src/modsroot.rs` with only:

```rust
//! Which mods folder Silo manages (#162). Pure logic plus the one trusted "active root".

use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_keys_ignore_case_verbatim_prefix_and_trailing_separators() {
        assert_eq!(normalize_key(r"\\?\C:\Users\Me\Mods\", true), r"c:\users\me\mods");
        assert_eq!(normalize_key("C:/Users/me/mods", true), r"c:\users\me\mods");
        assert_eq!(normalize_key(r"\\?\UNC\nas\share\mods", true), r"\\nas\share\mods");
        assert_eq!(normalize_key(r"C:\", true), r"c:\");
    }

    #[test]
    fn unix_keys_keep_case_and_drop_trailing_slash() {
        assert_eq!(normalize_key("/mnt/HDD/mods/", false), "/mnt/HDD/mods");
        assert_eq!(normalize_key("/", false), "/");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test modsroot::tests`
Expected: compile error — `normalize_key` not found.

- [ ] **Step 4: Implement** — insert above the test module:

```rust
/// Normalize a path string into a folder identity key. Windows: strip the `\\?\` verbatim
/// prefix, unify separators, lowercase (NTFS is case-insensitive). Elsewhere: keep case.
/// Trailing separators are dropped except on a bare root.
pub fn normalize_key(raw: &str, windows: bool) -> String {
    let mut s = raw.to_string();
    if windows {
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            s = format!(r"\\{rest}");
        } else if let Some(rest) = s.strip_prefix(r"\\?\") {
            s = rest.to_string();
        }
        s = s.replace('/', "\\");
        while s.len() > 3 && s.ends_with('\\') {
            s.pop();
        }
        s.to_lowercase()
    } else {
        while s.len() > 1 && s.ends_with('/') {
            s.pop();
        }
        s
    }
}

/// The identity key for a folder: canonicalized when it exists, then normalized. This is the
/// value stored in the `root` column and used for every "same folder?" comparison.
pub fn canonical_key(p: &Path) -> String {
    let c = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    normalize_key(&c.to_string_lossy(), cfg!(windows))
}

/// True when two paths name the same folder.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    canonical_key(a) == canonical_key(b)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test modsroot::tests`
Expected: `2 passed`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/modsroot.rs src-tauri/src/lib.rs
git commit -m "feat(modsroot): folder identity keys"
```

---

### Task 3: `modsroot` — read and write FS25's `modsDirectoryOverride`

**Files:**
- Modify: `src-tauri/src/modsroot.rs`

Facts this relies on: the real file's root element is `<gameSettings>` and the element is `<modsDirectoryOverride active="false" directory="C:/Temp"/>` — FS25 writes the directory with **forward slashes**. `xmlconfig::set_values` only replaces attributes that already exist; it never adds one, so writing must first check both attributes are present.

- [ ] **Step 1: Write the failing tests** — add inside `mod tests`:

```rust
    const GS: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\" ?>\n<gameSettings revision=\"15\">\n    <modsDirectoryOverride active=\"false\" directory=\"C:/Temp\"/>\n    <defaultMultiplayerPort>10823</defaultMultiplayerPort>\n</gameSettings>\n";

    #[test]
    fn reads_the_override() {
        assert_eq!(
            read_override(GS),
            Some(Override { active: false, directory: "C:/Temp".into() })
        );
        assert_eq!(read_override("<gameSettings><x/></gameSettings>"), None);
    }

    #[test]
    fn writes_the_override_and_preserves_everything_else() {
        let out = write_override(GS, "D:/FS25 Mods").unwrap();
        assert_eq!(
            read_override(&out),
            Some(Override { active: true, directory: "D:/FS25 Mods".into() })
        );
        assert!(out.starts_with('\u{feff}'), "BOM kept");
        assert!(out.contains("<defaultMultiplayerPort>10823</defaultMultiplayerPort>"));
    }

    #[test]
    fn refuses_to_write_when_the_element_is_missing() {
        assert!(write_override("<gameSettings><x/></gameSettings>", "D:/m").is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test modsroot::tests`
Expected: compile errors — `read_override`, `write_override`, `Override` not found.

- [ ] **Step 3: Implement** — add below the imports (and add `use crate::xmlconfig;`):

```rust
/// FS25's own mods-folder redirect in `gameSettings.xml`:
/// `<modsDirectoryOverride active="…" directory="…"/>`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub active: bool,
    pub directory: String,
}

const OV_ACTIVE: &str = "gameSettings.modsDirectoryOverride@active";
const OV_DIR: &str = "gameSettings.modsDirectoryOverride@directory";

fn override_paths() -> [String; 2] {
    [OV_ACTIVE.to_string(), OV_DIR.to_string()]
}

/// Read the override, or `None` when `gameSettings.xml` has no such element.
pub fn read_override(xml: &str) -> Option<Override> {
    let v = xmlconfig::get_values(xml, &override_paths());
    let directory = v.get(OV_DIR)?.clone();
    let active = v.get(OV_ACTIVE).map(|a| a.eq_ignore_ascii_case("true")).unwrap_or(false);
    Some(Override { active, directory })
}

/// Turn the override on and point it at `game_path` (a path in the game's own view — see
/// `PathMap::to_game`). Errors when the element or its attributes are missing, because
/// `xmlconfig::set_values` can only change values that already exist.
pub fn write_override(xml: &str, game_path: &str) -> Result<String, String> {
    let v = xmlconfig::get_values(xml, &override_paths());
    if !v.contains_key(OV_ACTIVE) || !v.contains_key(OV_DIR) {
        return Err("gameSettings.xml has no modsDirectoryOverride setting to update".into());
    }
    xmlconfig::set_values(
        xml,
        &[
            xmlconfig::Edit { path: OV_ACTIVE.into(), value: "true".into() },
            xmlconfig::Edit { path: OV_DIR.into(), value: game_path.into() },
        ],
    )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test modsroot::tests`
Expected: `5 passed`. If `reads_the_override` fails because `get_values` does not report attributes of a self-closing element, read `xmlconfig::get_values` (`src-tauri/src/xmlconfig.rs:40`) and handle `Event::Empty` there the same way it handles `Event::Start`, with a test in `xmlconfig`'s own module.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modsroot.rs
git commit -m "feat(modsroot): read/write FS25 modsDirectoryOverride"
```

---

### Task 4: `modsroot` — `PathMap` (Wine path translation)

**Files:**
- Modify: `src-tauri/src/modsroot.rs`

- [ ] **Step 1: Write the failing tests** — add inside `mod tests`:

```rust
    fn wine() -> PathMap {
        PathMap::new(vec![
            ('c', PathBuf::from("/home/u/pfx/drive_c")),
            ('Z', PathBuf::from("/")),
            ('d', PathBuf::from("/mnt/hdd")),
        ])
    }

    #[test]
    fn identity_map_passes_paths_through() {
        let m = PathMap::identity();
        assert_eq!(m.to_host("C:/Temp"), Some(PathBuf::from("C:/Temp")));
        assert_eq!(m.to_game(Path::new(r"D:\FS25 Mods")), "D:/FS25 Mods");
    }

    #[test]
    fn to_host_uses_the_drive_map() {
        let m = wine();
        assert_eq!(m.to_host(r"Z:\mnt\hdd\mods"), Some(PathBuf::from("/mnt/hdd/mods")));
        assert_eq!(m.to_host("C:/users/steamuser/x"), Some(PathBuf::from("/home/u/pfx/drive_c/users/steamuser/x")));
        assert_eq!(m.to_host("D:/"), Some(PathBuf::from("/mnt/hdd")));
        assert_eq!(m.to_host("Q:/nope"), None, "unknown drive letter is unresolvable, never guessed");
        assert_eq!(m.to_host("relative/path"), None);
    }

    #[test]
    fn to_game_picks_the_longest_drive_match() {
        let m = wine();
        assert_eq!(m.to_game(Path::new("/mnt/hdd/mods")), "D:/mods");
        assert_eq!(m.to_game(Path::new("/home/u/pfx/drive_c/users/a")), "C:/users/a");
        assert_eq!(m.to_game(Path::new("/srv/fs25")), "Z:/srv/fs25");
    }

    #[test]
    fn to_game_falls_back_to_z_without_a_root_mapping() {
        let m = PathMap::new(vec![('c', PathBuf::from("/home/u/pfx/drive_c"))]);
        assert_eq!(m.to_game(Path::new("/srv/fs25")), "Z:/srv/fs25");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test modsroot::tests`
Expected: compile error — `PathMap` not found.

- [ ] **Step 3: Implement** — change the import to `use std::path::{Path, PathBuf};`, then add to `modsroot.rs`:

```rust
/// Translates between the game's view of a path and the host path. Empty = identity
/// (Windows, macOS). On Linux it is Wine's drive map from the Proton prefix.
#[derive(Debug, Clone, Default)]
pub struct PathMap {
    drives: Vec<(char, PathBuf)>,
}

impl PathMap {
    pub fn identity() -> Self {
        Self::default()
    }

    pub fn new(drives: Vec<(char, PathBuf)>) -> Self {
        Self {
            drives: drives.into_iter().map(|(l, p)| (l.to_ascii_lowercase(), p)).collect(),
        }
    }

    /// Wine's drive map: each `<prefix>/dosdevices/<x>:` entry is a symlink to that drive's root.
    pub fn from_prefix(prefix: &Path) -> Self {
        let dd = prefix.join("dosdevices");
        let mut drives = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&dd) {
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().to_lowercase();
                let b = name.as_bytes();
                if b.len() == 2 && b[1] == b':' && b[0].is_ascii_alphabetic() {
                    if let Ok(t) = std::fs::read_link(e.path()) {
                        let abs = if t.is_absolute() { t } else { dd.join(t) };
                        let abs = std::fs::canonicalize(&abs).unwrap_or(abs);
                        drives.push((b[0] as char, abs));
                    }
                }
            }
        }
        Self::new(drives)
    }

    /// Game path (`Z:\mnt\hdd\mods` or `C:/Temp`) → host path. `None` when the drive letter
    /// has no mapping or the path has no drive letter.
    pub fn to_host(&self, game: &str) -> Option<PathBuf> {
        if self.drives.is_empty() {
            return Some(PathBuf::from(game));
        }
        let g = game.replace('\\', "/");
        let b = g.as_bytes();
        if b.len() < 2 || b[1] != b':' {
            return None;
        }
        let letter = (b[0] as char).to_ascii_lowercase();
        let (_, root) = self.drives.iter().find(|(l, _)| *l == letter)?;
        let rest = g[2..].trim_start_matches('/');
        Some(if rest.is_empty() { root.clone() } else { root.join(rest) })
    }

    /// Host path → game path, written with forward slashes the way FS25 writes it.
    /// Longest matching drive wins; no match → `Z:` + the host path (Wine maps `Z:` to `/`).
    pub fn to_game(&self, host: &Path) -> String {
        if self.drives.is_empty() {
            return host.to_string_lossy().replace('\\', "/");
        }
        let join = |letter: char, rest: &Path| {
            let parts: Vec<String> = rest
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            format!("{}:/{}", letter.to_ascii_uppercase(), parts.join("/"))
        };
        let best = self
            .drives
            .iter()
            .filter(|(_, r)| host.starts_with(r))
            .max_by_key(|(_, r)| r.components().count());
        match best {
            Some((l, r)) => join(*l, host.strip_prefix(r).unwrap_or(host)),
            None => join('z', host),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test modsroot::tests`
Expected: `9 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modsroot.rs
git commit -m "feat(modsroot): Wine drive-map path translation"
```

---

### Task 5: `modsroot` — resolve, active root, known folders, validation

**Files:**
- Modify: `src-tauri/src/modsroot.rs`

- [ ] **Step 1: Write the failing tests** — add inside `mod tests`:

```rust
    fn same(a: &Path, b: &Path) -> bool {
        a == b
    }
    fn ov(active: bool, dir: &str) -> Override {
        Override { active, directory: dir.into() }
    }

    #[test]
    fn resolve_precedence_choice_then_active_override_then_default() {
        let id = PathMap::identity();
        let def = Some(PathBuf::from("/def"));

        let r = resolve(None, None, def.clone(), &id, same);
        assert_eq!((r.path.clone(), r.source, r.mismatch), (def.clone(), Source::Default, false));

        let o = ov(true, "/ov");
        let r = resolve(None, Some(&o), def.clone(), &id, same);
        assert_eq!((r.path.clone(), r.source), (Some(PathBuf::from("/ov")), Source::GameOverride));
        assert_eq!(r.game_reads, Some(PathBuf::from("/ov")));

        let r = resolve(Some(PathBuf::from("/mine")), Some(&o), def.clone(), &id, same);
        assert_eq!((r.path.clone(), r.source, r.mismatch), (Some(PathBuf::from("/mine")), Source::User, true));

        let r = resolve(Some(PathBuf::from("/ov")), Some(&o), def, &id, same);
        assert!(!r.mismatch, "choosing the folder the game reads is not a mismatch");
    }

    #[test]
    fn resolve_ignores_an_inactive_override_and_reports_untranslatable_ones() {
        let def = Some(PathBuf::from("/def"));
        let r = resolve(None, Some(&ov(false, "/ov")), def.clone(), &PathMap::identity(), same);
        assert_eq!(r.source, Source::Default);

        let m = PathMap::new(vec![('z', PathBuf::from("/"))]);
        let r = resolve(None, Some(&ov(true, "Q:/x")), def.clone(), &m, same);
        assert_eq!(r.unresolved_override.as_deref(), Some("Q:/x"));
        assert_eq!((r.path, r.source), (def, Source::Default));
    }

    #[test]
    fn known_list_adds_once_and_survives_bad_json() {
        let a = add_known(None, "/a");
        let ab = add_known(Some(&a), "/b");
        assert_eq!(parse_known(Some(&add_known(Some(&ab), "/a"))), vec!["/a", "/b"]);
        assert!(parse_known(Some("not json")).is_empty());
    }

    #[test]
    fn active_root_round_trips() {
        set_active(Some(PathBuf::from("/x")));
        assert_eq!(active(), Some(PathBuf::from("/x")));
        set_active(None);
        assert_eq!(active(), None);
    }

    #[test]
    fn validate_folder_rejects_missing_and_accepts_writable() {
        let base = std::env::temp_dir().join(format!("silo_modsroot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        assert!(validate_folder(&base).is_err());
        std::fs::create_dir_all(&base).unwrap();
        assert!(validate_folder(&base).is_ok());
        assert!(!base.join(".silo-write-probe").exists(), "probe cleaned up");
        let _ = std::fs::remove_dir_all(&base);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test modsroot::tests`
Expected: compile errors — `resolve`, `Source`, `add_known`, `parse_known`, `set_active`, `active`, `validate_folder` not found.

- [ ] **Step 3: Implement** — add `use serde::Serialize;` and `use std::sync::RwLock;` to the imports, then:

```rust
/// `app_setting` keys.
pub const CHOICE_KEY: &str = "mods_root_choice";
pub const KNOWN_KEY: &str = "mods_root_known";

/// Where the live folder came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    User,
    GameOverride,
    Default,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// The folder Silo manages.
    pub path: Option<PathBuf>,
    pub source: Source,
    /// The folder the game reads.
    pub game_reads: Option<PathBuf>,
    /// A user choice that differs from what the game reads.
    pub mismatch: bool,
    /// An active override whose game path could not be translated to a host path.
    pub unresolved_override: Option<String>,
}

/// Decide the live folder: user choice > active game override > default. `same` compares
/// folders (inject `same_folder` in production; plain equality in tests).
pub fn resolve(
    choice: Option<PathBuf>,
    ov: Option<&Override>,
    default: Option<PathBuf>,
    map: &PathMap,
    same: impl Fn(&Path, &Path) -> bool,
) -> Resolved {
    let mut unresolved_override = None;
    let override_path = match ov {
        Some(o) if o.active => {
            let p = map.to_host(&o.directory);
            if p.is_none() {
                unresolved_override = Some(o.directory.clone());
            }
            p
        }
        _ => None,
    };
    let from_override = override_path.is_some();
    let game_reads = override_path.or(default);
    let (path, source) = match choice {
        Some(c) => (Some(c), Source::User),
        None => (
            game_reads.clone(),
            if from_override { Source::GameOverride } else { Source::Default },
        ),
    };
    let mismatch = source == Source::User
        && match (&path, &game_reads) {
            (Some(a), Some(b)) => !same(a, b),
            (Some(_), None) => true,
            _ => false,
        };
    Resolved { path, source, game_reads, mismatch, unresolved_override }
}

/// The single trusted write root, published by the mods-root commands. `lib::allowed_roots`
/// reads it, so every organize/install command follows the folder the user picked.
static ACTIVE: RwLock<Option<PathBuf>> = RwLock::new(None);

pub fn set_active(p: Option<PathBuf>) {
    if let Ok(mut g) = ACTIVE.write() {
        *g = p;
    }
}

pub fn active() -> Option<PathBuf> {
    ACTIVE.read().ok().and_then(|g| g.clone())
}

/// The known-folders list stored as a JSON array of path strings.
pub fn parse_known(json: Option<&str>) -> Vec<String> {
    json.and_then(|j| serde_json::from_str::<Vec<String>>(j).ok()).unwrap_or_default()
}

/// Add `path` to the known list (once) and return the new JSON.
pub fn add_known(json: Option<&str>, path: &str) -> String {
    let mut v = parse_known(json);
    if !v.iter().any(|k| k == path) {
        v.push(path.to_string());
    }
    serde_json::to_string(&v).unwrap_or_else(|_| "[]".into())
}

/// A folder Silo may manage: it exists and Silo can write to it.
pub fn validate_folder(p: &Path) -> Result<(), String> {
    if !p.is_dir() {
        return Err("That folder doesn't exist".into());
    }
    let probe = p.join(".silo-write-probe");
    std::fs::write(&probe, b"silo").map_err(|_| "Silo can't write to that folder".to_string())?;
    let _ = std::fs::remove_file(&probe);
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test modsroot::tests`
Expected: `14 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/modsroot.rs
git commit -m "feat(modsroot): resolve live folder, active root, known folders"
```

---

### Task 6: Root-scoped schema and migration

**Files:**
- Modify: `src-tauri/src/db.rs` (`open()` around lines 79-153; tests module at line 616)

- [ ] **Step 1: Write the failing tests** — add inside `db.rs`'s `mod tests`:

```rust
    fn temp_db(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("silo_db_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base.join("silo.db")
    }

    #[test]
    fn migrates_a_pre_root_database_and_is_idempotent() {
        let path = temp_db("migrate");
        {
            // The v0.7 schema, with one row in each table.
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE organized (tech_name TEXT PRIMARY KEY, file_name TEXT NOT NULL,
                     kind TEXT NOT NULL, category TEXT NOT NULL, subcategory TEXT,
                     active INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE projection (file_name TEXT PRIMARY KEY, size INTEGER NOT NULL,
                     mtime_ms INTEGER NOT NULL);
                 INSERT INTO organized VALUES ('FS25_A','FS25_A.zip','zip','Vehicles',NULL,1);
                 INSERT INTO projection VALUES ('FS25_A.zip', 10, 20);",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        // Old rows arrive unscoped (root = '') until claimed.
        assert_eq!(load_organized(&conn, "").len(), 1);
        claim_unscoped(&conn, "c:\\mods").unwrap();
        assert_eq!(load_organized(&conn, "c:\\mods")[0].tech_name, "FS25_A");
        assert_eq!(projection_stats(&conn, "c:\\mods", "FS25_A.zip"), Some((10, 20)));
        assert!(load_organized(&conn, "").is_empty());
        drop(conn);
        // Re-opening must not rebuild again or lose rows.
        let conn = open(&path).unwrap();
        assert_eq!(load_organized(&conn, "c:\\mods").len(), 1);
    }

    #[test]
    fn the_same_mod_can_be_organized_in_two_folders() {
        let conn = open(&temp_db("twofolders")).unwrap();
        let row = |active| OrganizedRow {
            tech_name: "FS25_A".into(),
            file_name: "FS25_A.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
            active,
        };
        upsert_organized(&conn, "a", &row(true)).unwrap();
        upsert_organized(&conn, "b", &row(false)).unwrap();
        assert!(load_organized(&conn, "a")[0].active);
        assert!(!load_organized(&conn, "b")[0].active);
        assert_eq!(count_organized(&conn, "a"), 1);
        delete_organized(&conn, "b", "FS25_A").unwrap();
        assert_eq!(count_organized(&conn, "a"), 1, "deleting in b leaves a alone");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test db::tests`
Expected: compile errors — functions take the wrong number of arguments; `claim_unscoped`, `count_organized` not found.

- [ ] **Step 3: New-database schema** — in `open()`, replace the `organized` and `projection` `CREATE TABLE IF NOT EXISTS` statements with:

```sql
         CREATE TABLE IF NOT EXISTS organized (
             root        TEXT NOT NULL DEFAULT '',
             tech_name   TEXT NOT NULL,
             file_name   TEXT NOT NULL,
             kind        TEXT NOT NULL,
             category    TEXT NOT NULL,
             subcategory TEXT,
             active      INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (root, tech_name)
         );
```

```sql
         CREATE TABLE IF NOT EXISTS projection (
             root      TEXT NOT NULL DEFAULT '',
             file_name TEXT NOT NULL,
             size      INTEGER NOT NULL,
             mtime_ms  INTEGER NOT NULL,
             PRIMARY KEY (root, file_name)
         );
```

- [ ] **Step 4: Migration for existing databases** — in `open()`, after the `ALTER TABLE curation …` line and before `Ok(conn)`, add `migrate_scope_by_root(&conn)?;`, and add these functions below `open()`:

```rust
fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    let Ok(mut st) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let names = st.query_map([], |r| r.get::<_, String>(1));
    names.map(|it| it.flatten().any(|n| n == col)).unwrap_or(false)
}

/// v0.8 (#162): scope the organize manifest and projection records to a mods folder. SQLite
/// can't change a primary key in place, so both tables are rebuilt in one transaction; old
/// rows get `root = ''` until `claim_unscoped` stamps them with the default folder. Skipped
/// when already migrated. A failure rolls back and leaves the old schema intact.
fn migrate_scope_by_root(conn: &Connection) -> Result<(), String> {
    if has_column(conn, "organized", "root") && has_column(conn, "projection", "root") {
        return Ok(());
    }
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE organized_new (
             root TEXT NOT NULL DEFAULT '', tech_name TEXT NOT NULL, file_name TEXT NOT NULL,
             kind TEXT NOT NULL, category TEXT NOT NULL, subcategory TEXT,
             active INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (root, tech_name));
         INSERT INTO organized_new(root, tech_name, file_name, kind, category, subcategory, active)
             SELECT '', tech_name, file_name, kind, category, subcategory, active FROM organized;
         DROP TABLE organized;
         ALTER TABLE organized_new RENAME TO organized;
         CREATE TABLE projection_new (
             root TEXT NOT NULL DEFAULT '', file_name TEXT NOT NULL, size INTEGER NOT NULL,
             mtime_ms INTEGER NOT NULL, PRIMARY KEY (root, file_name));
         INSERT INTO projection_new(root, file_name, size, mtime_ms)
             SELECT '', file_name, size, mtime_ms FROM projection;
         DROP TABLE projection;
         ALTER TABLE projection_new RENAME TO projection;
         COMMIT;",
    )
    .map_err(|e| {
        let _ = conn.execute_batch("ROLLBACK;");
        format!("database migration failed: {e}")
    })
}

/// Stamp rows written before folders were tracked (`root = ''`) with `root`. Before v0.8 Silo
/// only ever managed the default folder, so callers pass that folder's key. Idempotent; a
/// stale unscoped row that collides with an existing scoped one is dropped.
pub fn claim_unscoped(conn: &Connection, root: &str) -> Result<(), String> {
    for sql in [
        "UPDATE OR IGNORE organized SET root = ?1 WHERE root = ''",
        "UPDATE OR IGNORE projection SET root = ?1 WHERE root = ''",
    ] {
        conn.execute(sql, [root]).map_err(|e| e.to_string())?;
    }
    conn.execute_batch("DELETE FROM organized WHERE root = ''; DELETE FROM projection WHERE root = '';")
        .map_err(|e| e.to_string())
}

/// How many mods Silo has organized in this folder.
pub fn count_organized(conn: &Connection, root: &str) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM organized WHERE root = ?1", [root], |r| r.get(0))
        .unwrap_or(0)
}
```

- [ ] **Step 5: Scope the existing functions** — replace `load_organized` through `reconcile_organized` (lines ~323-426) with:

```rust
/// Load the organize manifest for one mods folder (`root` = its `modsroot::canonical_key`).
pub fn load_organized(conn: &Connection, root: &str) -> Vec<OrganizedRow> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT tech_name, file_name, kind, category, subcategory, active
         FROM organized WHERE root = ?1",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([root], |r| {
        Ok(OrganizedRow {
            tech_name: r.get(0)?,
            file_name: r.get(1)?,
            kind: r.get(2)?,
            category: r.get(3)?,
            subcategory: r.get(4)?,
            active: r.get::<_, i64>(5)? != 0,
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

pub fn upsert_organized(conn: &Connection, root: &str, row: &OrganizedRow) -> Result<(), String> {
    conn.execute(
        "INSERT INTO organized(root, tech_name, file_name, kind, category, subcategory, active)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(root, tech_name) DO UPDATE SET
             file_name = excluded.file_name, kind = excluded.kind,
             category = excluded.category, subcategory = excluded.subcategory,
             active = excluded.active",
        rusqlite::params![
            root,
            row.tech_name,
            row.file_name,
            row.kind,
            row.category,
            row.subcategory,
            row.active as i64
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_organized_active(
    conn: &Connection,
    root: &str,
    tech_name: &str,
    active: bool,
) -> Result<(), String> {
    conn.execute(
        "UPDATE organized SET active = ?3 WHERE root = ?1 AND tech_name = ?2",
        rusqlite::params![root, tech_name, active as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_organized(conn: &Connection, root: &str, tech_name: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM organized WHERE root = ?1 AND tech_name = ?2",
        rusqlite::params![root, tech_name],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Record a projected file's identity (size + mtime) so a later remove can confirm it's Silo's
/// own untouched copy without hashing it.
pub fn record_projection(conn: &Connection, root: &str, file_name: &str, size: u64, mtime_ms: i64) {
    let _ = conn.execute(
        "INSERT INTO projection(root, file_name, size, mtime_ms) VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(root, file_name) DO UPDATE SET size = excluded.size, mtime_ms = excluded.mtime_ms",
        rusqlite::params![root, file_name, size as i64, mtime_ms],
    );
}

/// The recorded (size, mtime_ms) for a projected file in this folder, if any.
pub fn projection_stats(conn: &Connection, root: &str, file_name: &str) -> Option<(u64, i64)> {
    conn.query_row(
        "SELECT size, mtime_ms FROM projection WHERE root = ?1 AND file_name = ?2",
        rusqlite::params![root, file_name],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)?)),
    )
    .ok()
}

/// Drop a projection's provenance (it's been removed from the flat root).
pub fn forget_projection(conn: &Connection, root: &str, file_name: &str) {
    let _ = conn.execute(
        "DELETE FROM projection WHERE root = ?1 AND file_name = ?2",
        rusqlite::params![root, file_name],
    );
}

/// Make one folder's `organized` manifest match what's physically in its `archive/`. Upserts a
/// row for every archived mod (so `set_active` can project it) and drops rows for mods that are
/// no longer archived. Other folders' rows are never touched.
pub fn reconcile_organized(
    conn: &mut Connection,
    root: &str,
    desired: &[OrganizedRow],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let keep: std::collections::HashSet<&str> =
        desired.iter().map(|r| r.tech_name.as_str()).collect();
    for existing in load_organized(&tx, root) {
        if !keep.contains(existing.tech_name.as_str()) {
            delete_organized(&tx, root, &existing.tech_name)?;
        }
    }
    for row in desired {
        upsert_organized(&tx, root, row)?;
    }
    tx.commit().map_err(|e| e.to_string())
}
```

- [ ] **Step 6: Update the existing `reconcile_adopts_disk_and_prunes_gone` test** (db.rs ~line 620) to the new signatures: `upsert_organized(&conn, "r", &OrganizedRow { … })`, `reconcile_organized(&mut conn, "r", &desired)`, and `let rows = load_organized(&conn, "r");`. Its assertions are unchanged.

- [ ] **Step 7: Run the db tests**

Run: `cargo test db::tests`
Expected: the crate does **not** compile yet (organize.rs and lib.rs still use the old signatures). That's Task 7 and 8; continue without committing.

---

### Task 7: Scope `organize.rs` by folder key

**Files:**
- Modify: `src-tauri/src/organize.rs` (non-test code lines 138-800; tests from line 822)

- [ ] **Step 1: Thread the key through the engine.** At the top of each of these functions, add `let key = crate::modsroot::canonical_key(root);`, then pass `&key` as the new second argument to every `db::` call inside it:

| Function (line) | DB calls to update |
|---|---|
| `apply_organize` (138) | `load_organized`, `upsert_organized`, `delete_organized` |
| `set_active` (202) | `load_organized`, `record_projection`, `set_organized_active`, `forget_projection`, and the `matches_provenance` call |
| `relink_projections` (306) | `load_organized` |
| `adopt_foreign` (403) | `load_organized` |
| `restore_projection` (432) | `load_organized` |
| `flatten` (468) | `load_organized`, `delete_organized` |
| `detect_foreign_projections` (724) | `record_projection`, and the `matches_provenance` call |

For example, in `set_active`:

```rust
pub fn set_active(conn: &Connection, root: &Path, active: &HashSet<String>) -> Report {
    let key = crate::modsroot::canonical_key(root);
    let mut rep = Report::default();
    for row in db::load_organized(conn, &key) {
        // …unchanged…
                            db::record_projection(conn, &key, &row.file_name, size, mtime);
        // …
                    if let Err(e) = db::set_organized_active(conn, &key, &row.tech_name, true) {
```

If any of those functions calls another helper that itself calls `db::`, add a `key: &str` parameter to that helper and pass `&key` the same way. `cargo check` lists every remaining call site.

- [ ] **Step 2: Scope `matches_provenance`** — replace it (line 288) with:

```rust
/// Fast, hash-free ownership check: the flat file is provably Silo's own untouched projection if
/// its current `(size, mtime)` still match what we recorded when we wrote it **in this folder**.
/// A file the user swapped in has a different mtime and won't match — so it's never wrongly
/// treated as ours — and another folder's records can never vouch for it.
fn matches_provenance(conn: &Connection, key: &str, link: &Path, file_name: &str) -> bool {
    let Some((size, mtime)) = db::projection_stats(conn, key, file_name) else {
        return false;
    };
    matches!(file_stats(link), Some((s, m)) if s == size && m == mtime)
}
```

Call sites become `matches_provenance(conn, &key, &link, &row.file_name)` (line ~242) and `matches_provenance(conn, &key, &link, &file_name)` (line ~774).

- [ ] **Step 3: Update the existing tests.** In `mod tests`, every test creates a temp folder `base` (or similar) and passes it as `root`. In each test that calls `db::` directly, add `let key = crate::modsroot::canonical_key(&base);` (use that test's root variable) right after the folder is created, and pass `&key` as the new second argument to each `db::upsert_organized`, `db::load_organized`, `db::projection_stats`, `db::set_organized_active` call, and to `matches_provenance` (line ~1078: `matches_provenance(&conn, &key, &flat, "FS25_Foo.zip")`). Engine calls such as `flatten(&conn, &base)` are unchanged.

- [ ] **Step 4: Write the cross-folder isolation test** — add to `mod tests`:

```rust
    #[test]
    fn records_from_one_folder_never_authorize_changes_in_another() {
        let base = std::env::temp_dir().join(format!("silo_iso_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(a.join("archive").join("Vehicles")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();

        // Folder A: one organized mod, projected (active).
        std::fs::write(a.join("archive/Vehicles/FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let key_a = crate::modsroot::canonical_key(&a);
        let row = OrganizedRow {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
            active: false,
        };
        db::upsert_organized(&conn, &key_a, &row).unwrap();
        let on: HashSet<String> = ["FS25_Foo".to_string()].into_iter().collect();
        set_active(&conn, &a, &on);
        assert!(a.join("FS25_Foo.zip").exists());

        // Folder B: the user's own file at the same name, unknown to Silo.
        std::fs::write(b.join("FS25_Foo.zip"), b"PK\x03\x04 the user's own file").unwrap();

        // Deactivating everything in B must not touch B's file, and must not touch A.
        set_active(&conn, &b, &HashSet::new());
        flatten(&conn, &b);
        assert!(b.join("FS25_Foo.zip").exists(), "B's file is not Silo's — never removed");
        assert!(a.join("FS25_Foo.zip").exists(), "A's projection is untouched");
        assert!(db::load_organized(&conn, &key_a)[0].active, "A's state is untouched");
        let _ = std::fs::remove_dir_all(&base);
    }
```

- [ ] **Step 5: Check compile state**

Run: `cargo check --all-targets`
Expected: remaining errors only in `lib.rs` (Task 8).

---

### Task 8: `lib.rs` — follow the active root

**Files:**
- Modify: `src-tauri/src/lib.rs` (`scan_mods` ~78-145, `allowed_roots` ~849, `get_organized` ~1003)

- [ ] **Step 1: `allowed_roots()` reads the active root** — replace the function and its comment:

```rust
/// The mod roots Silo is allowed to write to: the folder the mods-root commands resolved
/// (user choice > FS25 override > default, see `modsroot`), or — before they have run — the
/// auto-detected default. This is the single source of truth for "where may we write";
/// Rust, not the webview, is the authority.
fn allowed_roots() -> Vec<PathBuf> {
    match modsroot::active() {
        Some(p) => vec![p],
        None => fsgame::default_mods_paths(),
    }
}
```

- [ ] **Step 2: `scan_mods` defaults to it and reconciles per folder.** Change the `roots` match arm `_ => fsgame::default_mods_paths(),` to `_ => allowed_roots(),`. Immediately after that `let roots …;` statement add:

```rust
    // The organize manifest is per folder; scope the reconcile to the folder being scanned.
    let scan_key = roots.first().map(|r| modsroot::canonical_key(r));
```

and replace `let _ = db::reconcile_organized(&mut conn, &organized_rows);` with:

```rust
        if let Some(key) = scan_key.as_deref() {
            let _ = db::reconcile_organized(&mut conn, key, &organized_rows);
        }
```

- [ ] **Step 3: `get_organized` returns the active folder's rows:**

```rust
#[tauri::command]
fn get_organized(app: tauri::AppHandle) -> Result<Vec<db::OrganizedRow>, String> {
    let conn = db::open(&db_path(&app)?)?;
    let key = allowed_roots().first().map(|r| modsroot::canonical_key(r)).unwrap_or_default();
    Ok(db::load_organized(&conn, &key))
}
```

- [ ] **Step 4: Run the whole suite**

Run: `cargo test`
Expected: all pass, including `db::tests` (2 new), `organize::tests` (1 new), `fsgame::tests` (4), `modsroot::tests` (14).

- [ ] **Step 5: Lint**

Run: `cargo fmt --all` then `cargo clippy --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 6: Commit Tasks 6-8 together** (they only compile as a unit)

```bash
git add src-tauri/src/db.rs src-tauri/src/organize.rs src-tauri/src/lib.rs
git commit -m "feat(db): scope organize manifest and projections per mods folder"
```

---

### Task 9: Mods-root commands

**Files:**
- Modify: `src-tauri/src/lib.rs` (new section after `get_organized`; `invoke_handler` list ~2069-2140)

- [ ] **Step 1: Add the status type and helpers** after `get_organized`:

```rust
// ── Mods folder selection (#162) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct KnownRoot {
    path: String,
    mod_count: i64,
    current: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModsRootStatus {
    path: Option<String>,
    source: modsroot::Source,
    game_reads: Option<String>,
    mismatch: bool,
    available: bool,
    /// The FS25 user dir exists but has no `mods/` yet (common on Mac until a first download).
    mods_folder_missing: bool,
    known: Vec<KnownRoot>,
    can_write_game_setting: bool,
    game_setting_problem: Option<String>,
    unresolved_override: Option<String>,
}

fn path_map(loc: &fsgame::GameLocation) -> modsroot::PathMap {
    match &loc.prefix {
        Some(p) => modsroot::PathMap::from_prefix(p),
        None => modsroot::PathMap::identity(),
    }
}

fn loose_zip_count(dir: &std::path::Path) -> i64 {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| x.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
                })
                .count() as i64
        })
        .unwrap_or(0)
}

/// Resolve the live mods folder, publish it as the trusted write root, and describe it.
fn compute_mods_root_status(conn: &rusqlite::Connection) -> ModsRootStatus {
    let loc = fsgame::locate();
    let default = loc.as_ref().map(|l| l.user_dir.join("mods"));
    let map = loc.as_ref().map(path_map).unwrap_or_default();
    let (ov, game_setting_problem) = match loc.as_ref() {
        None => (None, Some("FS25 user folder not found".to_string())),
        Some(l) => match std::fs::read_to_string(l.user_dir.join("gameSettings.xml")) {
            Err(_) => (
                None,
                Some("gameSettings.xml not found — start FS25 once to create it".to_string()),
            ),
            Ok(xml) => match modsroot::read_override(&xml) {
                Some(o) => (Some(o), None),
                None => (
                    None,
                    Some("gameSettings.xml has no modsDirectoryOverride setting".to_string()),
                ),
            },
        },
    };
    // Rows from before folders were tracked belong to the default folder: it was the only one.
    if let Some(d) = default.as_ref() {
        let _ = db::claim_unscoped(conn, &modsroot::canonical_key(d));
    }
    let choice = db::get_app_setting(conn, modsroot::CHOICE_KEY).map(PathBuf::from);
    let r = modsroot::resolve(choice, ov.as_ref(), default.clone(), &map, modsroot::same_folder);
    let available = r.path.as_ref().map(|p| p.is_dir()).unwrap_or(false);
    let mods_folder_missing = !available
        && r.source == modsroot::Source::Default
        && loc.as_ref().map(|l| l.user_dir.is_dir()).unwrap_or(false);
    modsroot::set_active(if available { r.path.clone() } else { None });

    let current_key = r.path.as_ref().map(|p| modsroot::canonical_key(p));
    let mut paths: Vec<PathBuf> =
        modsroot::parse_known(db::get_app_setting(conn, modsroot::KNOWN_KEY).as_deref())
            .into_iter()
            .map(PathBuf::from)
            .collect();
    for p in default.iter().chain(r.path.iter()) {
        if p.is_dir() && !paths.iter().any(|k| modsroot::same_folder(k, p)) {
            paths.push(p.clone());
        }
    }
    let known = paths
        .iter()
        .map(|p| {
            let key = modsroot::canonical_key(p);
            KnownRoot {
                path: p.to_string_lossy().into_owned(),
                mod_count: db::count_organized(conn, &key) + loose_zip_count(p),
                current: current_key.as_deref() == Some(key.as_str()),
            }
        })
        .collect();

    ModsRootStatus {
        path: r.path.map(|p| p.to_string_lossy().into_owned()),
        source: r.source,
        game_reads: r.game_reads.map(|p| p.to_string_lossy().into_owned()),
        mismatch: r.mismatch,
        available,
        mods_folder_missing,
        known,
        can_write_game_setting: game_setting_problem.is_none(),
        game_setting_problem,
        unresolved_override: r.unresolved_override,
    }
}

/// Point FS25's `modsDirectoryOverride` at `folder`. `guarded_xml_write` keeps the original as
/// `gameSettings.xml.bak` and swaps the new file in atomically.
fn point_game_at(folder: &std::path::Path) -> Result<(), String> {
    let loc = fsgame::locate().ok_or_else(|| "FS25 user folder not found".to_string())?;
    let gs = loc.user_dir.join("gameSettings.xml");
    let roots = [loc.user_dir.clone()];
    let xml = paths::guarded_xml_read(&roots, &gs)?;
    let out = modsroot::write_override(&xml, &path_map(&loc).to_game(folder))?;
    paths::guarded_xml_write(&roots, &gs, &out)
}
```

`PathMap` needs `Default` for `unwrap_or_default()` — it already derives it (Task 4).

- [ ] **Step 2: Add the commands:**

```rust
/// The live mods folder and everything the Settings / banners need to describe it.
#[tauri::command]
async fn mods_root_status(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Use `path` as the mods folder (remembered, added to the known list). Never moves files.
#[tauri::command]
async fn set_mods_root(app: tauri::AppHandle, path: String) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        modsroot::validate_folder(std::path::Path::new(&path))?;
        let conn = db::open(&db)?;
        let known = modsroot::add_known(
            db::get_app_setting(&conn, modsroot::KNOWN_KEY).as_deref(),
            &path,
        );
        db::set_app_setting(&conn, modsroot::KNOWN_KEY, Some(&known))?;
        db::set_app_setting(&conn, modsroot::CHOICE_KEY, Some(&path))?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forget the user's choice and follow the folder FS25 reads.
#[tauri::command]
async fn clear_mods_root(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db)?;
        db::set_app_setting(&conn, modsroot::CHOICE_KEY, None)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Point FS25 at the folder Silo currently manages.
#[tauri::command]
async fn point_game_at_mods_root(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let folder = modsroot::active().ok_or_else(|| "No mods folder selected".to_string())?;
        point_game_at(&folder)?;
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create `<user dir>/mods` when FS25 hasn't made it yet.
#[tauri::command]
async fn create_mods_folder(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let loc = fsgame::locate().ok_or_else(|| "FS25 user folder not found".to_string())?;
        std::fs::create_dir_all(loc.user_dir.join("mods")).map_err(|e| e.to_string())?;
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// True when a folder holds no mods yet (no `.zip`, no folder with a `modDesc.xml`, no
/// `archive/`), so the UI can ask "use it anyway?".
#[tauri::command]
async fn folder_looks_empty(path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::path::Path::new(&path);
        let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        let has_mod = rd.flatten().any(|e| {
            let p = e.path();
            let name = e.file_name();
            name == "archive"
                || p.extension().map(|x| x.eq_ignore_ascii_case("zip")).unwrap_or(false)
                || p.join("modDesc.xml").is_file()
        });
        Ok(!has_mod)
    })
    .await
    .map_err(|e| e.to_string())?
}
```

- [ ] **Step 3: Register them** — add to the `tauri::generate_handler![…]` list (keep it alphabetical-ish as the file does): `mods_root_status, set_mods_root, clear_mods_root, point_game_at_mods_root, create_mods_folder, folder_looks_empty,`.

- [ ] **Step 4: Gate**

Run: `cargo fmt --all`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
Expected: clean; all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "feat: mods-root commands (status, choose, clear, point FS25, create)"
```

---

### Task 10: Frontend API

**Files:**
- Modify: `src/lib/api.ts` (after `defaultModsPaths`, line ~51)

- [ ] **Step 1: Add types and wrappers:**

```ts
export type ModsRootSource = "user" | "gameOverride" | "default";

export interface KnownRoot {
  path: string;
  modCount: number;
  current: boolean;
}

/** The live mods folder (#162): where it came from, whether FS25 reads it, what else is known. */
export interface ModsRootStatus {
  path: string | null;
  source: ModsRootSource;
  gameReads: string | null;
  mismatch: boolean;
  available: boolean;
  modsFolderMissing: boolean;
  known: KnownRoot[];
  canWriteGameSetting: boolean;
  gameSettingProblem: string | null;
  unresolvedOverride: string | null;
}

export function modsRootStatus(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("mods_root_status");
}
export function setModsRoot(path: string): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("set_mods_root", { path });
}
export function clearModsRoot(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("clear_mods_root");
}
export function pointGameAtModsRoot(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("point_game_at_mods_root");
}
export function createModsFolder(): Promise<ModsRootStatus> {
  return invoke<ModsRootStatus>("create_mods_folder");
}
export function folderLooksEmpty(path: string): Promise<boolean> {
  return invoke<boolean>("folder_looks_empty", { path });
}
/** System folder picker. Null if cancelled. */
export async function pickFolder(): Promise<string | null> {
  const p = await open({ directory: true, multiple: false });
  return typeof p === "string" ? p : null;
}
```

- [ ] **Step 2: Type-check**

Run (repo root): `npm run check`
Expected: `svelte-check found 0 errors`.

- [ ] **Step 3: Commit**

```bash
git add src/lib/api.ts
git commit -m "feat(ui): mods-root API wrappers"
```

---

### Task 11: Settings panel — Mods folder section

**Files:**
- Modify: `src/lib/components/panels/SettingsPanel.svelte` (props block lines 18-61; "Mods folder" section lines 79-95)

- [ ] **Step 1: Props** — add `import type { ModsRootStatus } from "../../api";` next to the existing imports; add to the destructured props `modsStatus, onChangeModsFolder, onUseGameFolder, onSwitchModsFolder,` and to the type block:

```ts
    modsStatus: ModsRootStatus | null;
    onChangeModsFolder: () => void;
    onUseGameFolder: () => void;
    onSwitchModsFolder: (path: string) => void;
```

After the `$props()` call add:

```ts
  const others = $derived((modsStatus?.known ?? []).filter((k) => !k.current));
  function sourceLabel(s: ModsRootStatus["source"]): string {
    return s === "user"
      ? "Chosen in Silo"
      : s === "gameOverride"
        ? "Following FS25's setting"
        : "FS25 default";
  }
```

- [ ] **Step 2: Replace the "Mods folder" section** with:

```svelte
  <div class="set-section">
    <div class="set-row">
      <div class="set-label">Mods folder</div>
      {#if roots.length}
        <button class="set-link" onclick={() => openFolder(roots[0]).catch(() => {})}>
          Open ↗
        </button>
      {/if}
      <button class="set-link" disabled={!!busy || scanning} onclick={onChangeModsFolder}>
        Change…
      </button>
    </div>
    {#if modsStatus?.path}
      <div class="set-path" class:muted={!modsStatus.available}>{modsStatus.path}</div>
      <div class="set-hint">
        {sourceLabel(modsStatus.source)}{modsStatus.available ? "" : " · not available"}
      </div>
      {#if modsStatus.source === "user"}
        <button class="set-link" disabled={!!busy || scanning} onclick={onUseGameFolder}>
          Use the game's folder
        </button>
      {/if}
    {:else}
      <div class="set-path muted">No mods folder detected.</div>
    {/if}
    {#if modsStatus?.unresolvedOverride}
      <div class="set-hint">
        FS25's override points at <span class="tnum">{modsStatus.unresolvedOverride}</span>, which
        Silo can't map to a folder on this computer. Choose the folder with Change….
      </div>
    {/if}
    {#if modsStatus?.gameSettingProblem}
      <div class="set-hint">Silo can't update FS25's setting: {modsStatus.gameSettingProblem}.</div>
    {/if}
    {#if others.length}
      <div class="set-hint">Other folders Silo knows:</div>
      {#each others as k (k.path)}
        <button
          class="set-link known"
          disabled={!!busy || scanning}
          onclick={() => onSwitchModsFolder(k.path)}
        >
          {k.path} · {k.modCount} mods
        </button>
      {/each}
    {/if}
  </div>
```

Add to the component's `<style>`:

```css
  .known {
    display: block;
    text-align: left;
    word-break: break-all;
    margin-top: 2px;
  }
```

- [ ] **Step 3: Type-check** — `npm run check` will now fail in `App.svelte` (missing props); that's fixed in Task 12. Do not commit yet.

---

### Task 12: App — startup, switching, banners

**Files:**
- Modify: `src/App.svelte` (imports ~3-47; state ~118-155; startup ~1419-1436; `<SettingsPanel>` ~1584; notices ~1799; styles ~2097)

- [ ] **Step 1: Imports** — add to the `./lib/api` import list: `modsRootStatus, setModsRoot, clearModsRoot, pointGameAtModsRoot, createModsFolder, folderLooksEmpty, pickFolder, bisectSnapshotGet,` (keep `bisectSnapshotGet` once if already imported) and the type `type ModsRootStatus`.

- [ ] **Step 2: State** — next to `let roots = …`:

```ts
  let modsStatus = $state<ModsRootStatus | null>(null);
  let overrideNoticeOpen = $state(false);
```

- [ ] **Step 3: Startup** — replace

```ts
      try {
        roots = await defaultModsPaths();
      } catch (e) {
        errorMsg = String(e);
      }
```

with

```ts
      try {
        modsStatus = await modsRootStatus();
        roots = modsStatus.available && modsStatus.path ? [modsStatus.path] : [];
        if (
          modsStatus.source === "gameOverride" &&
          localStorage.getItem("silo.overrideNoticeShown") !== "1"
        ) {
          overrideNoticeOpen = true;
        }
      } catch (e) {
        errorMsg = String(e);
      }
```

Remove `defaultModsPaths` from the import list if nothing else uses it (`npm run check` reports unused imports only as hints; keep the file tidy).

- [ ] **Step 4: Switching functions** — add near `runScan`:

```ts
  /** Why switching folders isn't safe right now, or null. */
  async function switchBlockReason(): Promise<string | null> {
    if (scanning) return "Wait for the scan to finish.";
    if (busy) return `Wait for “${busy}” to finish.`;
    const snap = await bisectSnapshotGet().catch(() => null);
    if (snap) return "Finish or cancel the guided bisection first — it belongs to this folder.";
    return null;
  }

  /** Adopt a new status; if the live folder changed, reload everything folder-specific. */
  async function applyModsStatus(st: ModsRootStatus) {
    const prev = roots[0] ?? null;
    modsStatus = st;
    roots = st.available && st.path ? [st.path] : [];
    if ((roots[0] ?? null) === prev) return;
    result = null;
    mods = [];
    activeSet = new Set();
    foreignFiles = [];
    if (roots.length) {
      await runScan();
      hardlinkSupport()
        .then((ok) => (cloudSynced = !ok))
        .catch(() => {});
    }
    await loadSavegames();
  }

  async function switchModsFolder(path: string) {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    try {
      if ((await folderLooksEmpty(path)) && !confirm("This folder has no mods yet — use it anyway?"))
        return;
      const st = await setModsRoot(path);
      // Switch first: if updating FS25's setting then fails, Silo is still on the chosen folder
      // and the mismatch banner explains (the error shows too).
      await applyModsStatus(st);
      if (
        st.mismatch &&
        st.canWriteGameSetting &&
        confirm(
          `Point Farming Simulator at this folder too?\n\nFS25 currently reads:\n${st.gameReads ?? "(unknown)"}\n\n` +
            "OK updates FS25's setting (a backup of gameSettings.xml is kept). Close FS25 first — " +
            "the game rewrites that file when it exits.\nCancel keeps the change in Silo only.",
        )
      ) {
        await pointGameHere();
      }
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function changeModsFolder() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    const path = await pickFolder();
    if (path) await switchModsFolder(path);
  }

  async function useGameFolder() {
    const why = await switchBlockReason();
    if (why) {
      errorMsg = why;
      return;
    }
    try {
      await applyModsStatus(await clearModsRoot());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function pointGameHere() {
    try {
      await applyModsStatus(await pointGameAtModsRoot());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function retryModsFolder() {
    try {
      await applyModsStatus(await modsRootStatus());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  async function makeModsFolder() {
    try {
      await applyModsStatus(await createModsFolder());
    } catch (e) {
      errorMsg = String(e);
    }
  }

  function dismissOverrideNotice() {
    overrideNoticeOpen = false;
    localStorage.setItem("silo.overrideNoticeShown", "1");
  }
```

- [ ] **Step 5: Pass the new props** — on `<SettingsPanel …>` add:

```svelte
      {modsStatus}
      onChangeModsFolder={changeModsFolder}
      onUseGameFolder={useGameFolder}
      onSwitchModsFolder={switchModsFolder}
```

- [ ] **Step 6: Banners** — directly after the `{#if cloudNoticeOpen}…{/if}` block add:

```svelte
  {#if modsStatus && modsStatus.source === "user" && !modsStatus.available}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>Your mods folder isn't available:</b> <span class="tnum">{modsStatus.path}</span>.
        Nothing was scanned. Reconnect the drive, or pick another folder.
      </div>
      <button class="cn-act" onclick={retryModsFolder}>Retry</button>
      <button class="cn-act" onclick={changeModsFolder}>Choose another</button>
      <button class="cn-act" onclick={useGameFolder}>Use the game's folder</button>
    </div>
  {:else if modsStatus?.modsFolderMissing}
    <div class="cloud-notice">
      <span class="cn-ico">ⓘ</span>
      <div class="cn-text">Farming Simulator hasn't created its <b>mods</b> folder yet.</div>
      <button class="cn-act" onclick={makeModsFolder}>Create it</button>
      <button class="cn-act" onclick={changeModsFolder}>Choose a folder</button>
    </div>
  {:else if modsStatus?.mismatch}
    <div class="cloud-notice">
      <span class="cn-ico">⚠</span>
      <div class="cn-text">
        <b>FS25 is reading a different mods folder</b>
        {#if modsStatus.gameReads}(<span class="tnum">{modsStatus.gameReads}</span>){/if} — mods
        you activate here won't load in-game.
      </div>
      {#if modsStatus.canWriteGameSetting}
        <button class="cn-act" onclick={pointGameHere}>Point FS25 here</button>
      {/if}
      <button class="cn-act" onclick={useGameFolder}>Switch Silo to the game's folder</button>
    </div>
  {/if}

  {#if overrideNoticeOpen && modsStatus?.path}
    <div class="cloud-notice">
      <span class="cn-ico">ⓘ</span>
      <div class="cn-text">
        Silo now uses the mods folder FS25 is set to: <span class="tnum">{modsStatus.path}</span>.
      </div>
      <button class="cn-x" onclick={dismissOverrideNotice} title="Dismiss">✕</button>
    </div>
  {/if}
```

- [ ] **Step 7: Style for the action buttons** — next to `.cn-x` in the `<style>` block:

```css
  .cn-act {
    flex: 0 0 auto;
    border: 1px solid color-mix(in srgb, var(--info) 40%, transparent);
    background: transparent;
    color: var(--text);
    font-size: 12px;
    cursor: pointer;
    padding: 3px 8px;
    border-radius: var(--radius-sm);
  }
  .cn-act:hover {
    background: color-mix(in srgb, var(--info) 15%, transparent);
  }
```

- [ ] **Step 8: Type-check and build**

Run: `npm run build`
Expected: `svelte-check found 0 errors` and `✓ built`.

- [ ] **Step 9: Commit Tasks 11-12**

```bash
git add src/App.svelte src/lib/components/panels/SettingsPanel.svelte
git commit -m "feat(ui): choose mods folder, switch between known folders, mismatch/unavailable banners"
```

---

### Task 13: Docs, changelog, spec corrections

**Files:**
- Modify: `CHANGELOG.md`, `docs/SMOKE_TEST.md`, `docs/CROSS-PLATFORM.md`, `CLAUDE.md`, `docs/superpowers/specs/2026-10-15-mods-folder-selection-design.md`

- [ ] **Step 1: CHANGELOG** — under `## [Unreleased]` add (above any existing `### Security`):

```markdown
### Added

- **Choose your mods folder.** Settings → Mods folder → **Change…** picks any folder; if FS25 reads
  a different one, Silo offers to point the game at it too (it keeps a backup of `gameSettings.xml`).
  Silo also follows FS25's own *mods directory override* automatically, and remembers each folder's
  organized library separately, so switching back restores it. (#162)
- **Linux (Steam Proton) and Mac App Store detection.** Silo now finds the FS25 user folder inside
  the Proton prefix, in any Steam library (native or Flatpak), and in the sandboxed Mac App Store
  container. (#162, #171)
```

- [ ] **Step 2: SMOKE_TEST** — add a "Mods folder" section to `docs/SMOKE_TEST.md` with these checks:

```markdown
## Mods folder (#162)

- [ ] [game] Set FS25's mods directory override to a scratch folder (in-game or by editing
      `gameSettings.xml`: `active="true"`). Launch Silo → it uses that folder and shows the one-time
      "Silo now uses the mods folder FS25 is set to" notice.
- [ ] Settings → Mods folder → Change… → pick another folder → confirm "Point Farming Simulator at
      this folder too?" → `gameSettings.xml` now points there and `gameSettings.xml.bak` exists.
- [ ] [game] Launch FS25 → the mods from the new folder load.
- [ ] Switch back via "Other folders Silo knows" → the original folder's active set is intact.
- [ ] Rename the chosen folder while Silo is closed → launch → the "isn't available" banner shows and
      nothing is scanned; Retry after renaming it back recovers.
- [ ] Choose a folder, decline the game update → the mismatch banner shows; "Point FS25 here" fixes it.
- [ ] [linux] (tester) Silo finds the Proton prefix user folder; an override set in-game round-trips.
```

- [ ] **Step 3: CROSS-PLATFORM** — in `docs/CROSS-PLATFORM.md`, replace the Linux/Proton "(?)" rows and the macOS "(to verify)" note with the implemented candidates (Proton prefix across libraries + Flatpak; macOS App Store container then Application Support), and note `modsDirectoryOverride` is now honored with Wine path translation via `dosdevices`.

- [ ] **Step 4: CLAUDE.md** — in "Feature set", add one bullet: **Mods folder selection** — follows FS25's override, manual picker, per-folder organize state (`modsroot`); and add `modsroot` to the Rust module list.

- [ ] **Step 5: Spec corrections** (implementation findings) — in the spec:
  - "The game's value is written with backslashes" → "written with forward slashes, as FS25 writes it (`directory="C:/Temp"`)".
  - Commands: `set_mods_root(path)` (no `updateGame`); pointing the game is its own command `point_game_at_mods_root`, plus `create_mods_folder` and `folder_looks_empty`.
  - Pick-flow step 2 uses a confirm dialog (OK = update FS25's setting, Cancel = only in Silo).

- [ ] **Step 6: Commit**

```bash
git add CHANGELOG.md docs/SMOKE_TEST.md docs/CROSS-PLATFORM.md CLAUDE.md docs/superpowers/specs/2026-10-15-mods-folder-selection-design.md
git commit -m "docs: mods folder selection — changelog, smoke test, cross-platform, spec corrections"
```

---

### Task 14: Verify on the real machine, then PR

- [ ] **Step 1: Full gate** (PowerShell): `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` in `src-tauri/`; `npm run build` in the repo root. All green.

- [ ] **Step 2: Back up the live DB first** — copy `silo.db` to the scratchpad. The folder is Tauri's `app_data_dir` for identifier `com.hllmr.silo` (`tauri.conf.json`), i.e. `%APPDATA%/com.hllmr.silo/`; also copy `silo.db-wal` and `silo.db-shm` if present. The first launch runs the migration on the user's real database.

- [ ] **Step 3: Launch** `npx tauri build --debug --no-bundle`, then run `src-tauri\target\debug\silo.exe`. Check:
  - The library looks exactly as before (migration claimed the old rows for the default folder): same active set, no "foreign file" errors.
  - Settings shows the folder with "FS25 default".
  - Change… to an empty scratch folder → "no mods yet" confirm → mismatch flow → decline → banner shows → "Switch Silo to the game's folder" returns to the real library intact.
  - Do **not** accept "Point FS25 here" on the user's real install unless they ask; verify the write path with a copy of `gameSettings.xml` if needed.

- [ ] **Step 4: PR** — push `feat/mods-folder-selection`, open a PR titled `feat: choose the mods folder (follow FS25 override, per-folder state, Linux/Mac discovery)` with `Fixes #162` and `Refs #171` (#171 stays open for signing/detection items), CI green, squash-merge, delete the branch locally and remotely. Then comment on #166 that the folder groundwork is in and the archive-location spec is next, and on #162 thanking the reporter.
