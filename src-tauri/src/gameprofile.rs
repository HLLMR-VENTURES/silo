//! FS25 profiles (#387): a whole separate FS25 user folder that the game uses when it is started
//! with `-profile <dir>`. Such a folder holds its own `mods/`, `gameSettings.xml`, `game.xml`,
//! `log.txt`, `inputBinding.xml`, `modSettings/` and savegames. Power users keep several setups
//! this way.
//!
//! When the mods folder Silo manages is `<profile>/mods`, Silo can start FS25 on that profile,
//! and everything that reads the game's own files (Diagnose, the crash banner, savegames, the
//! config editors) reads the profile's files instead of the main user folder's. Each mods folder
//! has its own on/off setting ("Launch FS25 with this folder's profile"), turned on by itself
//! when the folder's parent looks like a profile.
//!
//! Not to be confused with Silo's debug-only demo profile (`profile.rs`, `SILO_PROFILE_DIR`),
//! which moves *Silo's* data and the main user folder for screenshots. That one is called the
//! "demo profile"; this one is always the "FS25 profile" (`game_profile` in code).

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// `app_setting` key prefix for a mods folder's choice; the rest is the folder's
/// `modsroot::canonical_key`. Values: [`ON`] / [`OFF`]. No row = automatic (on when detected).
const SETTING_PREFIX: &str = "game_profile:";
pub const ON: &str = "on";
pub const OFF: &str = "off";

/// A platform, as a value rather than `#[cfg]`, so what each one supports unit-tests on any
/// machine (#405).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
    /// Anything else Silo might be built for: no profile launch.
    Other,
}

impl Os {
    /// The platform this build runs on.
    pub const CURRENT: Os = if cfg!(windows) {
        Os::Windows
    } else if cfg!(target_os = "macos") {
        Os::MacOs
    } else if cfg!(target_os = "linux") {
        Os::Linux
    } else {
        Os::Other
    };
}

/// Pure: why Silo can't launch FS25 on a separate profile on `os`, or `None` where it can. Where
/// it can't, the setting is hidden, with this reason as its note, and is always off. The evidence
/// for each platform is in `docs/CROSS-PLATFORM.md` (#405).
/// - Windows: the game executable is started directly with `-profile` (hands-on test 2026-11-01).
/// - macOS: no source shows that the Mac build reads `-profile`.
/// - Linux: the Windows game reads `-profile`, but it runs through Proton, so only Steam can start
///   it, and nothing shows that Steam on Linux hands a `steam://run` link's arguments to the game
///   (one report says it ignores them).
pub const fn unsupported_reason(os: Os) -> Option<&'static str> {
    match os {
        Os::Windows => None,
        Os::Linux => Some(
            "Silo can't start Farming Simulator 25 with a separate profile on Linux yet: it isn't confirmed that Steam passes the profile on to the game through Proton.",
        ),
        Os::MacOs => Some(
            "Silo can't start Farming Simulator 25 with a separate profile on macOS yet: it isn't confirmed that the Mac version of the game supports one.",
        ),
        Os::Other => {
            Some("Silo can't start Farming Simulator 25 with a separate profile on this system.")
        }
    }
}

/// Why a profile launch doesn't work on this platform ([`unsupported_reason`]), if it doesn't.
pub const UNSUPPORTED: Option<&str> = unsupported_reason(Os::CURRENT);

/// Whether Silo can launch FS25 on a separate profile on this platform. Where it can't, the
/// setting is hidden, with a note ([`UNSUPPORTED`]), and is always off.
pub const SUPPORTED: bool = UNSUPPORTED.is_none();

/// The `app_setting` key holding the choice for the mods folder with identity key `mods_key`.
pub fn setting_key(mods_key: &str) -> String {
    format!("{SETTING_PREFIX}{mods_key}")
}

/// The `app_setting` key remembering that the mods folder `mods_key`'s parent once looked like a
/// profile (see [`detected`]).
pub fn seen_key(mods_key: &str) -> String {
    format!("game_profile_seen:{mods_key}")
}

/// Whether a folder counts as detected: it looks like a profile now, or it did once. A profile
/// stays one when its files can't be seen for a while (an unplugged drive, the game rewriting its
/// settings, a failed look): Play, Diagnose and savegames never fall back to the main user folder
/// on their own, and the main settings stay protected. The user can still turn the launch off.
pub fn detected(looks_now: bool, seen_before: bool) -> bool {
    looks_now || seen_before
}

