//! Starting FS25 on a separate profile on Linux, where it runs through Steam's Proton (#417).
//!
//! There is no Linux build of the game: Steam starts the Windows game in a Proton prefix, so
//! Silo can only ask Steam, with `steam://run/<appid>//<args>/`. A tester confirmed on native
//! Steam (Linux Mint 22.3, Proton Experimental, 2026-11-14) that such a link reaches the game,
//! that Steam first asks to confirm the arguments, and that FS25 then uses the profile given as
//! `-profile "Z:\…"` (its log, settings and own `mods/` folder). `docs/CROSS-PLATFORM.md` has
//! the evidence and why each rule below is there.
//!
//! The rules all serve one aim: the folder Silo checks and manages is the folder the game is
//! given. Everything here is pure (paths as `/`-separated strings, links and lists passed in) so
//! it unit-tests on any OS; the disk and process reads are in [`assess_now`].

use std::path::PathBuf;

use crate::modsroot::PathMap;

const APP_ID: &str = "2300320";

/// Where the Flatpak Steam keeps its files, inside the home folder. Its games run in a sandbox
/// that may not see the folder Silo sees, so Silo doesn't pass it a profile.
const FLATPAK_STEAM: &str = "/.var/app/com.valvesoftware.Steam/";

/// The Flatpak Steam's application id, as it shows in its `.desktop` file name.
const FLATPAK_APP: &str = "com.valvesoftware.steam";

/// Pure: the Proton prefix Steam starts the game in: the `compatdata` of the Steam library the
/// game is installed in (`<lib>/steamapps/common/<game>` → `<lib>/steamapps/compatdata/<id>/pfx`).
/// `None` when `install_dir` isn't shaped like a Steam library's game folder.
pub fn launch_prefix(install_dir: &str) -> Option<String> {
    library_of(install_dir).map(|lib| format!("{lib}/steamapps/compatdata/{APP_ID}/pfx"))
}

/// Pure: the Steam library holding the game folder `install_dir`
/// (`<lib>/steamapps/common/<game>` → `<lib>`).
pub fn library_of(install_dir: &str) -> Option<String> {
    let parts: Vec<&str> = install_dir.trim_end_matches('/').rsplitn(3, '/').collect();
    match parts.as_slice() {
        [game, "common", lib_apps] if !game.is_empty() && lib_apps.starts_with('/') => lib_apps
            .strip_suffix("/steamapps")
            .filter(|lib| !lib.is_empty())
            .map(str::to_string),
        _ => None,
    }
}

/// Pure: whether a folder name would name another folder, or none, once Wine reads it as a
/// Windows name: a trailing dot or space (Wine drops them), a character Windows doesn't allow in
/// a name (`<>:"|?*`, a control character), `.` / `..`, or a device name (`CON`, `NUL`, `COM1`…,
/// with or without an extension).
fn wine_name_problem(name: &str) -> bool {
    const DEVICES: [&str; 4] = ["con", "prn", "aux", "nul"];
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_lowercase();
    let numbered =
        |p: &str| stem.len() == 4 && stem.starts_with(p) && stem.as_bytes()[3].is_ascii_digit();
    name.is_empty()
        || name == "."
        || name == ".."
        || name.ends_with('.')
        || name.ends_with(' ')
        || name
            .chars()
            .any(|c| matches!(c, '<' | '>' | ':' | '"' | '|' | '?' | '*') || c.is_control())
        || DEVICES.contains(&stem.as_str())
        || numbered("com")
        || numbered("lpt")
}

