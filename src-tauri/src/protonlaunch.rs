//! Starting FS25 on a separate profile on Linux, where it runs through Steam's Proton (#417).
//!
//! There is no Linux build of the game: Steam starts the Windows game in a Proton prefix, so
//! Silo can only ask Steam, with `steam://run/<appid>//<args>/`. A tester confirmed on native
//! Steam (Linux Mint 22.3, Proton Experimental, 2026-11-14) that such a link reaches the game,
//! that Steam first asks to confirm the arguments, and that FS25 then uses the profile given as
//! `-profile "Z:\…"` (its log, settings and own `mods/` folder). `docs/CROSS-PLATFORM.md` has
//! the evidence and why each rule below is there.
//!
//! Everything here is pure (paths as `/`-separated strings, links passed in) so it unit-tests on
//! any OS; the one disk read is [`problem_now`].

use std::path::PathBuf;

use crate::modsroot::PathMap;

const APP_ID: &str = "2300320";

/// Where the Flatpak Steam keeps its files, inside the home folder. Its games run in a sandbox
/// that may not see the folder Silo sees, so Silo doesn't pass it a profile there.
const FLATPAK_STEAM: &str = "/.var/app/com.valvesoftware.Steam/";

/// Pure: the Proton prefix Steam starts the game in: the `compatdata` of the Steam library the
/// game is installed in (`<lib>/steamapps/common/<game>` → `<lib>/steamapps/compatdata/<id>/pfx`).
/// `None` when `install_dir` isn't shaped like a Steam library's game folder.
pub fn launch_prefix(install_dir: &str) -> Option<String> {
    let parts: Vec<&str> = install_dir.trim_end_matches('/').rsplitn(3, '/').collect();
    match parts.as_slice() {
        [game, "common", lib_apps]
            if !game.is_empty()
                && lib_apps.ends_with("/steamapps")
                && lib_apps.starts_with('/') =>
        {
            Some(format!("{lib_apps}/compatdata/{APP_ID}/pfx"))
        }
        _ => None,
    }
}

