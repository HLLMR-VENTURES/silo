//! The "Copy diagnostics" bundle: the report plus the recent log, with the user's home folder
//! and user name removed, ready to paste into a public bug report.

use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// What the bundle carries from the log.
#[derive(Default)]
pub struct LogParts {
    /// The latest warning and error lines (`recent_warnings`).
    pub warnings: String,
    /// The end of the log (`recent_log`).
    pub recent: String,
}

/// The bundle: a `header` line, the `report`, and (when given) the recent warnings and log,
/// with the home folder and user name removed from all of it.
pub fn bundle(
    header: &str,
    report: &str,
    log: Option<&LogParts>,
    home: Option<&str>,
    user: Option<&str>,
) -> String {
    let mut out = format!("{header}\n\n{report}");
    if let Some(log) = log {
        let fenced = |s: &str| {
            if s.is_empty() {
                "None.".to_string()
            } else {
                format!("```\n{s}\n```")
            }
        };
        out.push_str(&format!(
            "\n\n## Recent warnings\n\n{}\n\n## Recent log\n\n{}\n",
            fenced(&log.warnings),
            fenced(&log.recent)
        ));
    }
    redact(&out, home, user)
}

/// The log file name without its extension (`app_log_dir()/silo.log`; rotated copies are
/// `silo_<date>_<time>.log`).
pub const LOG_NAME: &str = "silo";

/// The log files in `dir`, newest first: the live `silo.log`, then the newest rotated
/// `silo_<date>_<time>.log` (their names sort by date). Only files that exist.
pub fn log_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let live = dir.join(format!("{LOG_NAME}.log"));
    if live.is_file() {
        out.push(live);
    }
    let prefix = format!("{LOG_NAME}_");
    let newest_rotated = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(&prefix) && n.ends_with(".log"))
        .max();
    out.extend(newest_rotated.map(|n| dir.join(n)));
    out
}

/// The last `max_lines` lines across `files` (newest first, as `log_files` gives them), reading
/// at most `max_bytes` in all: the live log first, topped up from the rotated one when it's
/// short (just after a rotation). Oldest line first.
pub fn recent_log(files: &[PathBuf], max_lines: usize, max_bytes: u64) -> String {
    let mut parts: Vec<String> = Vec::new();
    let (mut lines_left, mut bytes_left) = (max_lines, max_bytes);
    for f in files {
        if lines_left == 0 || bytes_left == 0 {
            break;
        }
        let t = tail(f, lines_left, bytes_left);
        let len = std::fs::metadata(f).map(|m| m.len()).unwrap_or(0);
        bytes_left = bytes_left.saturating_sub(len.min(bytes_left));
        if !t.is_empty() {
            lines_left = lines_left.saturating_sub(t.lines().count());
            parts.push(t);
        }
    }
    parts.reverse();
    parts.join("\n")
}

/// Read at most this much of one log file when looking for warnings (a log file is at most
/// 2 MB; this only guards against a file that grew some other way).
const WARN_SCAN_BYTES: u64 = 4 * 1024 * 1024;

/// A log line at WARN or ERROR level (`[date][time][target][LEVEL] message`).
fn is_warning(line: &str) -> bool {
    line.split_once("] ")
        .is_some_and(|(head, _)| head.ends_with("[WARN") || head.ends_with("[ERROR"))
}

/// The last `max_lines` warning and error lines from the whole of each of `files` (newest
/// first), topped up from the rotated log when the live one has fewer; at most `max_bytes`,
/// dropping the oldest lines first. Oldest line first.
pub fn recent_warnings(files: &[PathBuf], max_lines: usize, max_bytes: u64) -> String {
    let mut found: Vec<String> = Vec::new(); // newest first
    for f in files {
        if found.len() >= max_lines {
            break;
        }
        let text = tail(f, usize::MAX, WARN_SCAN_BYTES);
        let mut here: Vec<String> = text
            .lines()
            .filter(|l| is_warning(l))
            .map(str::to_string)
            .collect();
        here.reverse();
        found.extend(here.into_iter().take(max_lines - found.len()));
    }
    let mut kept: Vec<String> = Vec::new(); // newest first, within the byte limit
    let mut used = 0u64;
    for line in found {
        let cost = line.len() as u64 + u64::from(!kept.is_empty());
        if used + cost > max_bytes {
            break;
        }
        used += cost;
        kept.push(line);
    }
    kept.reverse();
    kept.join("\n")
}

