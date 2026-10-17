//! Which mods folder Silo manages (#162). Pure logic plus the one trusted "active root".

use crate::xmlconfig;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;

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
            if from_override {
                Source::GameOverride
            } else {
                Source::Default
            },
        ),
    };
    let mismatch = source == Source::User
        && match (&path, &game_reads) {
            (Some(a), Some(b)) => !same(a, b),
            (Some(_), None) => true,
            _ => false,
        };
    Resolved {
        path,
        source,
        game_reads,
        mismatch,
        unresolved_override,
    }
}

/// The trusted write targets, published by the mods-root/archive commands: the active layout
/// and, while an archive move is unfinished, its other archive. `lib::allowed_roots` reads it,
/// so every organize/install command follows the folder (and archive) the user picked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Active {
    pub layout: crate::organize::Layout,
    /// The other archive of an unfinished move (`layout.archive` is where it started). Whether
    /// a move is copying right now is `relocate::is_running()`.
    pub move_to: Option<PathBuf>,
}

/// The published layout and, when there is none because the folder is blocked, why. One lock
/// for both, so no reader ever sees "no layout" without its reason (or a stale reason beside
/// a new layout).
struct Published {
    active: Option<Active>,
    blocked: Option<Blocked>,
}

static PUBLISHED: RwLock<Published> = RwLock::new(Published {
    active: None,
    blocked: None,
});

/// Set once the mods folder has been resolved at least once (at startup or by a mods-root
/// command), so "no active layout" can be told apart from "not worked out yet".
static RESOLVED: AtomicBool = AtomicBool::new(false);

/// Publish the resolved layout and its block together (`blocked` is meant for `active: None`).
pub fn publish(active: Option<Active>, blocked: Option<Blocked>) {
    if let Ok(mut g) = PUBLISHED.write() {
        *g = Published { active, blocked };
    }
    RESOLVED.store(true, Ordering::SeqCst);
}

/// Publish a layout (or none) with no block.
pub fn set_active(a: Option<Active>) {
    publish(a, None);
}

/// Whether `active()` reflects a resolution (it may still be `None`: no usable mods folder).
pub fn resolved() -> bool {
    RESOLVED.load(Ordering::SeqCst)
}

/// The published layout and block, read together.
pub fn published() -> (Option<Active>, Option<Blocked>) {
    PUBLISHED
        .read()
        .map(|g| (g.active.clone(), g.blocked.clone()))
        .unwrap_or((None, None))
}

pub fn active() -> Option<Active> {
    published().0
}

/// The folder FS25 reads (its override, or the default), as last resolved.
static GAME_READS: RwLock<Option<PathBuf>> = RwLock::new(None);

/// What makes the resolved mods folder share files with another layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BlockCause {
    /// It overlaps another mods folder or archive Silo knows.
    Overlap,
    /// Only because of the folder FS25's own setting points at: pointing FS25 at this folder
    /// clears it.
    GameSetting,
    /// The check couldn't read Silo's database (try again).
    ReadError,
}

/// Why the resolved mods folder `mods` has no writable layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocked {
    pub reason: String,
    pub cause: BlockCause,
    pub mods: PathBuf,
    /// The folder it runs into, when known.
    pub conflict: Option<PathBuf>,
}

pub fn set_game_reads(p: Option<PathBuf>) {
    if let Ok(mut g) = GAME_READS.write() {
        *g = p;
    }
}

pub fn game_reads() -> Option<PathBuf> {
    GAME_READS.read().ok().and_then(|g| g.clone())
}

/// The reason the resolved folder is blocked, if it is.
pub fn blocked() -> Option<String> {
    blocked_detail().map(|b| b.reason)
}

/// Why the resolved mods folder has no writable layout (it shares files with another layout),
/// as last published. `None` when nothing blocks it.
pub fn blocked_detail() -> Option<Blocked> {
    published().1
}

