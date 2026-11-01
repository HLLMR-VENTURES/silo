//! Crash / log triage — parse FS25's `log.txt` and tell the user WHICH mod is
//! throwing errors, separating genuine breakage from harmless noise.
//!
//! No FS25 tool does this; the community's standard advice is a manual "disable half,
//! relaunch, repeat" bisection. Step one is attribution: the engine writes the mod's
//! name (or its `mods/FS25_Xxx/` path) right next to most warnings/errors, so a lot of
//! the time we can name the culprit outright with no bisection at all.
//!
//! Everything here is PURE (string in, structured out) so it unit-tests against real
//! log fixtures with no game and no filesystem. Line shapes were taken from a real
//! FS25 log (2026-07): an optional `YYYY-MM-DD HH:MM:SS.mmm ` timestamp, then optional
//! indentation, then the message. The mod inventory line is
//! `Available mod: (Hash: <md5>) (Version: <v>) FS25_Name`, and a healthy run ends with
//! `Application quit` / `#End.` — whose ABSENCE means the run died (the crash signal).

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Category {
    /// Missing/duplicate localization — cosmetic, never a crash cause.
    L10n,
    /// Lua runtime error or call stack — the real "this mod is broken" signal.
    Lua,
    /// Failed to load/parse a mod asset (xml/i3d/dds).
    Load,
    /// Out of memory.
    Memory,
    /// A referenced thing (texture, i3d node, fillType…) is missing.
    Reference,
    Other,
}