/// Pure: the profile folder `host_dir` (a full Linux path) as the game sees it, `Z:\…` (Proton
/// maps `Z:` to `/`; [`z_drive_problem`] checks it does). `Z:` even for a folder inside the
/// prefix: `C:\…` would mean another folder if Steam used a different prefix than Silo found.
///
/// The folder goes to the game in double quotes ([`steam_profile_url`]), so a name with `\`, `$`
/// or a backtick is refused, besides `"`: those would split or change it differently by the
/// Windows rules and as shell words. So is every name Wine would read as another folder
/// ([`wine_name_problem`]), and the root folder (`Z:\` would end in a backslash before the
/// closing quote).
pub fn game_dir(host_dir: &str) -> Result<String, String> {
    if !host_dir.starts_with('/') {
        return Err(format!(
            "The profile folder isn't a full path, so the game can't be given it: {host_dir}"
        ));
    }
    let parts: Vec<&str> = host_dir.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return Err(
            "Silo can't start Farming Simulator 25 with the whole disk as its profile.".into(),
        );
    }
    for p in &parts {
        if p.contains(['\\', '$', '`']) || wine_name_problem(p) {
            return Err(format!(
                "Silo can't pass this profile folder to the game through Steam: the folder name \"{p}\" in its path has a character Windows or the launch line can't keep, ends in a dot or a space, or is a reserved Windows name ({host_dir}). Rename that folder to use the profile."
            ));
        }
    }
    Ok(format!("Z:\\{}", parts.join("\\")))
}

/// Pure: `steam://run/<appid>//-profile "<game dir>"/`, with every character but letters,
/// digits and `-._~` percent-encoded: the form the tester's link had (`%20`, `%22`, `%3A`,
/// `%5C`), which Steam decoded back to `-profile "Z:\…"` and passed to the game. The folder is
/// always in quotes, so it stays one argument whether Steam splits the line by the Windows
/// rules or as shell words (inside double quotes neither treats these backslashes specially).
pub fn steam_profile_url(game_dir: &str) -> String {
    let args = format!("-profile \"{game_dir}\"");
    let mut enc = String::with_capacity(args.len() * 3);
    for b in args.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~') {
            enc.push(b as char);
        } else {
            enc.push_str(&format!("%{b:02X}"));
        }
    }
    format!("steam://run/{APP_ID}//{enc}/")
}

/// One drive entry of a Proton prefix, `<prefix>/dosdevices/<x>:`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drive {
    /// No entry: Proton creates the default one when it starts the game.
    Missing,
    /// A link, to this folder (resolved).
    Link(String),
    /// Something else (a folder, a broken link) or unreadable: what the game sees is unknown.
    Unusable(String),
}

/// Pure: why the prefix's `Z:` drive doesn't reach `/`, if it doesn't.
pub fn z_drive_problem(z: &Drive) -> Option<String> {
    match z {
        Drive::Missing => None,
        Drive::Link(t) if t.trim_end_matches('/').is_empty() => None,
        Drive::Link(t) => Some(format!(
            "The game's Proton prefix maps drive Z: to {t} instead of /, so the game wouldn't find this profile folder. Silo can't start it with the profile."
        )),
        Drive::Unusable(why) => Some(format!(
            "Silo can't tell where the game's Proton prefix puts drive Z: ({why}), so it can't start the game with this profile."
        )),
    }
}

/// Pure: why Steam might not be the client that starts the game in the prefix Silo checked.
/// `listed_by_flatpak` says, for each Steam client folder whose libraries hold the game's
/// library, whether it's the Flatpak one. `handler` is the program that opens `steam://` links
/// (its `.desktop` name; `None`: unknown), `flatpak_installed` whether the Flatpak Steam is here.
pub fn client_problem(
    listed_by_flatpak: &[bool],
    handler: Option<&str>,
    flatpak_installed: bool,
) -> Option<String> {
    const FLATPAK: &str = "Silo can't start Farming Simulator 25 with a separate profile through the Flatpak version of Steam yet: its games run in a sandbox that may not see this folder.";
    if listed_by_flatpak.is_empty() {
        return Some(
            "Silo can't tell which Steam installed Farming Simulator 25, so it can't start the game with a separate profile.".into(),
        );
    }
    if listed_by_flatpak.iter().any(|f| *f) {
        return Some(FLATPAK.into());
    }
    match handler.map(str::to_ascii_lowercase) {
        Some(h) if h.contains(FLATPAK_APP) => Some(
            "Steam links on this computer open the Flatpak version of Steam, which can't be given this profile folder yet. Silo can't start the game with it.".into(),
        ),
        Some(_) => None,
        None if flatpak_installed => Some(
            "Silo can't tell whether Steam links open the native or the Flatpak version of Steam, so it can't start the game with a separate profile.".into(),
        ),
        None => None,
    }
}

