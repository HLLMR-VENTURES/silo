//! Library scan: walk the mod root(s), read each mod's `modDesc.xml`, parse it,
//! and return structured entries. All heavy work runs here on a rayon pool — the
//! UI thread never touches a zip. We read ONLY the `modDesc.xml` entry from each
//! archive (the zip central directory + one small entry), so even 100 MB+ map
//! mods scan cheaply. Content hashing and icon decode are deliberately deferred.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::db::CacheEntry;
use crate::moddesc;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModEntry {
    /// Tech name = the zip/dir basename (mod identity the game & deps use).
    pub tech_name: String,
    pub path: String,
    /// "zip" | "dir"
    pub kind: String,
    pub size: u64,
    pub mtime_ms: u64,

    pub title: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub desc_version: Option<i64>,
    /// Path to the mod's icon inside the archive/dir (for lazy thumbnail loading).
    pub icon_filename: Option<String>,

    pub is_map: bool,
    pub map_title: Option<String>,
    /// Best-effort category folder bucket (see `category` module).
    pub category: String,
    /// Optional second-level bucket (e.g. Tractors › Medium).
    pub subcategory: Option<String>,

    pub dependencies: Vec<String>,
    pub script_count: usize,
    pub registration_count: usize,
    pub unique_type: Option<String>,
    pub store_item_count: u32,
    pub mp_supported: bool,

    /// True when the mod lives in `mods/archive/<Category>/` (Silo-managed).
    pub organized: bool,
    /// True when the mod is present in the flat root (i.e. the game loads it):
    /// vanilla mods, and organized mods currently projected as links.
    pub active: bool,

    /// The game IGNORES mods whose name starts with a digit — a silent footgun.
    pub ignored_digit_prefix: bool,
    /// Populated when the mod couldn't be read/parsed (still listed, flagged).
    pub error: Option<String>,

    /// Whether the mod exposes user-configurable settings, detected by looking inside the
    /// archive/dir (not just the runtime `modSettings/` folder). See `detect_settings`.
    /// `serde(default)` so scan-cache rows written before this field deserialize cleanly.
    #[serde(default)]
    pub has_settings: bool,
    /// How the settings were detected — governs whether Silo can edit them right now.
    #[serde(default)]
    pub settings_source: SettingsSource,
}

/// How a mod's settings were detected, which decides whether Silo can edit them now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum SettingsSource {
    /// No settings detected.
    #[default]
    None,
    /// Ships a settings XML in the archive (Tier A) — defaults are readable statically.
    Shipped,
    /// Lua persists settings under `modSettings/` at runtime (Tier B) — values appear
    /// only after the game has run the mod once.
    Runtime,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub mods: Vec<ModEntry>,
    pub roots: Vec<String>,
    pub took_ms: u128,
    pub total: usize,
}

/// Result of a cached scan: the full list plus which paths were freshly parsed
/// (cache misses) so the caller can persist just those.
pub struct ScanOutput {
    pub result: ScanResult,
    pub fresh_paths: HashSet<String>,
}

struct Candidate {
    path: PathBuf,
    tech_name: String,
    kind: &'static str,
}

/// Build a Candidate from a directory entry (a `.zip` file or an unpacked mod dir),
/// or None if it isn't a mod.
fn candidate_from(path: PathBuf, name: &str, ft: std::fs::FileType) -> Option<Candidate> {
    // A symlink's own `file_type` is neither dir nor file, so resolve the target — otherwise
    // symlinked dev mods (a common workflow, e.g. `mods/FS25_Foo -> <dev repo>`) and Silo's
    // own dir-mod junction projections would be silently skipped. `fs::metadata` follows links.
    let (is_dir, is_file) = if ft.is_symlink() {
        match fs::metadata(&path) {
            Ok(m) => (m.is_dir(), m.is_file()),
            Err(_) => (false, false), // dangling link — ignore
        }
    } else {
        (ft.is_dir(), ft.is_file())
    };

    if is_dir {
        if path.join("modDesc.xml").is_file() {
            return Some(Candidate {
                tech_name: name.to_string(),
                path,
                kind: "dir",
            });
        }
    } else if is_file && name.to_lowercase().ends_with(".zip") {
        let tech_name = name[..name.len() - 4].to_string();
        return Some(Candidate {
            tech_name,
            path,
            kind: "zip",
        });
    }
    None
}

