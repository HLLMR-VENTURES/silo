//! Which mods folder Silo manages (#162). Pure logic plus the one trusted "active root".

use std::path::Path;

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
}