/// The known-folders list stored as a JSON array of path strings.
pub fn parse_known(json: Option<&str>) -> Vec<String> {
    json.and_then(|j| serde_json::from_str::<Vec<String>>(j).ok())
        .unwrap_or_default()
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
    let mut s = strip_verbatim(raw, windows);
    if windows {
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

/// Windows: drop the verbatim prefix `canonicalize` adds (`\\?\C:\x` → `C:\x`,
/// `\\?\UNC\srv\share` → `\\srv\share`), keeping case. Elsewhere: unchanged.
pub fn strip_verbatim(raw: &str, windows: bool) -> String {
    if windows {
        if let Some(rest) = raw.strip_prefix(r"\\?\UNC\") {
            return format!(r"\\{rest}");
        }
        if let Some(rest) = raw.strip_prefix(r"\\?\") {
            return rest.to_string();
        }
    }
    raw.to_string()
}

/// An existing folder the user picked, resolved to its real location and spelled the way
/// Silo stores it (no verbatim prefix, case kept). Relative paths are refused before
/// anything touches the disk.
pub fn real_folder(p: &Path) -> Result<PathBuf, String> {
    if !p.is_absolute() {
        return Err(format!("Choose a folder by its full path: {}", p.display()));
    }
    let c = std::fs::canonicalize(p).map_err(|_| "That folder doesn't exist".to_string())?;
    if !c.is_dir() {
        return Err("That's a file, not a folder".into());
    }
    Ok(PathBuf::from(strip_verbatim(
        &c.to_string_lossy(),
        cfg!(windows),
    )))
}

/// The identity key for a folder: canonicalized, then normalized. A folder that doesn't exist
/// (yet, or any more) is keyed through its nearest existing parent, canonicalized, with the
/// rest re-appended — so a link or junction on the way still yields the real path's key. This
/// is the value stored in the `root` column and used for every "same folder?" comparison.
pub fn canonical_key(p: &Path) -> String {
    normalize_key(&resolve_existing(p).to_string_lossy(), cfg!(windows))
}

/// `p` canonicalized as far as it exists: the nearest existing ancestor's real path plus the
/// missing names below it. Unchanged when no ancestor resolves, or the missing part holds a
/// `..` (it can't be re-appended faithfully).
fn resolve_existing(p: &Path) -> PathBuf {
    if let Ok(c) = std::fs::canonicalize(p) {
        return c;
    }
    let mut missing = Vec::new();
    let mut cur = p;
    while let (Some(parent), Some(name)) = (cur.parent(), cur.file_name()) {
        missing.push(name);
        cur = parent;
        if let Ok(mut c) = std::fs::canonicalize(cur) {
            for n in missing.iter().rev() {
                c.push(n);
            }
            return c;
        }
    }
    p.to_path_buf()
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
    fn strip_verbatim_keeps_case() {
        assert_eq!(strip_verbatim(r"\\?\D:\My Archive", true), r"D:\My Archive");
        assert_eq!(
            strip_verbatim(r"\\?\UNC\NAS\Share\Arch", true),
            r"\\NAS\Share\Arch"
        );
        assert_eq!(strip_verbatim(r"D:\Plain", true), r"D:\Plain");
        // Not Windows: never touched.
        assert_eq!(strip_verbatim(r"\\?\x", false), r"\\?\x");
    }

    #[test]
    fn real_folder_refuses_relative_missing_and_files() {
        assert!(real_folder(Path::new("relative/dir")).is_err());
        let d = std::env::temp_dir().join(format!("silo-real-folder-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("file.txt");
        std::fs::write(&f, b"x").unwrap();
        assert!(real_folder(&d.join("missing")).is_err());
        assert!(real_folder(&f).is_err());
        let r = real_folder(&d).unwrap();
        assert!(r.is_absolute() && r.is_dir());
        assert!(!r.to_string_lossy().starts_with(r"\\?\"));
        assert!(same_folder(&r, &d));
        let _ = std::fs::remove_dir_all(&d);
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

    fn same(a: &Path, b: &Path) -> bool {
        a == b
    }
    fn ov(active: bool, dir: &str) -> Override {
        Override {
            active,
            directory: dir.into(),
        }
    }

    #[test]
    fn resolve_precedence_choice_then_active_override_then_default() {
        let id = PathMap::identity();
        let def = Some(PathBuf::from("/def"));

        let r = resolve(None, None, def.clone(), &id, same);
        assert_eq!(
            (r.path.clone(), r.source, r.mismatch),
            (def.clone(), Source::Default, false)
        );

        let o = ov(true, "/ov");
        let r = resolve(None, Some(&o), def.clone(), &id, same);
        assert_eq!(
            (r.path.clone(), r.source),
            (Some(PathBuf::from("/ov")), Source::GameOverride)
        );
        assert_eq!(r.game_reads, Some(PathBuf::from("/ov")));

        let r = resolve(
            Some(PathBuf::from("/mine")),
            Some(&o),
            def.clone(),
            &id,
            same,
        );
        assert_eq!(
            (r.path.clone(), r.source, r.mismatch),
            (Some(PathBuf::from("/mine")), Source::User, true)
        );

        let r = resolve(Some(PathBuf::from("/ov")), Some(&o), def, &id, same);
        assert!(
            !r.mismatch,
            "choosing the folder the game reads is not a mismatch"
        );
    }

    #[test]
    fn resolve_ignores_an_inactive_override_and_reports_untranslatable_ones() {
        let def = Some(PathBuf::from("/def"));
        let r = resolve(
            None,
            Some(&ov(false, "/ov")),
            def.clone(),
            &PathMap::identity(),
            same,
        );
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
        assert_eq!(
            parse_known(Some(&add_known(Some(&ab), "/a"))),
            vec!["/a", "/b"]
        );
        assert!(parse_known(Some("not json")).is_empty());
    }

    #[test]
    fn a_missing_folder_is_keyed_through_its_nearest_existing_parent() {
        let base = std::env::temp_dir().join(format!("silo_key_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real");
        std::fs::create_dir_all(&real).unwrap();
        // Spelled with `.` segments: the existing part is resolved, the rest re-appended.
        assert_eq!(
            canonical_key(&real.join(".").join("gone/deeper")),
            canonical_key(&real.join("gone/deeper"))
        );
        let resolved = std::fs::canonicalize(&real)
            .unwrap()
            .join("gone")
            .join("deeper");
        assert_eq!(
            canonical_key(&real.join("gone").join("deeper")),
            normalize_key(&resolved.to_string_lossy(), cfg!(windows))
        );
        // Reached through a link (a junction on Windows): the same key as the real path.
        let link = base.join("link");
        #[cfg(windows)]
        crate::organize::junction_to(&real, &link).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(
            canonical_key(&link.join("gone/deeper")),
            canonical_key(&real.join("gone/deeper"))
        );
        let _ = std::fs::remove_dir(&link);
        let _ = std::fs::remove_file(&link);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_layout_and_its_block_are_published_together() {
        let b = Blocked {
            reason: "why".into(),
            cause: BlockCause::Overlap,
            mods: PathBuf::from("/m"),
            conflict: None,
        };
        let a = Active {
            layout: crate::organize::Layout::default_for(Path::new("/y")),
            move_to: None,
        };
        publish(None, Some(b.clone()));
        let (pa, pb) = published();
        assert!(pa.is_none());
        assert_eq!(pb, Some(b));
        // A new layout replaces the block in the same step.
        publish(Some(a.clone()), None);
        assert_eq!(published(), (Some(a), None));
        set_active(None);
    }

    #[test]
    fn active_layout_round_trips() {
        let l = crate::organize::Layout::default_for(Path::new("/x"));
        set_active(Some(Active {
            layout: l.clone(),
            move_to: None,
        }));
        assert_eq!(active().map(|a| a.layout), Some(l));
        set_active(None);
        assert!(active().is_none());
        assert!(
            resolved(),
            "publishing — even \"no folder\" — marks the layout resolved"
        );
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
}
