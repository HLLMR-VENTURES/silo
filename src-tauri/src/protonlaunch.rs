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
//! it unit-tests on any OS; the disk and process reads are in [`assess_now`] (Linux only).

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

/// Which Steam a `steam://` link opens, from the `Exec` line of the program that handles it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    Native,
    Flatpak,
    /// No handler, its `.desktop` file not found, or a command Silo doesn't recognize.
    Unknown,
}

/// Pure: which Steam the `.desktop` entry's `exec` command starts: the Flatpak one when it runs
/// `flatpak` or names the Flatpak app, the native one when the program is `steam` (or
/// `steam-runtime`, `steam-native`; any folder), else unknown.
pub fn classify_handler(exec: Option<&str>) -> Client {
    let Some(exec) = exec else {
        return Client::Unknown;
    };
    let low = exec.to_ascii_lowercase();
    if low.contains(FLATPAK_APP)
        || low
            .split_whitespace()
            .any(|w| w.trim_matches('"').ends_with("flatpak"))
    {
        return Client::Flatpak;
    }
    let program = low
        .split_whitespace()
        .map(|w| w.trim_matches('"'))
        .find(|w| *w != "env" && !w.contains('='))
        .unwrap_or("");
    match program.rsplit('/').next().unwrap_or("") {
        "steam" | "steam-runtime" | "steam-native" => Client::Native,
        _ => Client::Unknown,
    }
}

/// Pure: the `Exec=` command of a `.desktop` file's main entry.
pub fn desktop_exec(desktop: &str) -> Option<String> {
    let mut main = false;
    for line in desktop.lines().map(str::trim) {
        if line.starts_with('[') {
            main = line == "[Desktop Entry]";
        } else if main {
            if let Some(cmd) = line.strip_prefix("Exec=") {
                return Some(cmd.trim().to_string());
            }
        }
    }
    None
}

/// Pure: why Steam might not be the client that starts the game in the prefix Silo checked.
/// `listed_by_flatpak` says, for each Steam client folder whose libraries hold the game's
/// library, whether it's the Flatpak one. `handler` is which Steam opens `steam://` links
/// ([`classify_handler`]), `flatpak_installed` whether the Flatpak Steam is here.
pub fn client_problem(
    listed_by_flatpak: &[bool],
    handler: Client,
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
    match handler {
        Client::Flatpak => Some(
            "Steam links on this computer open the Flatpak version of Steam, which can't be given this profile folder yet. Silo can't start the game with it.".into(),
        ),
        Client::Native => None,
        Client::Unknown if flatpak_installed => Some(
            "Silo can't tell whether Steam links open the native or the Flatpak version of Steam, so it can't start the game with a separate profile.".into(),
        ),
        Client::Unknown => None,
    }
}

/// Pure: the `"installdir"` an `appmanifest_<id>.acf` names (the game's folder under
/// `steamapps/common`).
pub fn acf_installdir(acf: &str) -> Option<String> {
    acf.lines()
        .map(str::trim)
        .find(|l| l.starts_with("\"installdir\""))
        .and_then(|l| l.split('"').nth(3))
        .filter(|d| !d.is_empty() && !d.contains(['/', '\\']) && *d != "." && *d != "..")
        .map(str::to_string)
}

/// Pure: the game folder Steam has registered: the one library with an
/// `appmanifest_2300320.acf` (`registered`: each such library and the manifest's `installdir`),
/// so a leftover copy in another library isn't the one checked. Refused when no library or more
/// than one registers it.
pub fn registered_install(registered: &[(String, Option<String>)]) -> Result<String, String> {
    match registered {
        [] => Err(
            "Silo didn't find Farming Simulator 25 registered in a Steam library, so it can't start the game with a separate profile.".into(),
        ),
        [(lib, dir)] => Ok(format!(
            "{}/steamapps/common/{}",
            lib.trim_end_matches('/'),
            dir.as_deref().unwrap_or("Farming Simulator 25")
        )),
        _ => Err(
            "Farming Simulator 25 is registered in more than one Steam library, so Silo can't tell which one Steam starts. It can't start the game with a separate profile.".into(),
        ),
    }
}