/// Mods in the flat root (what the game reads) — excluding Silo's `archive/` and
/// any `backups/` folder.
fn collect_root_candidates(root: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(rd) = fs::read_dir(root) else {
        return out;
    };
    for entry in rd.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let lname = name.to_lowercase();
        if lname == "archive" || lname == "backups" {
            continue;
        }
        if let Ok(ft) = entry.file_type() {
            if let Some(c) = candidate_from(entry.path(), &name, ft) {
                out.push(c);
            }
        }
    }
    out
}

/// Mods parked in an archive folder's `<Category>/` subfolders (Silo-managed, one level of folders).
fn collect_archive_candidates(archive: &Path) -> Vec<Candidate> {
    let mut out = Vec::new();
    let Ok(cats) = fs::read_dir(archive) else {
        return out;
    };
    for cat in cats.flatten() {
        if !cat.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            continue;
        }
        let Ok(mods) = fs::read_dir(cat.path()) else {
            continue;
        };
        for entry in mods.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Ok(ft) = entry.file_type() {
                if let Some(c) = candidate_from(entry.path(), &name, ft) {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// Read a mod's `modDesc.xml` text, whether it's a `.zip` or an unpacked dir.
pub fn read_moddesc_xml(mod_path: &Path, kind: &str) -> Result<String, String> {
    match kind {
        "zip" => read_moddesc_from_zip(mod_path),
        _ => read_moddesc_from_dir(mod_path),
    }
}

/// Largest we'll read for a single member. modDesc.xml is normally a few KB; a script or
/// icon a few MB. These caps stop a hostile/broken archive from decompressing a tiny
/// entry into gigabytes of memory (a zip bomb) — the game trusts the mod folder, but Silo
/// parses arbitrary downloads, so it must not.
pub(crate) const MODDESC_LIMIT: u64 = 16 * 1024 * 1024;

/// Read a (possibly-decompressing) reader with a hard byte cap. Errors — never
/// truncates — if the content exceeds `limit`, so a zip bomb can't exhaust memory.
pub(crate) fn read_capped<R: Read>(r: R, limit: u64, what: &str) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    // Read one past the limit so we can tell "exactly at the cap" from "over it".
    r.take(limit + 1)
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    if buf.len() as u64 > limit {
        return Err(format!(
            "{what} is larger than the {limit}-byte safety limit"
        ));
    }
    Ok(buf)
}

fn read_moddesc_from_zip(path: &Path) -> Result<String, String> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    // modDesc.xml lives at the archive root per FS conventions.
    let f = archive
        .by_name("modDesc.xml")
        .map_err(|_| "modDesc.xml not found in archive".to_string())?;
    let bytes = read_capped(f, MODDESC_LIMIT, "modDesc.xml")?;
    String::from_utf8(bytes).map_err(|e| e.to_string())
}

fn read_moddesc_from_dir(path: &Path) -> Result<String, String> {
    let p = path.join("modDesc.xml");
    if fs::metadata(&p).map(|m| m.len()).unwrap_or(0) > MODDESC_LIMIT {
        return Err("modDesc.xml is larger than the safety limit".to_string());
    }
    fs::read_to_string(p).map_err(|e| e.to_string())
}

fn build_entry(c: &Candidate) -> ModEntry {
    let meta = fs::metadata(&c.path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let ignored_digit_prefix = c
        .tech_name
        .chars()
        .next()
        .map(|ch| ch.is_ascii_digit())
        .unwrap_or(false);

    let mut entry = ModEntry {
        tech_name: c.tech_name.clone(),
        path: c.path.to_string_lossy().into_owned(),
        kind: c.kind.to_string(),
        size,
        mtime_ms,
        title: None,
        author: None,
        version: None,
        desc_version: None,
        icon_filename: None,
        is_map: false,
        map_title: None,
        category: "Other".to_string(),
        subcategory: None,
        dependencies: Vec::new(),
        script_count: 0,
        registration_count: 0,
        unique_type: None,
        store_item_count: 0,
        mp_supported: false,
        organized: false,
        active: false,
        ignored_digit_prefix,
        error: None,
        has_settings: false,
        settings_source: SettingsSource::None,
    };

    let xml = match c.kind {
        "zip" => read_moddesc_from_zip(&c.path),
        _ => read_moddesc_from_dir(&c.path),
    };

    match xml {
        Ok(xml) => {
            let md = moddesc::parse(&xml);
            // Compute category while `md` is still whole (before we move fields out).
            // Read the authoritative FS store category when the mod has store items.
            let store_cat = if md.store_item_files.is_empty() {
                None
            } else {
                crate::store::first_store_category(&c.path, c.kind, &md.store_item_files)
            };
            let title = md.title.clone().or_else(|| Some(c.tech_name.clone()));
            let (category, subcategory) = crate::category::categorize(
                &md,
                store_cat.as_deref(),
                &c.tech_name,
                title.as_deref(),
            );
            entry.category = category;
            entry.subcategory = subcategory;
            entry.title = title;
            entry.author = md.author;
            entry.version = md.version;
            entry.desc_version = md.desc_version;
            entry.icon_filename = md.icon_filename;
            entry.is_map = md.is_map;
            entry.map_title = md.map_title;
            entry.dependencies = md.dependencies;
            entry.script_count = md.scripts.len();
            entry.registration_count = md.registrations.len();
            entry.unique_type = md.unique_type;
            entry.store_item_count = md.store_item_count;
            entry.mp_supported = md.mp_supported;
        }
        Err(e) => {
            entry.title = Some(c.tech_name.clone());
            entry.error = Some(e);
        }
    }

    let (has_settings, settings_source) = detect_settings(&c.path, c.kind);
    entry.has_settings = has_settings;
    entry.settings_source = settings_source;

    entry
}

/// Budget for the Tier-B Lua grep so a huge script mod can't stall the scan. The whole
/// scan of one mod stays cheap: Tier A is a name-list check (no decompression), Tier B
/// decompresses Lua only until a hit or this budget is spent.
const LUA_SCAN_BUDGET: u64 = 8 * 1024 * 1024;
const LUA_MEMBER_CAP: u64 = 4 * 1024 * 1024;

/// Detect whether a mod exposes user-configurable settings by looking inside it.
///
/// Two tiers (see the settings-detection research): **Shipped** (Tier A) — the archive
/// carries a `*settings*.xml` whose defaults we can read now — beats **Runtime** (Tier B) —
/// Lua that persists to a `modSettings/` path, whose values only exist after a run.
/// `modDesc.xml` declares nothing useful here, so we never consult it. Best-effort: any
/// read error just yields "no settings" rather than failing the scan.
pub fn detect_settings(path: &Path, kind: &str) -> (bool, SettingsSource) {
    match kind {
        "zip" => detect_settings_zip(path),
        _ => detect_settings_dir(path),
    }
}

/// Tier A: an XML whose basename says "settings" — but not a translation/l10n file, which
/// often ship a `settings` string in a different sense.
fn is_settings_xml(name_lower: &str) -> bool {
    if !name_lower.ends_with(".xml") {
        return false;
    }
    if name_lower.contains("translation") || name_lower.contains("l10n") {
        return false;
    }
    let base = name_lower.rsplit(['/', '\\']).next().unwrap_or(name_lower);
    base.contains("settings")
}

/// Tier B: Lua that references a `modSettings/` path (also the seen-in-the-wild
/// `modsSettings` typo, and the `g_*ModSettingsDirectory` globals — all lowercase to
/// `modsettings`). This is the strongest static predictor of runtime-persisted settings.
fn lua_has_settings_signature(bytes: &[u8]) -> bool {
    let lower = String::from_utf8_lossy(bytes).to_ascii_lowercase();
    lower.contains("modsettings") || lower.contains("modssettings")
}

fn detect_settings_zip(path: &Path) -> (bool, SettingsSource) {
    let Ok(file) = fs::File::open(path) else {
        return (false, SettingsSource::None);
    };
    let Ok(mut archive) = zip::ZipArchive::new(file) else {
        return (false, SettingsSource::None);
    };
    // Cheap first pass over just the name list (no decompression): Tier A wins outright,
    // and we note the Lua members for the bounded Tier-B grep.
    let names: Vec<String> = archive
        .file_names()
        .filter_map(|s| s.ok().map(|n| n.into_owned()))
        .collect();
    let mut lua: Vec<String> = Vec::new();
    for name in &names {
        let lower = name.to_ascii_lowercase();
        if is_settings_xml(&lower) {
            return (true, SettingsSource::Shipped);
        }
        if lower.ends_with(".lua") {
            lua.push(name.clone());
        }
    }
    let mut budget = LUA_SCAN_BUDGET;
    for name in lua {
        if budget == 0 {
            break;
        }
        let Ok(f) = archive.by_name(&name) else {
            continue;
        };
        let cap = budget.min(LUA_MEMBER_CAP);
        let Ok(bytes) = read_capped(f, cap, "lua") else {
            continue; // over the per-member cap — skip it, keep scanning the rest
        };
        budget = budget.saturating_sub(bytes.len() as u64);
        if lua_has_settings_signature(&bytes) {
            return (true, SettingsSource::Runtime);
        }
    }
    (false, SettingsSource::None)
}

fn detect_settings_dir(path: &Path) -> (bool, SettingsSource) {
    let mut lua: Vec<PathBuf> = Vec::new();
    for entry in walkdir::WalkDir::new(path)
        .max_depth(8)
        .into_iter()
        .flatten()
    {
        if !entry.file_type().is_file() {
            continue;
        }
        let lower = entry.path().to_string_lossy().to_ascii_lowercase();
        if is_settings_xml(&lower) {
            return (true, SettingsSource::Shipped);
        }
        if lower.ends_with(".lua") {
            lua.push(entry.path().to_path_buf());
        }
    }
    let mut budget = LUA_SCAN_BUDGET;
    for p in lua {
        if budget == 0 {
            break;
        }
        let Ok(file) = fs::File::open(&p) else {
            continue;
        };
        let cap = budget.min(LUA_MEMBER_CAP);
        let Ok(bytes) = read_capped(file, cap, "lua") else {
            continue;
        };
        budget = budget.saturating_sub(bytes.len() as u64);
        if lua_has_settings_signature(&bytes) {
            return (true, SettingsSource::Runtime);
        }
    }
    (false, SettingsSource::None)
}

/// Resolve one candidate: reuse the cached entry when mtime+size are unchanged
/// (no archive open), otherwise parse fresh. Returns (entry, was_freshly_parsed).
fn resolve_entry(c: &Candidate, cache: &HashMap<String, CacheEntry>) -> (ModEntry, bool) {
    let meta = fs::metadata(&c.path).ok();
    let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
    let mtime_ms = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    if let Some(ce) = cache.get(c.path.to_string_lossy().as_ref()) {
        if ce.mtime_ms == mtime_ms && ce.size == size {
            // A schema change that fails deserialization simply falls through to a
            // fresh parse — we never trust a partial decode.
            if let Ok(entry) = serde_json::from_str::<ModEntry>(&ce.json) {
                return (entry, false);
            }
        }
    }
    (build_entry(c), true)
}

/// Core scan over mods folders with explicit archive folders. Archives are read in order and a
/// techName already seen in an earlier archive is skipped (during a paused archive move the new
/// archive is listed first). No Tauri dependency — testable / CLI-reusable.
pub fn scan_layouts<F>(
    layouts: Vec<(PathBuf, Vec<PathBuf>)>,
    cache: &HashMap<String, CacheEntry>,
    progress: F,
) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    let started = std::time::Instant::now();
    let mut root_cands: Vec<Candidate> = Vec::new();
    let mut archive_cands: Vec<Candidate> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (root, archives) in &layouts {
        root_cands.extend(collect_root_candidates(root));
        for a in archives {
            // Skip only names seen in an EARLIER archive; a single archive's own contents are
            // kept as-is so a one-archive scan behaves exactly as before.
            let cands = collect_archive_candidates(a);
            let names: Vec<String> = cands.iter().map(|c| c.tech_name.clone()).collect();
            archive_cands.extend(cands.into_iter().filter(|c| !seen.contains(&c.tech_name)));
            seen.extend(names);
        }
    }
    let roots: Vec<PathBuf> = layouts.into_iter().map(|(r, _)| r).collect();
    finish_scan(roots, root_cands, archive_cands, cache, progress, started)
}

/// Scan with each root's default archive (`<root>/archive`).
pub fn scan_cached<F>(
    roots: Vec<PathBuf>,
    cache: &HashMap<String, CacheEntry>,
    progress: F,
) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    let layouts = roots
        .into_iter()
        .map(|r| {
            let a = r.join("archive");
            (r, vec![a])
        })
        .collect();
    scan_layouts(layouts, cache, progress)
}