/// User names shorter than this only go from paths (the home folder), so a short name like
/// "bo" doesn't eat ordinary words.
const MIN_USER_LEN: usize = 3;

/// Generic account names: ordinary words and folder names (`C:\Users\Public`, a Steam Deck's
/// `deck`, `...\Steam\steamapps`) that identify no one, so only the home folder goes.
const GENERIC_NAMES: &[&str] = &[
    "user",
    "owner",
    "admin",
    "administrator",
    "guest",
    "public",
    "default",
    "deck",
    "steam",
];

/// Replace the home folder with `~` and the user name with `<user>`.
///
/// The home folder goes in every spelling a path can take in a log or an error message: `\` or
/// `/` separators, `\` doubled (a path printed with `{:?}`, or JSON), any case, and only as a
/// whole folder (`/home/bob` never eats `/home/bobby`). The user name goes as a whole word, in
/// any case, for names of 3+ characters that aren't generic account names (`GENERIC_NAMES`);
/// a shorter name only where it fills a whole folder name (`D:\Users\jo\mods`). The home
/// folder's own name counts as a user name too, since on Windows it can differ from the
/// sign-in name.
pub fn redact(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    let home = home
        .map(|h| h.trim_end_matches(['/', '\\']))
        .filter(|h| h.chars().count() >= 2 && h.contains(['/', '\\']));
    let mut names: Vec<&str> = Vec::new();
    if let Some(h) = home {
        let doubled = h.replace('\\', "\\\\");
        for variant in [doubled, h.replace('/', "\\"), h.replace('\\', "/")] {
            out = replace_cased(&out, &variant, "~", Edges::FolderEnd);
        }
        names.extend(h.rsplit(['/', '\\']).next());
    }
    names.extend(user);
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        let key = name.to_lowercase();
        if name.is_empty() || seen.contains(&key) || GENERIC_NAMES.contains(&key.as_str()) {
            continue;
        }
        let edges = if name.chars().count() < MIN_USER_LEN {
            Edges::Segment
        } else {
            Edges::Word
        };
        out = replace_cased(&out, name, "<user>", edges);
        seen.push(key);
    }
    out
}

/// What must surround a match for [`replace_ci`] to replace it.
#[derive(Clone, Copy)]
enum Edges {
    /// Not inside a word on either side.
    Word,
    /// The match ends a folder name: the next character doesn't continue it.
    FolderEnd,
    /// A whole folder name: a path separator on both sides.
    Segment,
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_separator(c: char) -> bool {
    c == '/' || c == '\\'
}

/// [`replace_ci`] for `needle` and for its upper-case form, which catches case mappings that
/// change length (`ß` -> `SS`) that per-character folding can't.
fn replace_cased(hay: &str, needle: &str, with: &str, edges: Edges) -> String {
    let out = replace_ci(hay, needle, with, edges);
    let upper = needle.to_uppercase();
    if upper.chars().count() == needle.chars().count() {
        return out;
    }
    replace_ci(&out, &upper, with, edges)
}

/// The same letter in any case (per character; `to_lowercase` covers non-ASCII letters).
fn same_letter(a: char, b: char) -> bool {
    a == b || a.to_lowercase().eq(b.to_lowercase())
}

/// Replace `needle` in `hay` (case-insensitive, per character) with `with`, where `edges` allow.
fn replace_ci(hay: &str, needle: &str, with: &str, edges: Edges) -> String {
    if needle.is_empty() {
        return hay.to_string();
    }
    // The end of a match of `needle` starting at byte `i`, if there is one.
    let match_at = |i: usize| -> Option<usize> {
        let mut rest = hay[i..].char_indices();
        let mut end = i;
        for n in needle.chars() {
            let (off, h) = rest.next()?;
            if !same_letter(n, h) {
                return None;
            }
            end = i + off + h.len_utf8();
        }
        Some(end)
    };
    let mut out = String::with_capacity(hay.len());
    let mut copied = 0;
    for (i, _) in hay.char_indices() {
        if i < copied {
            continue; // inside the previous replacement
        }
        let Some(end) = match_at(i) else {
            continue;
        };
        let before = hay[..i].chars().next_back();
        let after = hay[end..].chars().next();
        let fits = match edges {
            Edges::Word => !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char),
            Edges::FolderEnd => !after.is_some_and(|c| is_word_char(c) || c == '-'),
            Edges::Segment => before.is_some_and(is_separator) && after.is_some_and(is_separator),
        };
        if fits {
            out.push_str(&hay[copied..i]);
            out.push_str(with);
            copied = end;
        }
    }
    out.push_str(&hay[copied..]);
    out
}