/// The profile a mods folder could belong to: its parent, when the folder is named `mods`
/// (FS25 reads `<profile>/mods`) and the parent is neither the main FS25 user folder (that's
/// the normal setup, no profile needed), a folder that holds the main user folder (its whole
/// subtree would become the profile's, main `gameSettings.xml` included), nor the game's install
/// folder (which also has a `game.xml`). `None` means the folder can't be a profile's, and the
/// setting isn't offered.
///
/// `main_user_dir` is the main FS25 user folder, `covers(dir, main)` is true when `dir` is `main`
/// or holds it, and `is_install` says whether a folder is a game install. All injected so this
/// unit-tests without a disk.
pub fn candidate_dir(
    mods: &Path,
    main_user_dir: Option<&Path>,
    covers: impl Fn(&Path, &Path) -> bool,
    is_install: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let name = mods.file_name()?.to_string_lossy();
    if !name.eq_ignore_ascii_case("mods") {
        return None;
    }
    let dir = mods.parent().filter(|p| !p.as_os_str().is_empty())?;
    if main_user_dir.is_some_and(|m| covers(dir, m)) || is_install(dir) {
        return None;
    }
    Some(dir.to_path_buf())
}

/// Pure: whether the folder with identity key `dir` is the folder with key `main`, or holds it
/// (keys as `modsroot::canonical_key` writes them, either separator).
pub fn key_covers(dir: &str, main: &str) -> bool {
    if dir == main {
        return true;
    }
    let Some(rest) = main.strip_prefix(dir) else {
        return false;
    };
    dir.ends_with(['\\', '/']) || rest.starts_with(['\\', '/'])
}

/// True when `dir` looks like a folder FS25 has used as a user folder: it has a `game.xml` or a
/// `gameSettings.xml`, the files the game writes on its first start there.
pub fn looks_like_profile(dir: &Path, exists: impl Fn(&Path) -> bool) -> bool {
    exists(&dir.join("game.xml")) || exists(&dir.join("gameSettings.xml"))
}

/// True when `dir` is an FS25 install folder (it has the game executable or its data archive):
/// it has a `game.xml` too, but it's never a profile.
pub fn is_install_dir(dir: &Path, exists: impl Fn(&Path) -> bool) -> bool {
    exists(&dir.join("FarmingSimulator2025.exe")) || exists(&dir.join("dataS.gar"))
}

/// Whether a folder's profile launch is on: the user's stored choice ([`ON`] / [`OFF`]) wins;
/// with no choice (or an unknown value) it's on exactly when the folder looks like a profile.
/// Always off where launching on a profile isn't supported.
pub fn enabled(stored: Option<&str>, detected: bool, supported: bool) -> bool {
    if !supported {
        return false;
    }
    match stored {
        Some(ON) => true,
        Some(OFF) => false,
        _ => detected,
    }
}

/// What Settings shows about the current mods folder's FS25 profile. Absent from the status
/// when the folder can't be a profile's (see [`candidate_dir`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameProfileStatus {
    /// The profile folder (the mods folder's parent).
    pub dir: String,
    /// Play starts FS25 on this profile, and Silo reads its files.
    pub enabled: bool,
    /// The folder looks like a profile (it has `game.xml` or `gameSettings.xml`).
    pub detected: bool,
    /// The user turned it on or off; otherwise it follows `detected`.
    pub chosen: bool,
    /// Launching on a profile works on this platform ([`SUPPORTED`]).
    pub supported: bool,
    /// Where it doesn't, why, in plain words ([`UNSUPPORTED`]): the Settings note shows it.
    pub unsupported_reason: Option<String>,
}

/// The current mods folder's FS25 profile, as published with the folder's layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    /// The profile folder.
    pub dir: PathBuf,
    /// "Launch FS25 with this folder's profile" is on: Play starts it, and the game's files are
    /// read from it.
    pub launch: bool,
    /// The folder FS25 reads mods from on this profile (its own setting, else `<dir>/mods`),
    /// when Silo could read it. It counts as a live mods folder for the overlap checks.
    pub reads: Option<PathBuf>,
}