/// Resolve each candidate from `cache` when possible and parse the rest in parallel.
/// Reports progress via a callback (invoked from many rayon threads, hence Sync).
fn finish_scan<F>(
    roots: Vec<PathBuf>,
    root_cands: Vec<Candidate>,
    archive_cands: Vec<Candidate>,
    cache: &HashMap<String, CacheEntry>,
    progress: F,
    started: std::time::Instant,
) -> ScanOutput
where
    F: Fn(usize, usize) + Sync + Send,
{
    // Tech-names present in the archive (organized) and in the flat root (loaded).
    let archived: HashSet<String> = archive_cands.iter().map(|c| c.tech_name.clone()).collect();
    let in_root: HashSet<String> = root_cands.iter().map(|c| c.tech_name.clone()).collect();

    // Parse every archived mod, plus flat-root mods that AREN'T organized (vanilla).
    // A flat-root entry whose tech-name is already in the archive is just an active
    // projection (hardlink) — we don't re-parse it, only flag the archive entry.
    let to_parse: Vec<(&Candidate, bool)> = archive_cands
        .iter()
        .map(|c| (c, true))
        .chain(
            root_cands
                .iter()
                .filter(|c| !archived.contains(&c.tech_name))
                .map(|c| (c, false)),
        )
        .collect();

    let total = to_parse.len();
    progress(0, total);

    let done = AtomicUsize::new(0);
    let step = (total / 100).max(10);

    let pairs: Vec<(ModEntry, bool)> = to_parse
        .par_iter()
        .map(|(c, organized)| {
            let (mut entry, fresh) = resolve_entry(c, cache);
            entry.organized = *organized;
            // Organized mods are "active" only when also linked into the flat root;
            // vanilla (unorganized) mods sit in the root, so they're active.
            entry.active = if *organized {
                in_root.contains(&entry.tech_name)
            } else {
                true
            };
            let n = done.fetch_add(1, Ordering::Relaxed) + 1;
            if n % step == 0 || n == total {
                progress(n, total);
            }
            (entry, fresh)
        })
        .collect();

    let mut fresh_paths = HashSet::new();
    let mut mods = Vec::with_capacity(pairs.len());
    for (entry, fresh) in pairs {
        if fresh {
            fresh_paths.insert(entry.path.clone());
        }
        mods.push(entry);
    }

    mods.sort_by(|a, b| {
        let ta = a.title.as_deref().unwrap_or(&a.tech_name).to_lowercase();
        let tb = b.title.as_deref().unwrap_or(&b.tech_name).to_lowercase();
        ta.cmp(&tb)
    });

    progress(total, total);

    ScanOutput {
        result: ScanResult {
            mods,
            roots: roots
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect(),
            took_ms: started.elapsed().as_millis(),
            total,
        },
        fresh_paths,
    }
}