/// The last `max_lines` lines of `path`, reading at most its last `max_bytes` (a line cut by
/// that limit is dropped, the trailing newline trimmed). Empty when the file is missing or
/// unreadable.
pub fn tail(path: &Path, max_lines: usize, max_bytes: u64) -> String {
    let Ok(mut f) = std::fs::File::open(path) else {
        return String::new();
    };
    let len = f.metadata().map(|m| m.len()).unwrap_or(0);
    // One byte early, to see whether the window starts at a line start.
    let start = len.saturating_sub(max_bytes.saturating_add(1));
    let mut buf = Vec::new();
    if f.seek(SeekFrom::Start(start)).is_err() || f.read_to_end(&mut buf).is_err() {
        return String::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let text = if len > max_bytes {
        // The first byte is the one before the window: drop through the first line break.
        text.split_once('\n').map(|(_, rest)| rest).unwrap_or("")
    } else {
        &text
    };
    let lines: Vec<&str> = text
        .trim_end_matches(['\n', '\r'])
        .split('\n')
        .map(|l| l.trim_end_matches('\r'))
        .collect();
    if lines == [""] {
        return String::new();
    }
    lines[lines.len().saturating_sub(max_lines)..].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removes_the_home_folder_and_the_user_name() {
        let text = "Mods: C:\\Users\\hllmr\\Documents\\My Games\\FarmingSimulator2025\\mods\n\
                    also c:/users/HLLMR/OneDrive\n\
                    signed in as hllmr today; hllmrish stays";
        assert_eq!(
            redact(text, Some("C:\\Users\\hllmr"), Some("hllmr")),
            "Mods: ~\\Documents\\My Games\\FarmingSimulator2025\\mods\n\
             also ~/OneDrive\n\
             signed in as <user> today; hllmrish stays"
        );
    }

    #[test]
    fn short_user_names_only_redact_paths() {
        assert_eq!(
            redact("/home/bo/mods and bob", Some("/home/bo"), Some("bo")),
            "~/mods and bob"
        );
    }

    /// Error strings often carry a path printed with `{:?}` (or JSON), where every `\` is
    /// doubled; the home folder must go from those too.
    #[test]
    fn removes_the_home_folder_from_escaped_and_verbatim_paths() {
        let text = r#"open "C:\\Users\\hllmr\\mods\\FS25_A.zip": denied; at \\?\C:\Users\hllmr\x"#;
        assert_eq!(
            redact(text, Some(r"C:\Users\hllmr"), Some("hllmr")),
            r#"open "~\\mods\\FS25_A.zip": denied; at \\?\~\x"#
        );
    }

    /// The home folder's own name is a user name too (on Windows it can differ from USERNAME),
    /// and the home folder only matches as a whole folder name.
    #[test]
    fn the_home_folder_name_counts_and_only_whole_folders_match() {
        assert_eq!(
            redact(
                "/home/dave/mods, /home/davey/mods, dave's D:\\dave\\mods",
                Some("/home/dave/"),
                Some("dhellmer")
            ),
            "~/mods, /home/davey/mods, <user>'s D:\\<user>\\mods"
        );
    }

    #[test]
    fn a_root_or_empty_home_is_ignored() {
        assert_eq!(redact("C:\\x and /y", Some("C:\\"), None), "C:\\x and /y");
        assert_eq!(redact("/y", Some("/"), Some("")), "/y");
    }

    /// Generic account names (a Steam Deck's "deck", Windows' "User") are ordinary words and
    /// folder names (`C:\Users\Public`, `...\Steam\steamapps`) and identify no one: only the home
    /// folder goes.
    #[test]
    fn generic_account_names_only_redact_paths() {
        for name in [
            "user",
            "Owner",
            "admin",
            "Administrator",
            "guest",
            "public",
            "default",
            "deck",
            "steam",
        ] {
            let home = format!("/home/{name}");
            let text = format!("{home}/mods; the {name} folder; /mnt/{name}/x");
            assert_eq!(
                redact(&text, Some(&home), Some(name)),
                format!("~/mods; the {name} folder; /mnt/{name}/x"),
                "{name}"
            );
        }
    }

    /// Case is folded for any letter, not just ASCII.
    #[test]
    fn non_ascii_case_is_folded() {
        assert_eq!(
            redact(
                "C:\\USERS\\JÖRG MÜLLER\\x and c:\\users\\jörg müller\\y by JÖRG MÜLLER",
                Some("C:\\Users\\Jörg Müller"),
                Some("Jörg Müller")
            ),
            "~\\x and ~\\y by <user>"
        );
        // A case mapping that changes length (ß -> SS) is tried too.
        assert_eq!(redact("/HOME/GROSS/x", Some("/home/groß"), None), "~/x");
    }

    /// A short name goes when it fills a whole folder name, in any path, but stays as a word.
    #[test]
    fn short_names_go_from_whole_path_segments() {
        assert_eq!(
            redact(
                "D:\\Users\\jo\\mods, D:\\\\Users\\\\JO\\\\mods, /mnt/jo/x; jo said /a/jos/b",
                Some("C:\\Users\\jo"),
                Some("jo")
            ),
            "D:\\Users\\<user>\\mods, D:\\\\Users\\\\<user>\\\\mods, /mnt/<user>/x; jo said /a/jos/b"
        );
    }

    #[test]
    fn non_ascii_text_and_names_are_safe() {
        assert_eq!(
            redact(
                "Fahrzeug é José: /home/josé/x",
                Some("/home/josé"),
                Some("José")
            ),
            "Fahrzeug é <user>: ~/x"
        );
    }

    /// Both halves of the bundle are redacted, including paths inside error strings.
    #[test]
    fn the_bundle_redacts_the_report_and_the_log() {
        let report = "- Mods folder: C:\\Users\\hllmr\\Documents\\mods\n\
                      - Corrupt: FS25_A — couldn't open C:\\Users\\hllmr\\mods\\FS25_A.zip: denied";
        let warn =
            "[WARN][silo_lib] copy failed: Os { path: \"C:\\\\Users\\\\hllmr\\\\x\" } for hllmr";
        let recent = format!(
            "[INFO][silo::fs] link \"C:\\Users\\hllmr\\arc\\A.zip\" -> \"C:\\Users\\hllmr\\mods\\A.zip\"\n{warn}"
        );
        let parts = LogParts {
            warnings: warn.into(),
            recent,
        };
        let with_log = bundle(
            "Silo 1.0.0 on windows",
            report,
            Some(&parts),
            Some("C:\\Users\\hllmr"),
            Some("hllmr"),
        );
        assert!(
            !with_log.to_ascii_lowercase().contains("hllmr"),
            "{with_log}"
        );
        assert!(with_log.starts_with("Silo 1.0.0 on windows\n\n- Mods folder: ~\\Documents\\mods"));
        let w = with_log
            .find("## Recent warnings")
            .expect("warnings section");
        let r = with_log.find("## Recent log").expect("log section");
        assert!(w < r, "warnings come first");
        assert!(with_log.contains("link \"~\\arc\\A.zip\" -> \"~\\mods\\A.zip\""));
        let none = LogParts::default();
        let quiet = bundle("h", "r", Some(&none), None, None);
        assert!(quiet.contains("## Recent warnings\n\nNone."), "{quiet}");
        let without = bundle("h", report, None, Some("C:\\Users\\hllmr"), Some("hllmr"));
        assert!(!without.contains("Recent log") && !without.contains("hllmr"));
    }

    fn log_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("silo_logs_{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn lines(prefix: &str, range: std::ops::Range<usize>) -> String {
        range.map(|i| format!("{prefix} {i}\n")).collect()
    }

    /// The live log, then the newest rotated one; anything else in the folder is ignored.
    #[test]
    fn log_files_are_the_live_one_then_the_newest_rotated_one() {
        let dir = log_dir("files");
        for f in [
            "silo_2026-10-01_10-00-00.log",
            "silo_2026-10-16_09-00-00.log",
            "silo_2026-10-16_09-00-00.log.bak",
            "silo.log",
            "other.log",
        ] {
            std::fs::write(dir.join(f), "x\n").unwrap();
        }
        let names: Vec<String> = log_files(&dir)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["silo.log", "silo_2026-10-16_09-00-00.log"]);
        assert!(log_files(&dir.join("missing")).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Just after a rotation the live log is short: the recent log is topped up from the
    /// rotated one, oldest first, within the line and byte limits.
    #[test]
    fn the_recent_log_is_topped_up_after_a_rotation() {
        let dir = log_dir("topup");
        let live = dir.join("silo.log");
        let old = dir.join("silo_2026-10-16_09-00-00.log");
        std::fs::write(&live, lines("new", 0..2)).unwrap();
        std::fs::write(&old, lines("old", 0..10)).unwrap();
        let files = [live.clone(), old.clone()];
        assert_eq!(
            recent_log(&files, 5, 64 * 1024),
            "old 7\nold 8\nold 9\nnew 0\nnew 1"
        );
        // A full live log needs nothing older.
        assert_eq!(recent_log(&files, 2, 64 * 1024), "new 0\nnew 1");
        // The byte limit covers both files: 12 bytes is "new 0\nnew 1\n" only.
        assert_eq!(recent_log(&files, 5, 12), "new 0\nnew 1");
        assert_eq!(recent_log(&[], 5, 1024), "");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Warnings and errors from the whole live log (not just its tail), topped up from the
    /// rotated one, newest last, within the line and byte limits.
    #[test]
    fn recent_warnings_come_from_the_whole_log() {
        let dir = log_dir("warn");
        let live = dir.join("silo.log");
        let old = dir.join("silo_2026-10-16_09-00-00.log");
        let mut body = String::from("[d][t][silo_lib][WARN] early warning\n");
        body.push_str(&lines("[d][t][silo_lib][INFO] busy", 0..2000));
        body.push_str("[d][t][silo_lib][ERROR] late error\n");
        body.push_str("[d][t][silo_lib][INFO] mentions [WARN] in passing\n");
        std::fs::write(&live, body).unwrap();
        std::fs::write(
            &old,
            "[d][t][silo_lib][WARN] old one\n[d][t][silo_lib][WARN] old two\n",
        )
        .unwrap();
        let files = [live.clone(), old.clone()];
        assert_eq!(
            recent_warnings(&files, 3, 16 * 1024),
            "[d][t][silo_lib][WARN] old two\n[d][t][silo_lib][WARN] early warning\n[d][t][silo_lib][ERROR] late error"
        );
        assert_eq!(recent_warnings(&files, 50, 16 * 1024).lines().count(), 4);
        // Over the byte limit, the oldest lines go first.
        assert_eq!(
            recent_warnings(&files, 50, 40),
            "[d][t][silo_lib][ERROR] late error"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_keeps_the_last_lines() {
        let dir = std::env::temp_dir().join(format!("silo_tail_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("silo.log");
        let body: String = (0..500).map(|i| format!("line {i}\n")).collect();
        std::fs::write(&f, body).unwrap();
        let t = tail(&f, 200, 64 * 1024);
        assert!(t.ends_with("line 499") && t.starts_with("line 300"), "{t}");
        assert_eq!(tail(&dir.join("missing.log"), 200, 1024), "");
        // Reading only the last bytes drops the partial first line.
        let t = tail(&f, 200, 20);
        assert_eq!(t, "line 498\nline 499");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