/// Pure: the profile folder `host_dir` (a full Linux path) as the game sees it, `Z:\…` (Proton
/// maps `Z:` to `/`; [`z_drive_problem`] checks it does). `Z:` even for a folder inside the
/// prefix: `C:\…` would mean another folder if Steam used a different prefix than Silo found.
///
/// The folder goes to the game in double quotes ([`steam_profile_url`]), so a name with `"`,
/// `\`, `$` or a backtick is refused: those are the characters that would split or change it
/// differently by the Windows rules and as shell words. So is a control character, the root
/// folder (`Z:\` would end in a backslash before the closing quote), and `.` / `..`.
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
        if *p == "." || *p == ".." {
            return Err(format!(
                "The profile folder's path has a `{p}` in it, so Silo can't pass it to the game: {host_dir}"
            ));
        }
        if p.chars()
            .any(|c| matches!(c, '"' | '\\' | '$' | '`') || c.is_control())
        {
            return Err(format!(
                "Silo can't pass this profile folder to the game through Steam: a folder name in its path has a \", \\, $ or ` in it ({host_dir}). Rename that folder to use the profile."
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

/// Pure: why the prefix's `Z:` drive doesn't reach `/`, if it doesn't. `link` is where
/// `<prefix>/dosdevices/z:` points (resolved), `None` when there's no such link: Proton then
/// creates it pointing at `/` when it starts the game.
pub fn z_drive_problem(link: Option<&str>) -> Option<String> {
    match link {
        None => None,
        Some(t) if t.trim_end_matches('/').is_empty() => None,
        Some(t) => Some(format!(
            "The game's Proton prefix maps drive Z: to {t} instead of /, so the game wouldn't find this profile folder. Silo can't start it with the profile."
        )),
    }
}

/// Pure: why Play can't start the game with a profile on this Linux setup, before looking at
/// the folder itself. `install_dir` is the Steam install Silo found (`None`: none), `z_link`
/// as for [`z_drive_problem`].
pub fn setup_problem(install_dir: Option<&str>, z_link: Option<&str>) -> Option<String> {
    let Some(install) = install_dir else {
        return Some(
            "Silo didn't find Farming Simulator 25 in a Steam library, so it can't start the game with a separate profile.".into(),
        );
    };
    if format!("{}/", install.trim_end_matches('/')).contains(FLATPAK_STEAM) {
        return Some(
            "Silo can't start Farming Simulator 25 with a separate profile from the Flatpak version of Steam yet: its games run in a sandbox that may not see this folder.".into(),
        );
    }
    if launch_prefix(install).is_none() {
        return Some(format!(
            "Silo can't tell which Proton prefix Steam starts the game in ({install}), so it can't start it with a separate profile."
        ));
    }
    z_drive_problem(z_link)
}

/// Pure: the prefix's drive map with Proton's defaults filled in: `Z:` → `/` and `C:` →
/// `<prefix>/drive_c` where the prefix has no such link yet (Proton creates them that way).
pub fn drive_map(prefix: &str, mut drives: Vec<(char, PathBuf)>) -> PathMap {
    if !drives.iter().any(|(l, _)| *l == 'z') {
        drives.push(('z', PathBuf::from("/")));
    }
    if !drives.iter().any(|(l, _)| *l == 'c') {
        drives.push(('c', PathBuf::from(format!("{prefix}/drive_c"))));
    }
    PathMap::new(drives)
}

/// Pure: a folder the game names in its drive letters (a profile's own mods folder setting,
/// `C:/…` or `Z:\…`) as a host path, through `map`. `None` when the drive isn't mapped, the
/// path has no drive letter, or it has a `..`: Wine stops `..` at the drive's top folder and a
/// host path doesn't, so the two could mean different folders.
pub fn host_path(map: &PathMap, game: &str) -> Option<PathBuf> {
    if game.split(['/', '\\']).any(|p| p == "..") {
        return None;
    }
    map.to_host(game)
}

/// The line under the switch while the profile launch is on, here.
pub const LAUNCH_NOTE: &str =
    "Steam asks you to confirm the launch options each time; choose Continue.";

/// Why Play can't start the game on profile `host_dir` here, if it can't: the setup
/// ([`setup_problem`]), then the folder ([`game_dir`]). Reads the install and the prefix's `Z:`
/// link from disk. Also gives the prefix's drive map for reading the profile's own settings.
#[cfg(target_os = "linux")]
pub fn problem_now(host_dir: &std::path::Path) -> Result<PathMap, String> {
    let install = crate::gamelaunch::detect().map(|i| i.install_dir);
    let prefix = install.as_deref().and_then(launch_prefix);
    let z_link = prefix.as_deref().and_then(|p| {
        let link = std::path::Path::new(p).join("dosdevices/z:");
        std::fs::read_link(&link).ok().map(|t| {
            let abs = if t.is_absolute() {
                t
            } else {
                link.parent().map(|d| d.join(&t)).unwrap_or(t)
            };
            std::fs::canonicalize(&abs)
                .unwrap_or(abs)
                .to_string_lossy()
                .into_owned()
        })
    });
    if let Some(why) = setup_problem(install.as_deref(), z_link.as_deref()) {
        return Err(why);
    }
    game_dir(&host_dir.to_string_lossy())?;
    let prefix = prefix.unwrap_or_default();
    let found = PathMap::from_prefix(std::path::Path::new(&prefix)).drives();
    Ok(drive_map(&prefix, found))
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
        assert_eq!(launch_prefix("/opt/fs25"), None);
        assert_eq!(launch_prefix("/steamapps/common/"), None);
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
        let url = steam_profile_url(r"Z:\home\ève\A&B #1?");
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
        assert_eq!(z_drive_problem(None), None);
        assert_eq!(z_drive_problem(Some("/")), None);
        assert!(z_drive_problem(Some("/home/evan"))
            .unwrap()
            .contains("/home/evan"));
    }

    #[test]
    fn setup_problems() {
        let native = "/home/evan/.local/share/Steam/steamapps/common/Farming Simulator 25";
        assert_eq!(setup_problem(Some(native), None), None);
        assert_eq!(setup_problem(Some(native), Some("/")), None);
        assert!(setup_problem(None, None).unwrap().contains("didn't find"));
        let flatpak = "/home/evan/.var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common/Farming Simulator 25";
        assert!(setup_problem(Some(flatpak), None)
            .unwrap()
            .contains("Flatpak"));
        assert!(setup_problem(Some("/opt/fs25"), None)
            .unwrap()
            .contains("Proton prefix"));
        assert!(setup_problem(Some(native), Some("/mnt"))
            .unwrap()
            .contains("Z:"));
    }

    #[test]
    fn a_profiles_own_mods_folder_reads_through_the_prefix() {
        let pfx = "/home/evan/.local/share/Steam/steamapps/compatdata/2300320/pfx";
        let map = drive_map(pfx, vec![('d', PathBuf::from("/mnt/data"))]);
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
        assert_eq!(host_path(&map, "Z:/home/evan/../../etc"), None);
        assert_eq!(host_path(&map, r"C:\..\x"), None);
        assert_eq!(host_path(&map, "E:/nowhere"), None);
        assert_eq!(host_path(&map, "mods"), None);
    }

    #[test]
    fn an_existing_drive_link_is_kept() {
        let map = drive_map("/p", vec![('z', PathBuf::from("/srv"))]);
        assert_eq!(
            host_path(&map, "Z:/x"),
            Some(PathBuf::from("/srv").join("x"))
        );
    }
}
