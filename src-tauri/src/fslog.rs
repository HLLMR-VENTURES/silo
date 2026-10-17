//! One log line per file change Silo makes, in one shape, so a support log shows exactly what
//! happened: `<action> "<path>"` or `<action> "<from>" -> "<to>"`, target `silo::fs`.
//! Paths are quoted but otherwise verbatim (no `\\` doubling), so the diagnostics redaction
//! finds the home folder in them.
//!
//! Actions: move, link, copy, unlink, remove, keep-edited-copy, update, backup, prune,
//! restore, place, delete-moved, write-config, backup-config, keep-original, backup-save,
//! install, generate, write-note, remove-note.
//! The calls sit in the lowest primitive that makes the change (an archive move's re-point,
//! for example, shows as the `unlink` / `link` / `move` lines of the links it remakes).
//!
//! Only paths go in these lines, never file contents.

use std::path::Path;

fn line(action: &str, path: &Path, to: Option<&Path>) -> String {
    match to {
        Some(to) => format!("{action} \"{}\" -> \"{}\"", path.display(), to.display()),
        None => format!("{action} \"{}\"", path.display()),
    }
}

/// Log a change to one path (`unlink`, `remove`, `write-config`, ...).
pub fn changed(action: &str, path: &Path) {
    log::info!(target: "silo::fs", "{}", line(action, path, None));
}

/// Log a change from one path to another (`move`, `link`, `copy`, `backup`, ...).
pub fn moved(action: &str, from: &Path, to: &Path) {
    log::info!(target: "silo::fs", "{}", line(action, from, Some(to)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn lines_have_one_shape() {
        assert_eq!(
            line("unlink", Path::new("/m/FS25_A.zip"), None),
            "unlink \"/m/FS25_A.zip\""
        );
        assert_eq!(
            line(
                "move",
                Path::new("/m/FS25_A.zip"),
                Some(Path::new("/a/Misc/FS25_A.zip"))
            ),
            "move \"/m/FS25_A.zip\" -> \"/a/Misc/FS25_A.zip\""
        );
    }

    /// Paths are written as they are (no `\\` doubling on Windows), so "Copy diagnostics" finds
    /// the home folder in them.
    #[test]
    fn paths_are_written_verbatim() {
        assert_eq!(
            line("remove", Path::new(r"C:\Users\x y\mods\FS25_A.zip"), None),
            r#"remove "C:\Users\x y\mods\FS25_A.zip""#
        );
    }
}