/// Pure: why Play can't start the game with a profile on this Linux setup, before looking at
/// the folder itself. `install_dir` is the Steam install Silo found (`None`: none), `client`
/// what [`client_problem`] said, `z` the prefix's `Z:` entry.
pub fn setup_problem(
    install_dir: Option<&str>,
    client: Option<String>,
    z: &Drive,
) -> Option<String> {
    let Some(install) = install_dir else {
        return Some(
            "Silo didn't find Farming Simulator 25 in a Steam library, so it can't start the game with a separate profile.".into(),
        );
    };
    if format!("{}/", install.trim_end_matches('/')).contains(FLATPAK_STEAM) {
        return client_problem(&[true], None, true);
    }
    if launch_prefix(install).is_none() {
        return Some(format!(
            "Silo can't tell which Proton prefix Steam starts the game in ({install}), so it can't start it with a separate profile."
        ));
    }
    client.or_else(|| z_drive_problem(z))
}

/// Pure: the prefix's drive map: the links found (`found`), with Proton's defaults for a drive
/// that has no entry yet (`Z:` → `/`, `C:` → `<prefix>/drive_c`, as Proton creates them). A `Z:`
/// or `C:` entry that's [`Drive::Unusable`] stays unmapped, so a path on it isn't guessed.
pub fn drive_map(prefix: &str, found: Vec<(char, PathBuf)>, z: &Drive, c: &Drive) -> PathMap {
    let unusable = |l: char| match l {
        'z' => matches!(z, Drive::Unusable(_)),
        'c' => matches!(c, Drive::Unusable(_)),
        _ => false,
    };
    let mut drives: Vec<(char, PathBuf)> =
        found.into_iter().filter(|(l, _)| !unusable(*l)).collect();
    if *z == Drive::Missing && !drives.iter().any(|(l, _)| *l == 'z') {
        drives.push(('z', PathBuf::from("/")));
    }
    if *c == Drive::Missing && !drives.iter().any(|(l, _)| *l == 'c') {
        drives.push(('c', PathBuf::from(format!("{prefix}/drive_c"))));
    }
    PathMap::new(drives)
}

/// Pure: a folder the game names in its drive letters (a profile's own mods folder setting,
/// `C:/…` or `Z:\…`) as a host path, through `map`. `None` unless it's a full path (a drive
/// letter, `:` and a separator: `C:mods` is relative to the game's working folder on `C:`) on a
/// mapped drive, and every name in it means the same folder to Wine and to Linux
/// ([`wine_name_problem`]: no `..`, no trailing dot or space…).
pub fn host_path(map: &PathMap, game: &str) -> Option<PathBuf> {
    let b = game.as_bytes();
    if b.len() < 3 || !b[0].is_ascii_alphabetic() || b[1] != b':' || !matches!(b[2], b'/' | b'\\') {
        return None;
    }
    // No drive known at all: nothing to translate through (an empty map passes paths as is).
    if map.drives().is_empty() {
        return None;
    }
    if game[3..]
        .split(['/', '\\'])
        .filter(|p| !p.is_empty())
        .any(wine_name_problem)
    {
        return None;
    }
    map.to_host(game)
}

/// The line under the switch while the profile launch is on, here.
pub const LAUNCH_NOTE: &str =
    "Steam asks you to confirm the launch options each time; choose Continue.";