/// Convenience wrapper with no cache — used by the example harness and tests.
pub fn scan_with<F>(roots: Vec<PathBuf>, progress: F) -> ScanResult
where
    F: Fn(usize, usize) + Sync + Send,
{
    scan_cached(roots, &HashMap::new(), progress).result
}

#[cfg(test)]
mod cap_tests {
    use super::read_capped;

    #[test]
    fn capped_read_errors_past_limit_never_truncates() {
        assert_eq!(read_capped(&b"hello"[..], 100, "x").unwrap(), b"hello");
        assert!(read_capped(&b"hello"[..], 5, "x").is_ok()); // exactly at cap
        assert!(read_capped(&vec![0u8; 200][..], 100, "big").is_err()); // over cap
    }
}

#[cfg(test)]
mod symlink_tests {
    use super::collect_root_candidates;

    #[cfg(windows)]
    fn symlink_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::os::windows::fs::symlink_dir(src, dst)
    }
    #[cfg(unix)]
    fn symlink_dir(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::os::unix::fs::symlink(src, dst)
    }

    #[test]
    fn follows_a_symlinked_dir_mod() {
        let base = std::env::temp_dir().join(format!("silo_symlink_scan_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("root");
        let target = base.join("devmod");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("modDesc.xml"), b"<modDesc/>").unwrap();

        let link = root.join("FS25_DevLink");
        if symlink_dir(&target, &link).is_err() {
            // Creating a symlink needs privilege on Windows (Developer Mode/admin) — if it's
            // unavailable in this environment, skip rather than fail.
            let _ = std::fs::remove_dir_all(&base);
            return;
        }

        let cands = collect_root_candidates(&root);
        assert!(
            cands
                .iter()
                .any(|c| c.tech_name == "FS25_DevLink" && c.kind == "dir"),
            "a symlinked dir mod should be scanned, got: {:?}",
            cands.iter().map(|c| &c.tech_name).collect::<Vec<_>>()
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}

#[cfg(test)]
mod settings_detect_tests {
    use super::{detect_settings, is_settings_xml, lua_has_settings_signature, SettingsSource};

    #[test]
    fn is_settings_xml_matches_value_and_gui_files_only() {
        assert!(is_settings_xml("shared/defaultusersettings.xml"));
        assert!(is_settings_xml("config/realisticeconomysettings.xml"));
        assert!(is_settings_xml("gui/settingspage.xml"));
        // not settings files
        assert!(!is_settings_xml("moddesc.xml"));
        assert!(!is_settings_xml("data/vehicle.xml"));
        assert!(!is_settings_xml("shared/defaultusersettings.lua"));
        // translation/l10n files that merely contain "settings" are excluded
        assert!(!is_settings_xml("translations/translation_settings_en.xml"));
        assert!(!is_settings_xml("l10n/settings_de.xml"));
    }

    #[test]
    fn lua_signature_catches_modsettings_paths_not_savegame_hooks() {
        assert!(lua_has_settings_signature(
            b"local p = getUserProfileAppPath() .. \"modSettings/Foo.xml\""
        ));
        assert!(lua_has_settings_signature(b"g_currentModSettingsDirectory"));
        assert!(lua_has_settings_signature(b"\"modsSettings/typo.xml\"")); // known typo
                                                                           // the standard savegame persistence hook is NOT a settings signal
        assert!(!lua_has_settings_signature(
            b"function foo:saveToXMLFile(xml) end"
        ));
    }

    fn tmp(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("silo_settings_{}_{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn dir_shipped_beats_runtime() {
        let d = tmp("shipped");
        std::fs::write(d.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::create_dir_all(d.join("scripts")).unwrap();
        std::fs::write(
            d.join("scripts/main.lua"),
            b"getUserProfileAppPath()..\"modSettings/X.xml\"",
        )
        .unwrap();
        std::fs::write(d.join("mysettings.xml"), b"<settings/>").unwrap();
        assert_eq!(detect_settings(&d, "dir"), (true, SettingsSource::Shipped));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dir_runtime_when_only_lua_signature() {
        let d = tmp("runtime");
        std::fs::write(d.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::create_dir_all(d.join("scripts")).unwrap();
        std::fs::write(d.join("scripts/s.lua"), b"local d = g_modSettingsDirectory").unwrap();
        assert_eq!(detect_settings(&d, "dir"), (true, SettingsSource::Runtime));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn dir_none_when_no_signal() {
        let d = tmp("none");
        std::fs::write(d.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(d.join("v.lua"), b"function v:saveToXMLFile() end").unwrap();
        assert_eq!(detect_settings(&d, "dir"), (false, SettingsSource::None));
        let _ = std::fs::remove_dir_all(&d);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    fn moddesc_zip(path: &std::path::Path, title: &str) {
        let f = std::fs::File::create(path).unwrap();
        let mut w = zip::ZipWriter::new(f);
        w.start_file("modDesc.xml", zip::write::SimpleFileOptions::default())
            .unwrap();
        use std::io::Write;
        write!(
            w,
            "<modDesc descVersion=\"92\"><title><en>{title}</en></title><version>1.0.0.0</version></modDesc>"
        )
        .unwrap();
        w.finish().unwrap();
    }

    #[test]
    fn reads_two_archives_without_duplicating_a_mod() {
        let base = std::env::temp_dir().join(format!("silo_scan2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let new_arch = base.join("new");
        let old_arch = base.join("old");
        for d in [&mods, &new_arch.join("Tools"), &old_arch.join("Tools")] {
            std::fs::create_dir_all(d).unwrap();
        }
        moddesc_zip(&new_arch.join("Tools/FS25_Moved.zip"), "Moved");
        moddesc_zip(&old_arch.join("Tools/FS25_Moved.zip"), "Moved");
        moddesc_zip(&old_arch.join("Tools/FS25_NotYet.zip"), "Not yet");

        let out = scan_layouts(
            vec![(mods.clone(), vec![new_arch.clone(), old_arch.clone()])],
            &HashMap::new(),
            |_, _| {},
        );
        let names: Vec<_> = out
            .result
            .mods
            .iter()
            .map(|m| m.tech_name.clone())
            .collect();
        assert_eq!(names.iter().filter(|n| *n == "FS25_Moved").count(), 1);
        assert!(names.contains(&"FS25_NotYet".to_string()));
        let moved = out
            .result
            .mods
            .iter()
            .find(|m| m.tech_name == "FS25_Moved")
            .unwrap();
        assert!(moved.path.contains("new"), "the new archive wins");
        let _ = std::fs::remove_dir_all(&base);
    }
}
