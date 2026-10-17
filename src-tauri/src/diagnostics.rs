//! The "Copy diagnostics" bundle: the report plus the recent log, with the user's home folder
//! and user name removed, ready to paste into a public bug report.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The bundle: a `header` line, the `report`, and (when given) the recent `log`, with the home
/// folder and user name removed from all of it.
pub fn bundle(
    header: &str,
    report: &str,
    log: Option<&str>,
    home: Option<&str>,
    user: Option<&str>,
) -> String {
    let mut out = format!("{header}\n\n{report}");
    if let Some(log) = log {
        out.push_str(&format!("\n\n## Recent log\n\n```\n{log}\n```\n"));
    }
    redact(&out, home, user)
}

/// User names shorter than this only go from paths (the home folder), so a short name like
/// "bo" doesn't eat ordinary words.
const MIN_USER_LEN: usize = 3;

/// Replace the home folder with `~` and the user name with `<user>`.
///
/// The home folder goes in every spelling a path can take in a log or an error message: `\` or
/// `/` separators, `\` doubled (a path printed with `{:?}`, or JSON), any ASCII case, and only
/// as a whole folder (`/home/bob` never eats `/home/bobby`). The user name goes as a whole word
/// (ASCII case-insensitive), for names of 3+ characters; the home folder's own name counts as a
/// user name too, since on Windows it can differ from the sign-in name.
pub fn redact(text: &str, home: Option<&str>, user: Option<&str>) -> String {
    let mut out = text.to_string();
    let home = home
        .map(|h| h.trim_end_matches(['/', '\\']))
        .filter(|h| h.chars().count() >= 2 && h.contains(['/', '\\']));
    let mut names: Vec<&str> = Vec::new();
    if let Some(h) = home {
        let doubled = h.replace('\\', "\\\\");
        for variant in [doubled, h.replace('/', "\\"), h.replace('\\', "/")] {
            out = replace_ci(&out, &variant, "~", Edges::FolderEnd);
        }
        names.extend(h.rsplit(['/', '\\']).next());
    }
    names.extend(user);
    let mut seen: Vec<String> = Vec::new();
    for name in names {
        let key = name.to_ascii_lowercase();
        if name.chars().count() < MIN_USER_LEN || seen.contains(&key) {
            continue;
        }
        out = replace_ci(&out, name, "<user>", Edges::Word);
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
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Replace `needle` in `hay` (ASCII case-insensitive) with `with`, where `edges` allow. Byte
/// scan: a match of a whole UTF-8 `needle` always starts and ends on character boundaries.
fn replace_ci(hay: &str, needle: &str, with: &str, edges: Edges) -> String {
    let (h, n) = (hay.as_bytes(), needle.as_bytes());
    if n.is_empty() || n.len() > h.len() {
        return hay.to_string();
    }
    let mut out = String::with_capacity(hay.len());
    let (mut copied, mut i) = (0, 0);
    while i + n.len() <= h.len() {
        if !h[i..i + n.len()].eq_ignore_ascii_case(n) {
            i += 1;
            continue;
        }
        let end = i + n.len();
        let before = hay[..i].chars().next_back();
        let after = hay[end..].chars().next();
        let fits = match edges {
            Edges::Word => !before.is_some_and(is_word_char) && !after.is_some_and(is_word_char),
            Edges::FolderEnd => !after.is_some_and(|c| is_word_char(c) || c == '-'),
        };
        if fits {
            out.push_str(&hay[copied..i]);
            out.push_str(with);
            copied = end;
            i = end;
        } else {
            i += 1;
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
        let log = "[INFO][silo::fs] link \"C:\\Users\\hllmr\\arc\\A.zip\" -> \"C:\\Users\\hllmr\\mods\\A.zip\"\n\
                   [WARN][silo_lib] copy failed: Os { path: \"C:\\\\Users\\\\hllmr\\\\x\" } for hllmr";
        let with_log = bundle(
            "Silo 1.0.0 on windows",
            report,
            Some(log),
            Some("C:\\Users\\hllmr"),
            Some("hllmr"),
        );
        assert!(
            !with_log.to_ascii_lowercase().contains("hllmr"),
            "{with_log}"
        );
        assert!(with_log.starts_with("Silo 1.0.0 on windows\n\n- Mods folder: ~\\Documents\\mods"));
        assert!(with_log.contains("## Recent log"));
        assert!(with_log.contains("link \"~\\arc\\A.zip\" -> \"~\\mods\\A.zip\""));
        let without = bundle("h", report, None, Some("C:\\Users\\hllmr"), Some("hllmr"));
        assert!(!without.contains("Recent log") && !without.contains("hllmr"));
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