impl Category {
    /// Cosmetic categories that shouldn't alarm the user. Errors are never benign.
    fn benign_as_warning(self) -> bool {
        matches!(self, Category::L10n)
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    /// Tech name of the mod at fault, or None when we can't attribute it.
    pub mod_name: Option<String>,
    pub severity: Severity,
    pub category: Category,
    pub message: String,
    pub line: usize,
    /// True for cosmetic noise (e.g. missing l10n) the user can safely ignore.
    pub benign: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModHealth {
    pub mod_name: String,
    pub errors: usize,
    pub warnings: usize,
    /// Of the warnings, how many are cosmetic — so the UI can say "12 warnings (all cosmetic)".
    pub benign: usize,
    /// One representative message, for the collapsed row.
    pub sample: String,
    pub sample_line: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogReport {
    pub engine_version: Option<String>,
    pub mod_count: usize,
    /// The run ended normally (saw the quit/#End markers).
    pub clean_exit: bool,
    /// !clean_exit — the log stops mid-run, i.e. the game likely crashed.
    pub crashed: bool,
    /// Per-mod rollup, worst first (real errors above cosmetic-only).
    pub mods: Vec<ModHealth>,
    /// Findings that couldn't be tied to any mod (engine/base-game/unknown).
    pub unattributed: usize,
    pub total_findings: usize,
    /// The game ran out of ground-pile (height) type slots: the cap it reported, e.g. 63 (#339).
    pub ground_pile_cap: Option<u32>,
    /// What turned itself off for lack of ground-pile slots, as the game names it ("Straw
    /// Harvest", "Pumps N' Hoses").
    pub ground_pile_disabled: Vec<String>,
    /// The game hit its fill-type limit: the limit it reported, e.g. 255 (#341).
    pub fill_type_cap: Option<u32>,
    /// The fill types it skipped because of that limit, in log order.
    pub dropped_fill_types: Vec<String>,
    /// The log was too big to read whole: only its start and its last [`LOG_TAIL_BYTES`] were
    /// read, so findings from the middle are missing (#351).
    pub truncated: bool,
}

/// What a DLC names when it turns itself off for lack of ground-pile slots: "PnH: unable to
/// register the required density map height types, the limit of the game is reached. Pumps N'
/// Hoses is disabled and the savegame will be closed." (measured in-game, FS25 1.25).
fn ground_pile_disabled(line: &str) -> Option<String> {
    if !line.contains("unable to register the required density map height types") {
        return None;
    }
    let rest = &line[line.find("is reached.")? + "is reached.".len()..];
    let name = rest[..rest.find(" is disabled")?].trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// The limit and the skipped name in "FillTypeManager.addFillType too many fill types. Only 255
/// fill types are supported. Ignoring 'LIQUID_UREA'." (#341).
fn dropped_fill_type(line: &str) -> Option<(u32, String)> {
    let rest =
        &line[line.find("too many fill types. Only ")? + "too many fill types. Only ".len()..];
    let cap = rest.split_whitespace().next()?.parse().ok()?;
    let name = rest.split("Ignoring '").nth(1)?.split('\'').next()?.trim();
    (!name.is_empty()).then(|| (cap, name.to_string()))
}

/// The cap in "maximum number (63) of height types already registered", the line the game
/// logs when mods ask for more ground-pile slots than the map has (#339).
fn ground_pile_cap(line: &str) -> Option<u32> {
    let l = line.to_lowercase();
    let at = l.find("maximum number (")? + "maximum number (".len();
    let rest = &l[at..];
    let close = rest.find(')')?;
    rest[close..].contains("of height types").then_some(())?;
    rest[..close].trim().parse().ok()
}

/// Strip an optional leading `YYYY-MM-DD HH:MM:SS.mmm ` timestamp, then any indentation.
fn strip_prefix(line: &str) -> &str {
    let bytes = line.as_bytes();
    // A timestamp is exactly "dddd-dd-dd dd:dd:dd.ddd " (24 chars). Cheap shape check.
    let rest = if bytes.len() > 24
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes[10] == b' '
        && bytes[13] == b':'
        && bytes[16] == b':'
        && bytes[19] == b'.'
        && bytes[23] == b' '
        && bytes[..4].iter().all(u8::is_ascii_digit)
    {
        &line[24..]
    } else {
        line
    };
    rest.trim_start()
}

/// Pull a mod tech name out of a `mods/FS25_Xxx/…` path fragment if present.
fn mod_from_path(s: &str) -> Option<String> {
    let idx = s.find("mods/")?;
    let after = &s[idx + 5..];
    let name: String = after
        .chars()
        .take_while(|&c| c != '/' && c != '\\')
        .collect();
    if name.starts_with("FS25_") || name.starts_with("FS22_") {
        Some(name)
    } else {
        None
    }
}

/// Pull a mod tech name out of an `in mod 'FS25_Xxx'` clause if present.
fn mod_from_clause(s: &str) -> Option<String> {
    let idx = s.find("in mod '")?;
    let after = &s[idx + 8..];
    let name: String = after.chars().take_while(|&c| c != '\'').collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn classify(msg: &str) -> Category {
    let m = msg.to_lowercase();
    if m.contains("l10n") {
        Category::L10n
    } else if m.contains("lua") || m.contains("call stack") || m.contains("callstack") {
        Category::Lua
    } else if m.contains("out of memory") || m.contains("oom") {
        Category::Memory
    } else if m.contains("could not")
        || m.contains("failed to load")
        || m.contains("failed to open")
    {
        Category::Load
    } else if m.contains("not found") || m.contains("missing") || m.contains("unknown") {
        Category::Reference
    } else {
        Category::Other
    }
}

fn severity_of(s: &str) -> Option<Severity> {
    if s.starts_with("Error") || s.starts_with("Fatal") {
        Some(Severity::Error)
    } else if s.starts_with("Warning") {
        Some(Severity::Warning)
    } else {
        None
    }
}

/// A continuation of the finding above it — a Lua stack frame or indented detail — as
/// opposed to a new record. This is how a multi-line Lua error's `mods/FS25_X/…` path
/// (which sits on a frame line, not the `Error:` line) gets folded in for attribution.
fn is_continuation(raw: &str, stripped: &str) -> bool {
    if severity_of(stripped).is_some()
        || stripped.starts_with("Available mod:")
        || stripped.starts_with("Application quit")
        || stripped.starts_with("#End.")
        || stripped.is_empty()
    {
        return false;
    }
    // Indented in the raw line, or clearly a stack frame / path detail.
    raw.starts_with(' ')
        || raw.starts_with('\t')
        || stripped.contains(".lua")
        || stripped.contains("mods/")
        || stripped.contains("call stack")
}

/// How much of a runaway `log.txt` Silo reads (#351). A mod that throws an error every frame can
/// grow the log to gigabytes, and the middle is then the same error over and over. The start
/// holds the engine version and the `Available mod:` inventory; the end holds the crash or the
/// clean exit.
pub const LOG_HEAD_BYTES: u64 = 4 * 1024 * 1024;
pub const LOG_TAIL_BYTES: u64 = 64 * 1024 * 1024;

/// Read and parse a log, keeping at most `head + tail` bytes of it in memory. A bigger log gives
/// its first `head` and last `tail` bytes, cut to whole lines; the lines in between are counted
/// (in fixed-size chunks, never kept), so line numbers stay true, and the report is marked
/// truncated.
pub fn scan<R: std::io::Read + std::io::Seek>(
    mut r: R,
    head: u64,
    tail: u64,
) -> std::io::Result<LogReport> {
    use std::io::{Read, SeekFrom};
    let len = r.seek(SeekFrom::End(0))?;
    r.seek(SeekFrom::Start(0))?;
    let limit = head.saturating_add(tail);
    if len <= limit {
        let mut bytes = Vec::new();
        // `take` keeps a log that grows while we read within the limit.
        r.take(limit).read_to_end(&mut bytes)?;
        return Ok(parse(&String::from_utf8_lossy(&bytes)));
    }
    let mut head_bytes = Vec::new();
    r.by_ref().take(head).read_to_end(&mut head_bytes)?;
    // Whole lines only: a cut line's rest is in the skipped middle.
    let keep = head_bytes
        .iter()
        .rposition(|&b| b == b'\n')
        .map_or(0, |i| i + 1);
    head_bytes.truncate(keep);
    // The tail starts one byte early, so a line that starts exactly at the cut is kept whole.
    let tail_from = len - tail - 1;
    let pos = r.stream_position()?;
    let mut newlines = count_newlines(r.by_ref().take(tail_from.saturating_sub(pos)))?;
    r.seek(SeekFrom::Start(tail_from))?;
    let mut tail_bytes = Vec::new();
    r.take(tail + 1).read_to_end(&mut tail_bytes)?;
    // Drop the partial line the tail starts in; its newline counts with the middle's.
    let tail_text = match tail_bytes.iter().position(|&b| b == b'\n') {
        Some(i) => {
            newlines += 1;
            String::from_utf8_lossy(&tail_bytes[i + 1..]).into_owned()
        }
        None => String::new(),
    };
    let head_text = String::from_utf8_lossy(&head_bytes);
    let first_tail_line = head_text.lines().count() + newlines + 1;
    let lines = head_text
        .lines()
        .enumerate()
        .map(|(i, l)| (i + 1, l))
        .chain(
            tail_text
                .lines()
                .enumerate()
                .map(|(i, l)| (first_tail_line + i, l)),
        );
    let mut report = parse_lines(lines);
    report.truncated = true;
    Ok(report)
}

/// The `\n` bytes in `r`, read in fixed-size chunks.
fn count_newlines(mut r: impl std::io::Read) -> std::io::Result<usize> {
    let mut buf = vec![0u8; 1024 * 1024];
    let mut n = 0;
    loop {
        match r.read(&mut buf) {
            Ok(0) => return Ok(n),
            Ok(got) => n += buf[..got].iter().filter(|&&b| b == b'\n').count(),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
}

/// Parse a full `log.txt` into a triage report.
pub fn parse(log: &str) -> LogReport {
    parse_lines(log.lines().enumerate().map(|(i, l)| (i + 1, l)))
}

/// Parse a log's lines, each with its 1-based line number, into a triage report. Lines are
/// streamed, never collected: only the findings are kept. A line continues the finding above it
/// only when its number follows on, so a gap (the skipped middle of a huge log) ends a block.
fn parse_lines<'a>(lines: impl Iterator<Item = (usize, &'a str)>) -> LogReport {
    use std::collections::HashSet;
    let mut engine_version: Option<String> = None;
    let mut mods_seen: HashSet<String> = HashSet::new();
    let mut clean_exit = false;
    let mut ground_pile: Option<u32> = None;
    let mut fill_cap: Option<u32> = None;
    let (mut dropped, mut dropped_seen) = (Vec::new(), HashSet::new());
    let (mut disabled, mut disabled_seen) = (Vec::new(), HashSet::new());

    // Dedup findings: the engine writes a scan pass AND a load pass, repeating the same
    // warnings. Key on (mod, severity, message) so a mod that logs one thing twice per
    // run isn't reported as twice as broken.
    let mut seen: HashSet<(Option<String>, Severity, String)> = HashSet::new();
    let mut findings: Vec<Finding> = Vec::new();

    let mut lines = lines.peekable();
    while let Some((line_no, raw)) = lines.next() {
        let s = strip_prefix(raw);

        if ground_pile.is_none() {
            ground_pile = ground_pile_cap(s);
        }
        if let Some(name) = ground_pile_disabled(s) {
            if disabled_seen.insert(name.clone()) {
                disabled.push(name);
            }
        }
        if let Some((cap, name)) = dropped_fill_type(s) {
            fill_cap = Some(cap);
            if dropped_seen.insert(name.clone()) {
                dropped.push(name);
            }
        }
        if engine_version.is_none() {
            if let Some(rest) = s.strip_prefix("GIANTS Engine Runtime ") {
                engine_version = Some(rest.trim().to_string());
            }
        }
        if let Some(rest) = s.strip_prefix("Available mod:") {
            if let Some(name) = rest.split_whitespace().last() {
                mods_seen.insert(name.to_string());
            }
            continue;
        }
        if s.starts_with("Application quit") || s.starts_with("#End.") {
            clean_exit = true;
            continue;
        }

        let Some(severity) = severity_of(s) else {
            continue;
        };

        // Fold continuation lines (bounded) into one block, so a Lua error and its stack
        // frames are attributed and reported together. A line that doesn't continue it stays
        // in the iterator for the next round.
        let mut block = s.to_string();
        let mut last = line_no;
        while last - line_no < 12 {
            let Some(&(next_no, next)) = lines.peek() else {
                break;
            };
            let cs = strip_prefix(next);
            if next_no != last + 1 || !is_continuation(next, cs) {
                break;
            }
            block.push('\n');
            block.push_str(cs);
            last = next_no;
            lines.next();
        }

        // Attribute ONLY via strong signals (path, then clause). No "last loaded mod"
        // guess — blaming the wrong mod is worse than admitting we don't know.
        let mod_name = mod_from_path(&block).or_else(|| mod_from_clause(&block));
        let category = classify(&block);
        let benign = severity == Severity::Warning && category.benign_as_warning();

        let key = (mod_name.clone(), severity, block.clone());
        if !seen.insert(key) {
            continue; // exact repeat from the other pass
        }
        findings.push(Finding {
            mod_name,
            severity,
            category,
            message: block,
            line: line_no,
            benign,
        });
    }
    let mod_count = mods_seen.len();

    // Roll up per mod.
    use std::collections::BTreeMap;
    let mut roll: BTreeMap<String, ModHealth> = BTreeMap::new();
    let mut unattributed = 0usize;
    for f in &findings {
        let Some(name) = &f.mod_name else {
            unattributed += 1;
            continue;
        };
        let h = roll.entry(name.clone()).or_insert_with(|| ModHealth {
            mod_name: name.clone(),
            errors: 0,
            warnings: 0,
            benign: 0,
            sample: f.message.clone(),
            sample_line: f.line,
        });
        match f.severity {
            Severity::Error => h.errors += 1,
            Severity::Warning => h.warnings += 1,
        }
        if f.benign {
            h.benign += 1;
        }
        // Prefer a non-benign sample so the collapsed row shows the real problem.
        if f.severity == Severity::Error && h.errors == 1 {
            h.sample = f.message.clone();
            h.sample_line = f.line;
        }
    }

    let mut mods: Vec<ModHealth> = roll.into_values().collect();
    // Worst first: most errors, then most (real) warnings, then name for stability.
    mods.sort_by(|a, b| {
        b.errors
            .cmp(&a.errors)
            .then((b.warnings - b.benign).cmp(&(a.warnings - a.benign)))
            .then(b.warnings.cmp(&a.warnings))
            .then(a.mod_name.cmp(&b.mod_name))
    });

    LogReport {
        engine_version,
        mod_count,
        clean_exit,
        crashed: !clean_exit,
        mods,
        unattributed,
        total_findings: findings.len(),
        ground_pile_cap: ground_pile,
        ground_pile_disabled: disabled,
        fill_type_cap: fill_cap,
        dropped_fill_types: dropped,
        truncated: false,
    }
}

/// The `app_setting` key holding the log modification time (epoch ms) Silo last looked at, so a
/// crash is announced once and not again on every start.
pub const LAST_SEEN_KEY: &str = "log_last_seen_ms";

/// A cheap look at `log.txt` for crash detection: no read, just a stat. The frontend re-reads
/// the log (`scan_log`) only when `modified_ms` moved past `last_seen_ms` and the game is closed
/// (a running game's log has no exit marker yet, so it would look like a crash).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogStatus {
    /// Where the log should be, or None when the FS25 user folder wasn't found.
    pub path: Option<String>,
    /// The log's modification time in epoch ms, or None when there's no log yet.
    pub modified_ms: Option<i64>,
    /// FS25 (or its dedicated server) is running right now.
    pub game_running: bool,
    /// The modification time Silo last looked at (`LAST_SEEN_KEY`), if any.
    pub last_seen_ms: Option<i64>,
}

/// A file's modification time in epoch milliseconds, or None if it's missing or unreadable.
pub fn modified_ms(path: &std::path::Path) -> Option<i64> {
    let t = std::fs::metadata(path).ok()?.modified().ok()?;
    let ms = t.duration_since(std::time::UNIX_EPOCH).ok()?.as_millis();
    i64::try_from(ms).ok()
}

/// Parse a stored last-seen value; anything that isn't a number counts as never seen.
pub fn parse_last_seen(v: Option<&str>) -> Option<i64> {
    v.and_then(|s| s.trim().parse::<i64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modified_ms_reads_mtime_and_none_when_missing() {
        let dir = std::env::temp_dir().join(format!("silo_logstatus_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("log.txt");
        std::fs::write(&f, "x").unwrap();
        let ms = modified_ms(&f).expect("mtime of a fresh file");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        assert!(
            (now - ms).abs() < 60_000,
            "mtime {ms} should be close to now {now}"
        );
        assert_eq!(modified_ms(&dir.join("missing.txt")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn last_seen_parses_numbers_only() {
        assert_eq!(
            parse_last_seen(Some("1760000000000")),
            Some(1_760_000_000_000)
        );
        assert_eq!(parse_last_seen(Some(" 42 ")), Some(42));
        assert_eq!(parse_last_seen(Some("soon")), None);
        assert_eq!(parse_last_seen(None), None);
    }

    // Mirrors the real healthy FS25 log shapes observed 2026-07: timestamped + plain
    // Available-mod lines, both l10n warning styles (path-named and `in mod '...'`),
    // clean exit markers.
    const CLEAN: &str = "GIANTS Engine Runtime 10.0.0 (46522) 64bit Steam (Build Date: May 28 2026)
Available mod: (Hash: abc) (Version: 1.0.0.0) FS25_FarmOperationsDashboard
Warning: Duplicate l10n entry 'ui_balance' in 'C:/Users/x/mods/FS25_FarmOperationsDashboard/l10n/translation_en.xml'. Ignoring this definition.
Available mod: (Hash: def) (Version: 1.0.0.0) FS25_actionHonk
2026-07-11 21:19:53.981   Warning: Missing l10n 'input_ACTION_HONK_CHANGE_1' in mod 'FS25_actionHonk'
2026-07-11 21:19:55.954 Application quit
2026-07-11 21:19:55.986 #End.";

    // Synthetic crashy shapes from known FS conventions (real crash-log fixture TBD):
    // a Lua error naming a mod via its script path, then an abrupt end (no quit marker).
    const CRASHY: &str = "GIANTS Engine Runtime 10.0.0 (46522) 64bit
Available mod: (Hash: abc) (Version: 1.0.0.0) FS25_SomeVehicle
Error: Running LUA method 'onUpdate'.
dataS/scripts/../../mods/FS25_SomeVehicle/scripts/SomeVehicle.lua(142) : attempt to index nil value
Error: Out of memory";

    #[test]
    fn clean_run_is_clean_and_noise_is_benign() {
        let r = parse(CLEAN);
        assert_eq!(
            r.engine_version.as_deref(),
            Some("10.0.0 (46522) 64bit Steam (Build Date: May 28 2026)")
        );
        assert_eq!(r.mod_count, 2);
        assert!(r.clean_exit);
        assert!(!r.crashed);
        // Both mods appear, both with only cosmetic l10n warnings.
        let dash = r
            .mods
            .iter()
            .find(|m| m.mod_name == "FS25_FarmOperationsDashboard")
            .unwrap();
        assert_eq!(dash.errors, 0);
        assert_eq!(dash.warnings, 1);
        assert_eq!(dash.benign, 1);
        let honk = r
            .mods
            .iter()
            .find(|m| m.mod_name == "FS25_actionHonk")
            .unwrap();
        assert_eq!(honk.warnings, 1);
        assert_eq!(honk.benign, 1); // attributed via `in mod '...'`
    }

    #[test]
    fn crash_is_flagged_and_lua_error_attributed() {
        let r = parse(CRASHY);
        assert!(r.crashed, "no quit marker => crashed");
        assert!(!r.clean_exit);
        // The Lua error is attributed via the mods/ path folded in from its stack-frame
        // continuation line — not the "Error:" line itself.
        let v = r
            .mods
            .iter()
            .find(|m| m.mod_name == "FS25_SomeVehicle")
            .unwrap();
        assert_eq!(v.errors, 1);
        assert_eq!(v.benign, 0);
        // "Error: Out of memory" names no mod, so it's honestly unattributed rather than
        // blamed on whatever loaded last.
        assert_eq!(r.unattributed, 1);
        assert_eq!(r.total_findings, 2);
    }

    #[test]
    fn worst_mod_sorts_first() {
        let r = parse(CRASHY);
        // The mod with a real error must outrank any cosmetic-only mod.
        assert_eq!(r.mods[0].mod_name, "FS25_SomeVehicle");
    }

    #[test]
    fn strip_prefix_handles_timestamp_and_indent() {
        assert_eq!(
            strip_prefix("2026-07-11 21:19:53.981   Warning: x"),
            "Warning: x"
        );
        assert_eq!(strip_prefix("  Warning: y"), "Warning: y");
        assert_eq!(strip_prefix("Warning: z"), "Warning: z");
    }

    #[test]
    fn attribution_helpers() {
        assert_eq!(
            mod_from_path("in 'C:/x/mods/FS25_Foo/l10n/a.xml'"),
            Some("FS25_Foo".into())
        );
        assert_eq!(
            mod_from_clause("Missing l10n 'k' in mod 'FS25_Bar'"),
            Some("FS25_Bar".into())
        );
        assert_eq!(mod_from_path("no mod here"), None);
    }

    /// #339: the slot-cap line, wherever the game puts it, with its number.
    #[test]
    fn finds_the_ground_pile_cap() {
        assert_eq!(
            ground_pile_cap("Error: addDensityMapHeightType \"GRAVEL_FINE\": maximum number (63) of height types already registered. Adjust densityMapHeightTypes#numChannels to allow for more"),
            Some(63)
        );
        assert_eq!(
            ground_pile_cap("Warning: Maximum number (127) of height types already registered!"),
            Some(127)
        );
        assert_eq!(
            ground_pile_cap("Maximum number of pallets (5) reached"),
            None
        );
        assert_eq!(ground_pile_cap("Error: something else"), None);
        let log = "GIANTS Engine Runtime 10.0
2026-10-30 10:00:00.000 Error: maximum number (63) of height types already registered
Application quit
";
        assert_eq!(parse(log).ground_pile_cap, Some(63));
        assert_eq!(parse(CRASHY).ground_pile_cap, None);
    }

    /// #341: each skipped fill type is its own line; collect the names once each.
    #[test]
    fn finds_dropped_fill_types() {
        let line = "Error: FillTypeManager.addFillType too many fill types. Only 255 fill types are supported. Ignoring 'LIQUID_UREA'.";
        assert_eq!(dropped_fill_type(line), Some((255, "LIQUID_UREA".into())));
        assert_eq!(dropped_fill_type("Error: something else 'X'"), None);
        let other = line.replace("LIQUID_UREA", "GYPSUM");
        let log =
            format!("GIANTS Engine Runtime 10.0\n{line}\n{other}\n{line}\nApplication quit\n");
        let r = parse(&log);
        assert_eq!(r.fill_type_cap, Some(255));
        assert_eq!(r.dropped_fill_types, ["LIQUID_UREA", "GYPSUM"]);
        assert!(parse(CRASHY).dropped_fill_types.is_empty());
    }

    /// A log no bigger than head + tail is read whole, exactly as `parse` reads it.
    #[test]
    fn scan_reads_a_small_log_whole() {
        let r = scan(std::io::Cursor::new(CRASHY), 10, CRASHY.len() as u64).unwrap();
        assert!(!r.truncated);
        assert_eq!(r.total_findings, parse(CRASHY).total_findings);
        assert_eq!(r.mods[0].sample_line, parse(CRASHY).mods[0].sample_line);
    }

    /// #351: a log bigger than head + tail keeps whole lines from its start and its end, and
    /// every finding keeps its true line number, whatever the cut points.
    #[test]
    fn scan_keeps_the_head_and_tail_with_true_line_numbers() {
        let log = "GIANTS Engine Runtime 10.0
Available mod: (Hash: a) (Version: 1.0.0.0) FS25_A
Error: early in mod 'FS25_A'
Error: spam 1 in mod 'FS25_Spam'
Error: spam 2 in mod 'FS25_Spam'
  mods/FS25_Spam/a.lua(1)
Error: spam 3 in mod 'FS25_Spam'
Error: Running LUA method 'update'.
  mods/FS25_Late/x.lua(9) : nil
Application quit
";
        let full: Vec<&str> = log.lines().collect();
        let len = log.len() as u64;
        for head in 0..len {
            for tail in 0..len - head {
                let r = scan(std::io::Cursor::new(log), head, tail).unwrap();
                assert!(r.truncated, "head {head} tail {tail}");
                let check = |line: usize, msg: &str| {
                    let first = msg.lines().next().unwrap();
                    assert_eq!(
                        strip_prefix(full[line - 1]),
                        first,
                        "head {head} tail {tail}"
                    );
                };
                for m in &r.mods {
                    check(m.sample_line, &m.sample);
                }
                // The last line is whole in the tail as soon as the tail can hold it.
                if tail >= "Application quit\n".len() as u64 {
                    assert!(r.clean_exit, "head {head} tail {tail}");
                } else {
                    assert!(!r.clean_exit, "head {head} tail {tail}");
                }
            }
        }
        // A realistic cut: the start and the crash at the end survive, the spam doesn't.
        let tail = (log.len() - log.find("Error: Running").unwrap()) as u64;
        let r = scan(std::io::Cursor::new(log), 120, tail).unwrap();
        assert_eq!(r.engine_version.as_deref(), Some("10.0"));
        assert_eq!(r.mod_count, 1);
        let late = r.mods.iter().find(|m| m.mod_name == "FS25_Late").unwrap();
        assert_eq!(late.sample_line, 8);
        assert!(r.mods.iter().all(|m| m.mod_name != "FS25_Spam"));
    }

    /// Lines that don't follow on (the gap where a huge log's middle was skipped) never fold
    /// into the finding above them.
    #[test]
    fn a_gap_ends_a_block() {
        let r = parse_lines([(1, "Error: boom"), (9, "  mods/FS25_B/x.lua(1)")].into_iter());
        assert_eq!((r.unattributed, r.total_findings), (1, 1));
        let r = parse_lines([(1, "Error: boom"), (2, "  mods/FS25_B/x.lua(1)")].into_iter());
        assert_eq!(r.mods[0].mod_name, "FS25_B");
    }

    /// The fold still stops after 12 continuation lines, as before #351.
    #[test]
    fn a_block_folds_at_most_twelve_lines() {
        let mut log = String::from("Error: boom\n");
        for i in 0..14 {
            log.push_str(&format!("  frame{i}.lua\n"));
        }
        let r = parse(&log);
        assert_eq!(r.total_findings, 1);
        let mut log = String::from("Error: boom\n");
        for i in 0..12 {
            log.push_str(&format!("  frame{i}.lua\n"));
        }
        log.push_str("  mods/FS25_Thirteen/x.lua\n");
        // The 13th line isn't folded, so the mod it names isn't blamed.
        assert_eq!(parse(&log).unattributed, 1);
    }

    /// #339: name only the DLCs that actually logged turning themselves off.
    #[test]
    fn finds_what_turned_off_for_lack_of_slots() {
        let pnh = "Error: PnH: unable to register the required density map height types, the limit of the game is reached. Pumps N' Hoses is disabled and the savegame will be closed.";
        let sh = "Error: StrawHarvest: unable to register the required density map height types, the limit of the game is reached. Straw Harvest is disabled and the savegame will be closed.";
        assert_eq!(ground_pile_disabled(pnh).as_deref(), Some("Pumps N' Hoses"));
        assert_eq!(ground_pile_disabled(sh).as_deref(), Some("Straw Harvest"));
        assert_eq!(ground_pile_disabled("Error: something is disabled"), None);
        let log = format!("GIANTS Engine Runtime 10.0\n{pnh}\n{sh}\n{pnh}\n");
        assert_eq!(
            parse(&log).ground_pile_disabled,
            ["Pumps N' Hoses", "Straw Harvest"]
        );
        assert!(parse(CRASHY).ground_pile_disabled.is_empty());
    }
}