/// Pure: the launch options a Steam `localconfig.vdf` holds for app `app_id` (any user block).
pub fn launch_options(vdf: &str, app_id: &str) -> Vec<String> {
    let quoted = format!("\"{app_id}\"");
    let mut out = Vec::new();
    // Depth of each open block, and the depth at which the app's block opened.
    let mut depth = 0usize;
    let mut app_depth: Option<usize> = None;
    let mut pending_app = false;
    for line in vdf.lines().map(str::trim) {
        match line {
            "{" => {
                depth += 1;
                if pending_app {
                    app_depth = Some(depth);
                }
                pending_app = false;
            }
            "}" => {
                if app_depth == Some(depth) {
                    app_depth = None;
                }
                depth = depth.saturating_sub(1);
            }
            _ => {
                pending_app = line == quoted;
                if app_depth == Some(depth) {
                    if let Some(rest) = line.strip_prefix("\"LaunchOptions\"") {
                        let v = rest.trim();
                        let v = v.strip_prefix('"').unwrap_or(v);
                        let v = v.strip_suffix('"').unwrap_or(v);
                        out.push(v.to_string());
                    }
                }
            }
        }
    }
    out
}

/// Pure: why the launch options would move the game to another prefix, if they would:
/// `STEAM_COMPAT_DATA_PATH` in them tells Proton to use that folder instead.
pub fn options_problem(options: &[String]) -> Option<String> {
    options
        .iter()
        .any(|o| o.contains("STEAM_COMPAT_DATA_PATH"))
        .then(|| {
            "Farming Simulator 25's Steam launch options set STEAM_COMPAT_DATA_PATH, so Silo can't tell which Proton prefix the game starts in. It can't start the game with a separate profile.".into()
        })
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
        return client_problem(&[true], Client::Unknown, true);
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
/// ([`wine_name_problem`]: no `..`, no trailing dot or space…). Wine also matches names
/// regardless of case and by short (8.3) names, so the caller only trusts a result that exists
/// under exactly this name (Wine takes an exact match first).
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

/// How long a look at the Steam setup ([`assess_now`]) is reused: one folder check asks for it
/// several times. Play always looks again.
#[cfg(target_os = "linux")]
const ASSESS_TTL: std::time::Duration = std::time::Duration::from_secs(10);

/// How long Silo waits for `xdg-mime` before taking the link handler as unknown.
#[cfg(target_os = "linux")]
const XDG_MIME_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// The last look at the Steam setup, and when it was taken.
#[cfg(target_os = "linux")]
#[allow(clippy::type_complexity)]
static SETUP: std::sync::Mutex<Option<(std::time::Instant, Result<PathMap, String>)>> =
    std::sync::Mutex::new(None);

/// Whether Play can start the game on profile `host_dir` here, and if so the prefix's drive map
/// (for reading the profile's own settings); else why not. Checks the setup ([`setup`]: the
/// registered install, which Steam client, launch options, the prefix's `Z:`), then the folder
/// ([`game_dir`]). `fresh` looks again; otherwise a look from the last [`ASSESS_TTL`] is reused.
#[cfg(target_os = "linux")]
pub fn assess_now(host_dir: &std::path::Path, fresh: bool) -> Result<PathMap, String> {
    let map = {
        let mut last = SETUP
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let recent = last
            .as_ref()
            .filter(|(at, _)| !fresh && at.elapsed() < ASSESS_TTL)
            .map(|(_, map)| map.clone());
        match recent {
            Some(map) => map,
            None => {
                let map = setup();
                *last = Some((std::time::Instant::now(), map.clone()));
                map
            }
        }
    }?;
    let host = host_dir.to_str().ok_or_else(|| {
        format!(
            "Silo can't pass this profile folder to the game: its path isn't valid text ({}).",
            host_dir.display()
        )
    })?;
    game_dir(host)?;
    Ok(map)
}

/// The program `xdg-mime` names for `steam://` links, waiting at most [`XDG_MIME_TIMEOUT`].
#[cfg(target_os = "linux")]
fn steam_link_handler() -> Option<String> {
    use std::io::Read;
    let mut child = std::process::Command::new("xdg-mime")
        .args(["query", "default", "x-scheme-handler/steam"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut out = String::new();
                child.stdout.take()?.read_to_string(&mut out).ok()?;
                let name = out.trim().to_string();
                return (status.success() && !name.is_empty()).then_some(name);
            }
            Ok(None) if start.elapsed() < XDG_MIME_TIMEOUT => {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
}

/// The `Exec=` command of the `.desktop` file `name`, looked up the way the desktop does: the
/// user's data folder, then the system's (`XDG_DATA_HOME`, `XDG_DATA_DIRS`).
#[cfg(target_os = "linux")]
fn handler_exec(name: &str) -> Option<String> {
    if name.contains('/') {
        return None;
    }
    let home = dirs::home_dir();
    let user = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".local/share")));
    let system = std::env::var("XDG_DATA_DIRS")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    user.into_iter()
        .chain(
            system
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        )
        .map(|d| d.join("applications").join(name))
        .find_map(|f| std::fs::read_to_string(f).ok())
        .and_then(|text| desktop_exec(&text))
}

