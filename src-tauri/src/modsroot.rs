//! Which mods folder Silo manages (#162). Pure logic plus the one trusted "active root".

use crate::xmlconfig;
use std::path::{Path, PathBuf};

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
    let active = v
        .get(OV_ACTIVE)
        .map(|a| a.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
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
            xmlconfig::Edit {
                path: OV_ACTIVE.into(),
                value: "true".into(),
            },
            xmlconfig::Edit {
                path: OV_DIR.into(),
                value: game_path.into(),
            },
        ],
    )
}

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
            drives: drives
                .into_iter()
                .map(|(l, p)| (l.to_ascii_lowercase(), p))
                .collect(),
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
        Some(if rest.is_empty() {
            root.clone()
        } else {
            root.join(rest)
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_keys_ignore_case_verbatim_prefix_and_trailing_separators() {
        assert_eq!(
            normalize_key(r"\\?\C:\Users\Me\Mods\", true),
            r"c:\users\me\mods"
        );
        assert_eq!(normalize_key("C:/Users/me/mods", true), r"c:\users\me\mods");
        assert_eq!(
            normalize_key(r"\\?\UNC\nas\share\mods", true),
            r"\\nas\share\mods"
        );
        assert_eq!(normalize_key(r"C:\", true), r"c:\");
    }

    #[test]
    fn unix_keys_keep_case_and_drop_trailing_slash() {
        assert_eq!(normalize_key("/mnt/HDD/mods/", false), "/mnt/HDD/mods");
        assert_eq!(normalize_key("/", false), "/");
    }

    const GS: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\" ?>\n<gameSettings revision=\"15\">\n    <modsDirectoryOverride active=\"false\" directory=\"C:/Temp\"/>\n    <defaultMultiplayerPort>10823</defaultMultiplayerPort>\n</gameSettings>\n";

    #[test]
    fn reads_the_override() {
        assert_eq!(
            read_override(GS),
            Some(Override {
                active: false,
                directory: "C:/Temp".into()
            })
        );
        assert_eq!(read_override("<gameSettings><x/></gameSettings>"), None);
    }

    #[test]
    fn writes_the_override_and_preserves_everything_else() {
        let out = write_override(GS, "D:/FS25 Mods").unwrap();
        assert_eq!(
            read_override(&out),
            Some(Override {
                active: true,
                directory: "D:/FS25 Mods".into()
            })
        );
        assert!(out.starts_with('\u{feff}'), "BOM kept");
        assert!(out.contains("<defaultMultiplayerPort>10823</defaultMultiplayerPort>"));
    }

    #[test]
    fn refuses_to_write_when_the_element_is_missing() {
        assert!(write_override("<gameSettings><x/></gameSettings>", "D:/m").is_err());
    }

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
        assert_eq!(
            m.to_host(r"Z:\mnt\hdd\mods"),
            Some(PathBuf::from("/mnt/hdd/mods"))
        );
        assert_eq!(
            m.to_host("C:/users/steamuser/x"),
            Some(PathBuf::from("/home/u/pfx/drive_c/users/steamuser/x"))
        );
        assert_eq!(m.to_host("D:/"), Some(PathBuf::from("/mnt/hdd")));
        assert_eq!(
            m.to_host("Q:/nope"),
            None,
            "unknown drive letter is unresolvable, never guessed"
        );
        assert_eq!(m.to_host("relative/path"), None);
    }

    #[test]
    fn to_game_picks_the_longest_drive_match() {
        let m = wine();
        assert_eq!(m.to_game(Path::new("/mnt/hdd/mods")), "D:/mods");
        assert_eq!(
            m.to_game(Path::new("/home/u/pfx/drive_c/users/a")),
            "C:/users/a"
        );
        assert_eq!(m.to_game(Path::new("/srv/fs25")), "Z:/srv/fs25");
    }

    #[test]
    fn to_game_falls_back_to_z_without_a_root_mapping() {
        let m = PathMap::new(vec![('c', PathBuf::from("/home/u/pfx/drive_c"))]);
        assert_eq!(m.to_game(Path::new("/srv/fs25")), "Z:/srv/fs25");
    }
}