/// Published only for a folder that belongs to a profile ([`belongs`]); see `lib::resolve_mods_root`.
static PUBLISHED: RwLock<Option<Published>> = RwLock::new(None);

/// Whether a mods folder belongs to an FS25 profile: its launch is on, or it looks like one. Such
/// a folder never gets the main profile's mods folder setting changed for it, even with the
/// launch off (the owner's rule: the main `gameSettings.xml` isn't touched for a profile folder).
pub fn belongs(status: &GameProfileStatus) -> bool {
    status.enabled || status.detected
}

/// Publish the current folder's profile (or none). The caller publishes it before the layout
/// (`modsroot::publish`), so once the folder counts as resolved its profile is already known.
pub fn publish(p: Option<Published>) {
    if let Ok(mut g) = PUBLISHED.write() {
        *g = p;
    }
}

/// The current folder's profile, if it belongs to one.
pub fn published() -> Option<Published> {
    PUBLISHED.read().ok().and_then(|g| g.clone())
}

/// The FS25 profile in use: `Some(dir)` when the current mods folder's profile launch is on.
pub fn active() -> Option<PathBuf> {
    published().filter(|p| p.launch).map(|p| p.dir)
}

/// Every place Silo reads (or writes) the FS25 user folder, and whether it follows the active
/// FS25 profile. This enum is the one record of that decision; each call site names its place.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// `log.txt`: Diagnose / log triage, the "last game ended unexpectedly" banner and its
    /// last-seen marker, guided bisection's log reads. **Follows**: the game writes the log of
    /// the profile it runs on.
    Log,
    /// `inputBinding.xml` for the Controls map. **Follows**: each profile has its own bindings.
    Bindings,
    /// `savegameN/` folders: the list, backups, and the savegame config editor. **Follows**: a
    /// profile's savegames are its own.
    Savegames,
    /// `modSettings/`: the per-mod settings editors. **Follows**: mods write their settings into
    /// the user folder the game runs on.
    ModSettings,
    /// `game.xml` / `gameSettings.xml` editors (Graphics settings) and the user folder the
    /// webview builds those paths from. **Follows**: editing the main profile's graphics while
    /// playing another would change nothing in-game.
    GameConfig,
    /// The main `gameSettings.xml`'s mods folder setting: what "the game's folder" means, and
    /// what "Point FS25 here" writes. **Main only**: the main profile's setting must never be
    /// touched for a profile folder (a profile has its own `gameSettings.xml`).
    MainGameSettings,
    /// The default mods folder (`<user dir>/mods`), "Create it", the overlap fences between
    /// mods folders and the trusted roots before the first resolution. **Main only**: that
    /// folder stays a mods folder of its own, whichever profile is in use.
    DefaultModsFolder,
}

impl Place {
    /// True when this place reads the active FS25 profile's folder instead of the main one.
    pub fn follows_profile(self) -> bool {
        match self {
            Place::Log
            | Place::Bindings
            | Place::Savegames
            | Place::ModSettings
            | Place::GameConfig => true,
            Place::MainGameSettings | Place::DefaultModsFolder => false,
        }
    }
}

/// Pure: the user folder `place` reads, given the main FS25 user folder and the active profile.
pub fn resolve(place: Place, main: Option<PathBuf>, profile: Option<PathBuf>) -> Option<PathBuf> {
    match profile {
        Some(p) if place.follows_profile() => Some(p),
        _ => main,
    }
}

/// The user folder `place` reads right now (see [`Place`]).
pub fn user_dir(place: Place) -> Option<PathBuf> {
    resolve(place, crate::fsgame::user_dir(), active())
}