/// One look at the Steam setup on disk: the registered install and its prefix, which Steam
/// client lists it and opens links, its launch options, and the prefix's drives.
#[cfg(target_os = "linux")]
fn setup() -> Result<PathMap, String> {
    use std::path::Path;
    fn canon(p: &Path) -> PathBuf {
        std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
    }
    fn is_flatpak(r: &Path) -> bool {
        format!("{}/", r.to_string_lossy()).contains(FLATPAK_STEAM)
    }
    let roots: Vec<PathBuf> = dirs::home_dir()
        .map(|h| crate::fsgame::linux_steam_roots(&h))
        .unwrap_or_default()
        .into_iter()
        .filter(|r| r.join("steamapps").is_dir())
        .collect();
    // Every library any client lists, once (by its real folder), with the clients listing it.
    let mut libs: Vec<(PathBuf, Vec<bool>)> = Vec::new();
    for r in &roots {
        for l in crate::gamelaunch::library_paths(r) {
            let c = canon(&l);
            match libs.iter_mut().find(|(p, _)| *p == c) {
                Some((_, by)) => by.push(is_flatpak(r)),
                None => libs.push((c, vec![is_flatpak(r)])),
            }
        }
    }
    let manifest = format!("steamapps/appmanifest_{APP_ID}.acf");
    let registered: Vec<(String, Option<String>)> = libs
        .iter()
        .filter_map(|(l, _)| {
            let acf = std::fs::read_to_string(l.join(&manifest)).ok()?;
            Some((l.to_string_lossy().into_owned(), acf_installdir(&acf)))
        })
        .collect();
    let install = registered_install(&registered)?;
    let lib = library_of(&install).map(|l| canon(Path::new(&l)));
    let listed: Vec<bool> = libs
        .iter()
        .find(|(p, _)| Some(p) == lib.as_ref())
        .map(|(_, by)| by.clone())
        .unwrap_or_default();
    let handler = steam_link_handler()
        .map(|name| classify_handler(handler_exec(&name).as_deref()))
        .unwrap_or(Client::Unknown);
    let flatpak_installed = roots.iter().any(|r| is_flatpak(r));
    let client = client_problem(&listed, handler, flatpak_installed).or_else(|| {
        let options: Vec<String> = roots
            .iter()
            .filter(|r| !is_flatpak(r))
            .filter_map(|r| std::fs::read_dir(r.join("userdata")).ok())
            .flat_map(|users| users.flatten())
            .filter_map(|u| std::fs::read_to_string(u.path().join("config/localconfig.vdf")).ok())
            .flat_map(|vdf| launch_options(&vdf, APP_ID))
            .collect();
        options_problem(&options)
    });
    let prefix = launch_prefix(&install);
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
    if let Some(why) = setup_problem(Some(&install), client, &z) {
        return Err(why);
    }
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
        use Client::*;
        // Listed by a native Steam, links open it (or nothing else could): fine.
        assert_eq!(client_problem(&[false], Native, true), None);
        assert_eq!(client_problem(&[false, false], Unknown, false), None);
        // A library the Flatpak Steam lists, wherever it is (an external drive too).
        assert!(client_problem(&[true], Native, true)
            .unwrap()
            .contains("Flatpak"));
        assert!(client_problem(&[false, true], Native, true)
            .unwrap()
            .contains("Flatpak"));
        // Links open the Flatpak Steam, though the native one has the game.
        assert!(client_problem(&[false], Flatpak, true)
            .unwrap()
            .contains("Flatpak"));
        // Both installed and the link handler unknown (or unrecognized): ambiguous.
        assert!(client_problem(&[false], Unknown, true)
            .unwrap()
            .contains("can't tell whether"));
        // No Steam lists the game's library.
        assert!(client_problem(&[], Native, false)
            .unwrap()
            .contains("which Steam"));
    }

    #[test]
    fn the_link_handler_is_judged_by_what_it_runs() {
        assert_eq!(
            classify_handler(Some("/usr/games/steam %U")),
            Client::Native
        );
        assert_eq!(
            classify_handler(Some("/usr/bin/steam-runtime %U")),
            Client::Native
        );
        assert_eq!(
            classify_handler(Some("env STEAM_X=1 steam %U")),
            Client::Native
        );
        assert_eq!(
            classify_handler(Some(
                "/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=/app/bin/steam-wrapper --file-forwarding com.valvesoftware.Steam @@u %U @@"
            )),
            Client::Flatpak
        );
        // A custom name doesn't make it native: what it runs does.
        assert_eq!(
            classify_handler(Some("flatpak run com.valvesoftware.Steam %U")),
            Client::Flatpak
        );
        assert_eq!(
            classify_handler(Some("/opt/my-launcher %U")),
            Client::Unknown
        );
        assert_eq!(classify_handler(None), Client::Unknown);
        let desktop = "[Desktop Entry]
Name=Steam
Exec=/usr/games/steam %U
[Desktop Action Store]
Exec=steam steam://store
";
        assert_eq!(
            desktop_exec(desktop).as_deref(),
            Some("/usr/games/steam %U")
        );
        assert_eq!(
            desktop_exec(
                "[Other]
Exec=x
"
            ),
            None
        );
    }

    #[test]
    fn the_install_is_the_one_steam_registered() {
        let acf = "\"AppState\"
{
	\"appid\"		\"2300320\"
	\"installdir\"		\"Farming Simulator 25\"
}
";
        assert_eq!(acf_installdir(acf).as_deref(), Some("Farming Simulator 25"));
        assert_eq!(acf_installdir("\"installdir\" \"../x\""), None);
        assert_eq!(
            registered_install(&[("/mnt/ext".into(), Some("Farming Simulator 25".into()))]),
            Ok("/mnt/ext/steamapps/common/Farming Simulator 25".into())
        );
        // A stale copy elsewhere has no manifest, so only the registered one counts; two
        // registrations can't be told apart.
        assert!(registered_install(&[]).is_err());
        assert!(registered_install(&[("/a".into(), None), ("/b".into(), None)]).is_err());
    }

    #[test]
    fn launch_options_that_move_the_prefix_are_refused() {
        let vdf = "\"UserLocalConfigStore\"
{
	\"Software\"
	{
		\"Valve\"
		{
			\"Steam\"
			{
				\"apps\"
				{
					\"2300320\"
					{
						\"LaunchOptions\"		\"STEAM_COMPAT_DATA_PATH=/x %command%\"
					}
					\"220\"
					{
						\"LaunchOptions\"		\"-other\"
					}
				}
			}
		}
	}
}
";
        assert_eq!(
            launch_options(vdf, "2300320"),
            vec!["STEAM_COMPAT_DATA_PATH=/x %command%".to_string()]
        );
        assert_eq!(launch_options(vdf, "220"), vec!["-other".to_string()]);
        assert!(options_problem(&launch_options(vdf, "2300320")).is_some());
        assert_eq!(options_problem(&["-profile x".into()]), None);
        assert_eq!(options_problem(&[]), None);
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
