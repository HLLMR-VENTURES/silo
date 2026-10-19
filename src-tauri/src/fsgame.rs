//! FS25 game-file discovery. Cross-platform; see docs/CROSS-PLATFORM.md.
//! Finds the FS25 user data dir (parent of `mods/`, `savegameN/`, `gameSettings.xml`) per OS,
//! including the Proton prefix on Linux and the sandboxed Mac App Store container.

use std::path::{Path, PathBuf};

/// Steam app id of Farming Simulator 25.
#[allow(dead_code)] // used on Linux and in tests
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
fn linux_candidates(
    home: &Path,
    read: impl Fn(&Path) -> Option<String>,
) -> Vec<(PathBuf, PathBuf)> {
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
            let prefix = lib
                .join("steamapps/compatdata")
                .join(FS25_APP_ID)
                .join("pfx");
            let user =
                prefix.join("drive_c/users/steamuser/Documents/My Games/FarmingSimulator2025");
            (user, prefix)
        })
        .collect()
}

/// Locate the FS25 user data dir for this OS (a demo profile's own, when one is active).
pub fn locate() -> Option<GameLocation> {
    if let Some(p) = crate::profile::dir() {
        return Some(GameLocation {
            user_dir: crate::profile::fs25_user_dir(p),
            prefix: None,
        });
    }
    #[cfg(target_os = "windows")]
    {
        let user_dir = dirs::document_dir()?
            .join("My Games")
            .join("FarmingSimulator2025");
        Some(GameLocation {
            user_dir,
            prefix: None,
        })
    }
    #[cfg(target_os = "macos")]
    {
        let c = mac_candidates(&dirs::home_dir()?);
        // Nothing exists yet → keep the historical default (Application Support).
        let user_dir = pick_user_dir(&c, |p| p.exists()).or_else(|| c.last().cloned())?;
        Some(GameLocation {
            user_dir,
            prefix: None,
        })
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let pairs = linux_candidates(&dirs::home_dir()?, |p| std::fs::read_to_string(p).ok());
        let dirs_only: Vec<PathBuf> = pairs.iter().map(|(u, _)| u.clone()).collect();
        let user_dir = pick_user_dir(&dirs_only, |p| p.exists())?;
        let prefix = pairs
            .into_iter()
            .find(|(u, _)| *u == user_dir)
            .map(|(_, p)| p);
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;
    use std::path::Path;

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
        assert_eq!(
            c[1],
            PathBuf::from("/Users/u/Library/Application Support/FarmingSimulator2025")
        );
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
        assert!(c.contains(&(
            PathBuf::from("/mnt/games").join(fs25),
            PathBuf::from("/mnt/games").join(pfx)
        )));
        // Flatpak root is tried even with no VDF.
        let flat = home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        assert!(c.contains(&(flat.join(fs25), flat.join(pfx))));
        // No duplicates (the native root appears both as a root and in its own VDF).
        let unique: HashSet<_> = c.iter().collect();
        assert_eq!(unique.len(), c.len());
    }
}