/// The `app_setting` key for "the log change time Silo last looked at", per user folder: the
/// main folder keeps the original key (so an upgrade doesn't re-announce an old crash), and a
/// profile gets its own, so switching folders never compares one log's time with another's.
pub fn log_seen_key(base: &str, profile_key: Option<&str>) -> String {
    match profile_key {
        Some(k) => format!("{base}:{k}"),
        None => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn covers(dir: &Path, main: &Path) -> bool {
        key_covers(&dir.to_string_lossy(), &main.to_string_lossy())
    }

    fn files(list: &[&str]) -> impl Fn(&Path) -> bool {
        let set: HashSet<PathBuf> = list.iter().map(PathBuf::from).collect();
        move |q: &Path| set.contains(q)
    }

    const MAIN: &str = "/docs/My Games/FarmingSimulator2025";

    #[test]
    fn a_mods_folder_inside_another_folder_is_a_candidate() {
        assert_eq!(
            candidate_dir(&p("/f/fs25 smoke/mods"), Some(&p(MAIN)), covers, |_| false),
            Some(p("/f/fs25 smoke"))
        );
        // Name check ignores case (Windows folders).
        assert_eq!(
            candidate_dir(&p("/f/prof/Mods"), Some(&p(MAIN)), covers, |_| false),
            Some(p("/f/prof"))
        );
        // No main folder known: still a candidate.
        assert_eq!(
            candidate_dir(&p("/f/prof/mods"), None, covers, |_| false),
            Some(p("/f/prof"))
        );
    }

    #[test]
    fn the_main_folder_a_non_mods_name_and_an_install_are_not_candidates() {
        let main_mods = Path::new(MAIN).join("mods");
        assert_eq!(
            candidate_dir(&main_mods, Some(&p(MAIN)), covers, |_| false),
            None
        );
        // FS25 reads `<profile>/mods`; a folder named otherwise isn't what a profile loads.
        assert_eq!(
            candidate_dir(&p("/f/prof/my mods"), Some(&p(MAIN)), covers, |_| false),
            None
        );
        // The install folder has a game.xml, but it's not a profile.
        let install = p("/steam/common/Farming Simulator 25");
        let is_install = |d: &Path| d == install;
        assert_eq!(
            candidate_dir(&install.join("mods"), Some(&p(MAIN)), covers, is_install),
            None
        );
        // A bare name or a root has no usable parent.
        assert_eq!(
            candidate_dir(&p("mods"), Some(&p(MAIN)), covers, |_| false),
            None
        );
        assert_eq!(
            candidate_dir(&p("/"), Some(&p(MAIN)), covers, |_| false),
            None
        );
        // A folder that holds the main user folder: its subtree would take in the main
        // gameSettings.xml, so it's never a profile.
        assert_eq!(
            candidate_dir(&p("/docs/mods"), Some(&p(MAIN)), covers, |_| false),
            None
        );
        assert_eq!(
            candidate_dir(&p("/docs/My Games/mods"), Some(&p(MAIN)), covers, |_| false),
            None
        );
        // A folder inside the main one, or beside it, is fine.
        assert_eq!(
            candidate_dir(
                &p("/docs/My Games/FarmingSimulator2025/test/mods"),
                Some(&p(MAIN)),
                covers,
                |_| false
            ),
            Some(p("/docs/My Games/FarmingSimulator2025/test"))
        );
        assert_eq!(
            candidate_dir(
                &p("/docs/My Games/FarmingSimulator2025b/mods"),
                Some(&p(MAIN)),
                covers,
                |_| false
            ),
            Some(p("/docs/My Games/FarmingSimulator2025b"))
        );
    }

    #[test]
    fn key_cover_is_by_whole_folder_names() {
        assert!(key_covers(
            r"c:\users\me\documents",
            r"c:\users\me\documents\my games\fs"
        ));
        assert!(key_covers(r"c:\", r"c:\users"));
        assert!(key_covers("/home/u", "/home/u"));
        assert!(!key_covers(r"c:\users\me\doc", r"c:\users\me\documents"));
        assert!(!key_covers(
            r"c:\users\me\documents\x",
            r"c:\users\me\documents"
        ));
    }

    #[test]
    fn detection_needs_game_xml_or_game_settings() {
        let dir = p("/f/prof");
        assert!(looks_like_profile(&dir, files(&["/f/prof/game.xml"])));
        assert!(looks_like_profile(
            &dir,
            files(&["/f/prof/gameSettings.xml"])
        ));
        assert!(!looks_like_profile(
            &dir,
            files(&["/f/prof/mods", "/f/prof/log.txt"])
        ));
        assert!(!looks_like_profile(&dir, |_| false));
    }

    #[test]
    fn install_detection() {
        let d = p("/steam/common/Farming Simulator 25");
        assert!(is_install_dir(
            &d,
            files(&["/steam/common/Farming Simulator 25/FarmingSimulator2025.exe"])
        ));
        assert!(is_install_dir(
            &d,
            files(&["/steam/common/Farming Simulator 25/dataS.gar"])
        ));
        assert!(!is_install_dir(
            &d,
            files(&["/steam/common/Farming Simulator 25/game.xml"])
        ));
    }

    #[test]
    fn a_stored_choice_wins_over_detection() {
        assert!(enabled(None, true, true));
        assert!(!enabled(None, false, true));
        assert!(enabled(Some(ON), false, true));
        assert!(!enabled(Some(OFF), true, true));
        // An unknown stored value counts as no choice.
        assert!(enabled(Some("maybe"), true, true));
        // Never on where it isn't supported, whatever was stored.
        assert!(!enabled(Some(ON), true, false));
        assert!(!enabled(None, true, false));
    }

    #[test]
    fn each_place_follows_the_profile_or_stays_main() {
        let main = Some(p(MAIN));
        let prof = Some(p("/f/prof"));
        for place in [
            Place::Log,
            Place::Bindings,
            Place::Savegames,
            Place::ModSettings,
            Place::GameConfig,
        ] {
            assert_eq!(
                resolve(place, main.clone(), prof.clone()),
                prof,
                "{place:?}"
            );
            // No profile in use: the main folder.
            assert_eq!(resolve(place, main.clone(), None), main, "{place:?}");
        }
        for place in [Place::MainGameSettings, Place::DefaultModsFolder] {
            assert_eq!(
                resolve(place, main.clone(), prof.clone()),
                main,
                "{place:?}"
            );
            assert_eq!(resolve(place, main.clone(), None), main, "{place:?}");
        }
        // A profile is used even when the main folder can't be found.
        assert_eq!(resolve(Place::Log, None, prof.clone()), prof);
        assert_eq!(resolve(Place::MainGameSettings, None, prof), None);
    }

    #[test]
    fn setting_and_log_keys() {
        assert_eq!(setting_key("f:\\prof\\mods"), "game_profile:f:\\prof\\mods");
        assert_eq!(log_seen_key("log_last_seen_ms", None), "log_last_seen_ms");
        assert_eq!(
            log_seen_key("log_last_seen_ms", Some("f:\\prof")),
            "log_last_seen_ms:f:\\prof"
        );
    }

    #[test]
    fn a_profile_folder_with_its_launch_off_still_belongs_to_the_profile() {
        let st = |enabled, detected| GameProfileStatus {
            dir: "/f/prof".into(),
            enabled,
            detected,
            chosen: false,
            supported: true,
            unsupported_reason: None,
        };
        assert!(belongs(&st(true, true)));
        assert!(belongs(&st(true, false)));
        // Turned off, or not supported here: still a profile's folder (main settings untouched).
        assert!(belongs(&st(false, true)));
        // Only named `mods`, never used by FS25, launch off: just a folder.
        assert!(!belongs(&st(false, false)));
    }

    #[test]
    fn profile_launch_works_on_windows_and_says_why_not_elsewhere() {
        assert_eq!(unsupported_reason(Os::Windows), None);
        // Each "not yet" names the platform and the part that isn't confirmed (#405).
        let mac = unsupported_reason(Os::MacOs).expect("macOS stays off until it's confirmed");
        assert!(
            mac.contains("macOS") && mac.contains("isn't confirmed that the Mac version"),
            "{mac}"
        );
        let linux = unsupported_reason(Os::Linux).expect("Linux stays off until it's confirmed");
        assert!(
            linux.contains("Linux") && linux.contains("isn't confirmed that Steam passes"),
            "{linux}"
        );
        assert!(unsupported_reason(Os::Other).is_some());
        // The constants follow the build's own platform.
        assert_eq!(SUPPORTED, unsupported_reason(Os::CURRENT).is_none());
        assert_eq!(UNSUPPORTED, unsupported_reason(Os::CURRENT));
        assert_eq!(SUPPORTED, cfg!(windows));
    }

    #[test]
    fn only_a_launching_profile_is_active() {
        // The one test that touches the published value.
        let prof = |launch| Published {
            dir: p("/f/prof"),
            launch,
            reads: Some(p("/f/prof/mods")),
        };
        publish(Some(prof(true)));
        assert_eq!(active(), Some(p("/f/prof")));
        publish(Some(prof(false)));
        assert_eq!(active(), None);
        assert_eq!(published(), Some(prof(false)));
        publish(None);
        assert_eq!(active(), None);
        assert_eq!(published(), None);
    }
}