/// Whether Play can start the game on profile `host_dir` here, and if so the prefix's drive map
/// (for reading the profile's own settings); else why not. Checks the setup
/// ([`setup_problem`]: the install, which Steam client, the prefix's `Z:`), then the folder
/// ([`game_dir`]). Reads the Steam folders and the prefix from disk and asks `xdg-mime` which
/// program opens `steam://` links.
#[cfg(target_os = "linux")]
pub fn assess_now(host_dir: &std::path::Path) -> Result<PathMap, String> {
    use std::path::Path;
    let install = crate::gamelaunch::detect().map(|i| i.install_dir);
    let prefix = install.as_deref().and_then(launch_prefix);
    let client = install.as_deref().and_then(library_of).map(|lib| {
        let canon = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let lib = canon(Path::new(&lib));
        let roots: Vec<PathBuf> = dirs::home_dir()
            .map(|h| crate::fsgame::linux_steam_roots(&h))
            .unwrap_or_default()
            .into_iter()
            .filter(|r| r.join("steamapps").is_dir())
            .collect();
        let is_flatpak = |r: &PathBuf| format!("{}/", r.to_string_lossy()).contains(FLATPAK_STEAM);
        let listed: Vec<bool> = roots
            .iter()
            .filter(|r| {
                crate::gamelaunch::library_paths(r)
                    .iter()
                    .any(|l| canon(l) == lib)
            })
            .map(is_flatpak)
            .collect();
        let handler = std::process::Command::new("xdg-mime")
            .args(["query", "default", "x-scheme-handler/steam"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
        let flatpak_installed = roots.iter().any(is_flatpak);
        client_problem(&listed, handler.as_deref(), flatpak_installed)
    });
    let read_drive = |letter: &str| -> Drive {
        let Some(p) = prefix.as_deref() else {
            return Drive::Missing;
        };
        let dd = Path::new(p).join("dosdevices");
        let entry = dd.join(letter);
        match std::fs::symlink_metadata(&entry) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Drive::Missing,
            Err(e) => Drive::Unusable(e.to_string()),
            Ok(m) if !m.file_type().is_symlink() => Drive::Unusable("it isn't a link".into()),
            Ok(_) => match std::fs::read_link(&entry) {
                Err(e) => Drive::Unusable(e.to_string()),
                Ok(t) => {
                    let abs = if t.is_absolute() { t } else { dd.join(t) };
                    match std::fs::canonicalize(&abs) {
                        Ok(r) => Drive::Link(r.to_string_lossy().into_owned()),
                        Err(e) => Drive::Unusable(format!("{} ({e})", abs.display())),
                    }
                }
            },
        }
    };
    let (z, c) = (read_drive("z:"), read_drive("c:"));
    let client = client.flatten();
    if let Some(why) = setup_problem(install.as_deref(), client, &z) {
        return Err(why);
    }
    let host = host_dir.to_str().ok_or_else(|| {
        format!(
            "Silo can't pass this profile folder to the game: its path isn't valid text ({}).",
            host_dir.display()
        )
    })?;
    game_dir(host)?;
    let prefix = prefix.unwrap_or_default();
    let found = PathMap::from_prefix(Path::new(&prefix)).drives();
    Ok(drive_map(&prefix, found, &z, &c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_prefix_is_the_compatdata_of_the_games_library() {
        assert_eq!(
            launch_prefix("/home/evan/.local/share/Steam/steamapps/common/Farming Simulator 25"),
            Some("/home/evan/.local/share/Steam/steamapps/compatdata/2300320/pfx".into())
        );
        assert_eq!(
            launch_prefix("/mnt/games/SteamLibrary/steamapps/common/Farming Simulator 25/"),
            Some("/mnt/games/SteamLibrary/steamapps/compatdata/2300320/pfx".into())
        );
        assert_eq!(
            library_of("/mnt/games/steamapps/common/Farming Simulator 25"),
            Some("/mnt/games".into())
        );
        assert_eq!(launch_prefix("/opt/fs25"), None);
        assert_eq!(launch_prefix("/steamapps/common/"), None);
        assert_eq!(launch_prefix("/steamapps/common/FS25"), None);
        assert_eq!(launch_prefix("relative/steamapps/common/FS25"), None);
    }

    #[test]
    fn the_folder_becomes_a_z_drive_path() {
        assert_eq!(
            game_dir("/home/evan/FS25 Profiles/smoke").unwrap(),
            r"Z:\home\evan\FS25 Profiles\smoke"
        );
        assert_eq!(
            game_dir("/home/evan/fs25//smoke/").unwrap(),
            r"Z:\home\evan\fs25\smoke"
        );
        assert_eq!(game_dir("/home/ève/Fermes").unwrap(), r"Z:\home\ève\Fermes");
        assert_eq!(
            game_dir("/home/evan/v1.2 farm").unwrap(),
            r"Z:\home\evan\v1.2 farm"
        );
        assert_eq!(
            game_dir("/home/evan/console").unwrap(),
            r"Z:\home\evan\console"
        );
    }

    #[test]
    fn folders_that_would_split_or_change_are_refused() {
        for bad in [
            "/home/evan/a\"b",
            r"/home/evan/a\b",
            "/home/evan/$HOME",
            "/home/evan/a`id`",
            "/home/evan/a\nb",
            "/home/evan/../root",
            "/home/./evan",
            // Wine would read these as another folder, or as none.
            "/profiles/Farm./slot",
            "/profiles/Farm /slot",
            "/profiles/a:b",
            "/profiles/a*b",
            "/profiles/what?",
            "/profiles/a|b",
            "/profiles/<x>",
            "/profiles/NUL",
            "/profiles/con.txt",
            "/profiles/COM1",
            "/profiles/lpt9.farm",
            "/",
            "home/evan",
        ] {
            assert!(game_dir(bad).is_err(), "{bad:?} should be refused");
        }
    }

    #[test]
    fn the_url_is_the_one_the_tester_proved() {
        // The exact link the #417 tester opened with xdg-open, which Steam passed to the game.
        assert_eq!(
            steam_profile_url(r"Z:\home\evan\FS25 Profiles\smoke"),
            "steam://run/2300320//-profile%20%22Z%3A%5Chome%5Cevan%5CFS25%20Profiles%5Csmoke%22/"
        );
        let url = steam_profile_url(r"Z:\home\ève\A&B #1");
        assert!(url.starts_with("steam://run/2300320//-profile%20%22Z%3A%5Chome%5C%C3%A8ve%5C"));
        let tail = &url["steam://run/2300320//".len()..url.len() - 1];
        assert!(
            tail.bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-._~%".contains(&b)),
            "{tail}"
        );
    }

    #[test]
    fn z_must_reach_the_root_or_be_left_to_proton() {
        assert_eq!(z_drive_problem(&Drive::Missing), None);
        assert_eq!(z_drive_problem(&Drive::Link("/".into())), None);
        assert!(z_drive_problem(&Drive::Link("/home/evan".into()))
            .unwrap()
            .contains("/home/evan"));
        // A folder (not a link), a broken link or an unreadable entry: not guessed to be `/`.
        assert!(z_drive_problem(&Drive::Unusable("it isn't a link".into()))
            .unwrap()
            .contains("it isn't a link"));
    }

    #[test]
    fn only_the_native_steam_that_opens_steam_links_is_trusted() {
        // Listed by a native Steam, links open it (or nothing else could): fine.
        assert_eq!(client_problem(&[false], Some("steam.desktop"), true), None);
        assert_eq!(client_problem(&[false, false], None, false), None);
        // A library the Flatpak Steam lists, wherever it is (an external drive too).
        assert!(client_problem(&[true], Some("steam.desktop"), true)
            .unwrap()
            .contains("Flatpak"));
        assert!(client_problem(&[false, true], Some("steam.desktop"), true)
            .unwrap()
            .contains("Flatpak"));
        // Links open the Flatpak Steam, though the native one has the game.
        assert!(
            client_problem(&[false], Some("com.valvesoftware.Steam.desktop"), true)
                .unwrap()
                .contains("Flatpak")
        );
        // Both installed and the link handler unknown: ambiguous.
        assert!(client_problem(&[false], None, true)
            .unwrap()
            .contains("can't tell whether"));
        // No Steam lists the game's library.
        assert!(client_problem(&[], Some("steam.desktop"), false)
            .unwrap()
            .contains("which Steam"));
    }

    #[test]
    fn setup_problems() {
        let native = "/home/evan/.local/share/Steam/steamapps/common/Farming Simulator 25";
        assert_eq!(setup_problem(Some(native), None, &Drive::Missing), None);
        assert_eq!(
            setup_problem(Some(native), None, &Drive::Link("/".into())),
            None
        );
        assert!(setup_problem(None, None, &Drive::Missing)
            .unwrap()
            .contains("didn't find"));
        let flatpak = "/home/evan/.var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/Farming Simulator 25";
        assert!(setup_problem(Some(flatpak), None, &Drive::Missing)
            .unwrap()
            .contains("Flatpak"));
        assert!(setup_problem(Some("/opt/fs25"), None, &Drive::Missing)
            .unwrap()
            .contains("Proton prefix"));
        assert!(
            setup_problem(Some(native), Some("client".into()), &Drive::Missing)
                .unwrap()
                .contains("client")
        );
        assert!(
            setup_problem(Some(native), None, &Drive::Link("/mnt".into()))
                .unwrap()
                .contains("Z:")
        );
    }

    #[test]
    fn a_profiles_own_mods_folder_reads_through_the_prefix() {
        let pfx = "/home/evan/.local/share/Steam/steamapps/compatdata/2300320/pfx";
        let map = drive_map(
            pfx,
            vec![('d', PathBuf::from("/mnt/data"))],
            &Drive::Missing,
            &Drive::Missing,
        );
        assert_eq!(
            host_path(&map, "Z:/home/evan/other mods"),
            Some(PathBuf::from("/").join("home/evan/other mods"))
        );
        assert_eq!(
            host_path(&map, r"C:\users\steamuser\mods"),
            Some(PathBuf::from(format!("{pfx}/drive_c")).join("users/steamuser/mods"))
        );
        assert_eq!(
            host_path(&map, "D:/fs25/mods"),
            Some(PathBuf::from("/mnt/data").join("fs25/mods"))
        );
        for bad in [
            "Z:/home/evan/../../etc",
            r"C:\..\x",
            "E:/nowhere",
            "mods",
            // Drive-relative: relative to the game's working folder on that drive.
            "C:mods",
            "C:",
            // Names Wine reads as another folder.
            "Z:/home/evan/Farm./mods",
            "Z:/home/evan/mods ",
            "Z:/home/NUL/mods",
        ] {
            assert_eq!(host_path(&map, bad), None, "{bad:?}");
        }
    }

    #[test]
    fn only_missing_drives_get_protons_defaults() {
        // An existing link is kept.
        let map = drive_map(
            "/p",
            vec![('z', PathBuf::from("/srv"))],
            &Drive::Link("/srv".into()),
            &Drive::Missing,
        );
        assert_eq!(
            host_path(&map, "Z:/x"),
            Some(PathBuf::from("/srv").join("x"))
        );
        // An unusable `C:` (a folder, a broken link) isn't guessed: paths on it aren't mapped,
        // and a found entry for it is dropped.
        let map = drive_map(
            "/p",
            vec![('c', PathBuf::from("/p/gone"))],
            &Drive::Missing,
            &Drive::Unusable("broken".into()),
        );
        assert_eq!(host_path(&map, "C:/users/mods"), None);
        assert_eq!(host_path(&map, "Z:/x"), Some(PathBuf::from("/").join("x")));
        let map = drive_map(
            "/p",
            vec![],
            &Drive::Unusable("dir".into()),
            &Drive::Missing,
        );
        assert_eq!(host_path(&map, "Z:/x"), None);
    }
}
