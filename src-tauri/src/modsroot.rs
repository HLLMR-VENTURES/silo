//! Which mods folder Silo manages (#162). Pure logic plus the one trusted "active root".

use crate::xmlconfig;
use std::path::Path;

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
}
