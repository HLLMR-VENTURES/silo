//! Detect the FS25 install and launch it. Because the active set is already
//! projected into the flat mods folder the game reads, "launch with this loadout"
//! is simply starting FS25 — via Steam when it's a Steam install, so it's tracked
//! properly. On macOS the game is a `.app` bundle and may also come from the Mac App
//! Store or GIANTS directly (installed in `/Applications`); those launch with `open -a`.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// FS25 on Steam.
const APP_ID: &str = "2300320";
const INSTALL_DIR: &str = "Farming Simulator 25";

#[cfg(windows)]
const EXE: &str = "FarmingSimulator2025.exe";
#[cfg(all(unix, not(target_os = "macos")))]
const EXE: &str = "FarmingSimulator2025";

/// Bundle id of the macOS build (the App Store sandbox container is named after it).
#[cfg(target_os = "macos")]
const MAC_BUNDLE_ID: &str = "com.giants-software.fs25";

/// Which store the detected install belongs to — decides how it's launched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    /// Inside a Steam library: launch via `steam://rungameid/<appid>`.
    Steam,
    /// Mac App Store / GIANTS-direct `.app`: launch the bundle itself.
    Standalone,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GameInfo {
    pub app_id: String,
    /// The executable — on macOS, the `.app` bundle.
    pub exe: String,
    pub install_dir: String,
    pub store: Store,
}

/// Cheap, name-only pre-filter: could a process with this name be FS25 (or a host for it)?
/// Anything this lets through is then judged by [`is_fs25_process`] with its command line.
fn is_candidate_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("farmingsimulator")
        || n.contains("farming simulator")
        || n.starts_with("dedicatedserver")
        || n.starts_with("wine")
        || n.contains("preloader")
}

/// True for an FS25 process: launcher/game, a Proton/Wine host running the game, or the
/// dedicated server shipped in an FS25 install. Deliberately conservative: a false positive only
/// blocks a mods move, a false negative would let Silo move files the game has open.
///
/// `name` is the process name (truncated to 15 chars on Linux), `cmd` its command line and `exe`
/// its executable path when known. Policy:
/// - name contains `farmingsimulator2025` / `farming simulator 25` => FS25;
/// - Linux-truncated name (`farmingsimulato…`) => FS25 if `cmd` or `exe` mentions the game;
/// - Wine host (`wine…` / `…preloader`) => FS25 if `cmd` mentions `farmingsimulator2025game`;
/// - `dedicatedServer…` => FS25 if `cmd` or `exe` points into an FS25 install, and also when we
///   cannot tell (empty `cmd` and no `exe`); a path that names something else (FS22) => no.
///
/// Anything else, including a launcher such as `steam` that merely mentions the game, is no.
/// Pure so it unit-tests without a running game.
pub fn is_fs25_process(name: &str, cmd: &[String], exe: Option<&str>) -> bool {
    let n = name.to_ascii_lowercase();
    let line = cmd.join(" ").to_ascii_lowercase();
    let exe = exe.map(str::to_ascii_lowercase);
    let game = |s: &str| s.contains("farmingsimulator2025") || s.contains("farming simulator 25");
    let mentions_game = || game(&line) || exe.as_deref().is_some_and(game);

    if game(&n) {
        return true;
    }
    if n.starts_with("farmingsimulato") {
        return mentions_game();
    }
    if n.starts_with("wine") || n.contains("preloader") {
        return line.contains("farmingsimulator2025game");
    }
    if n.starts_with("dedicatedserver") {
        return mentions_game() || (cmd.is_empty() && exe.is_none());
    }
    false
}

/// True if FS25 (or its dedicated server) is running right now. Used to refuse moving mods the
/// game may have open, so it runs before every mod of an archive move: it must stay cheap.
///
/// Two passes: a name-only snapshot of all processes, then — only for the few whose name could
/// be FS25 — the command line and exe path (a cross-process read per process on Windows).
pub fn fs25_running() -> bool {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let pids: Vec<Pid> = sys
        .processes()
        .iter()
        .filter(|(_, p)| is_candidate_name(&p.name().to_string_lossy()))
        .map(|(pid, _)| *pid)
        .collect();
    if pids.is_empty() {
        return false;
    }
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&pids),
        true,
        ProcessRefreshKind::nothing()
            .with_cmd(UpdateKind::Always)
            .with_exe(UpdateKind::OnlyIfNotSet),
    );
    pids.iter().filter_map(|pid| sys.process(*pid)).any(|p| {
        let cmd: Vec<String> = p
            .cmd()
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        let exe = p.exe().map(|e| e.to_string_lossy());
        is_fs25_process(&p.name().to_string_lossy(), &cmd, exe.as_deref())
    })
}

/// Candidate Steam install roots per platform.
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    {
        for p in ["C:/Program Files (x86)/Steam", "C:/Program Files/Steam"] {
            roots.push(PathBuf::from(p));
        }
    }
    #[cfg(target_os = "macos")]
    if let Some(h) = dirs::home_dir() {
        roots.push(h.join("Library/Application Support/Steam"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(h) = dirs::home_dir() {
        roots.push(h.join(".steam/steam"));
        roots.push(h.join(".local/share/Steam"));
    }
    roots
        .into_iter()
        .filter(|r| r.join("steamapps").is_dir())
        .collect()
}

/// Library paths from a Steam root's `libraryfolders.vdf` (plus the root itself).
fn library_paths(steam_root: &Path) -> Vec<PathBuf> {
    let mut paths = vec![steam_root.to_path_buf()];
    let vdf = steam_root.join("steamapps/libraryfolders.vdf");
    if let Ok(content) = std::fs::read_to_string(&vdf) {
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("\"path\"") {
                // `"path"    "E:\\SteamLibrary"` -> 4th quoted field.
                if let Some(val) = line.split('"').nth(3) {
                    paths.push(PathBuf::from(val.replace("\\\\", "/").replace('\\', "/")));
                }
            }
        }
    }
    paths
}

/// FS25 install folders (`steamapps/common/Farming Simulator 25`) across all Steam libraries.
fn steam_install_dirs() -> Vec<PathBuf> {
    steam_roots()
        .iter()
        .flat_map(|steam| library_paths(steam))
        .map(|lib| lib.join("steamapps/common").join(INSTALL_DIR))
        .collect()
}

/// Locate the FS25 install across all Steam libraries.
#[cfg(not(target_os = "macos"))]
pub fn detect() -> Option<GameInfo> {
    for install in steam_install_dirs() {
        let exe = install.join(EXE);
        if exe.is_file() {
            return Some(GameInfo {
                app_id: APP_ID.to_string(),
                exe: exe.to_string_lossy().into_owned(),
                install_dir: install.to_string_lossy().into_owned(),
                store: Store::Steam,
            });
        }
    }
    None
}

/// Locate the FS25 `.app` bundle: Steam libraries first, then `/Applications` and
/// `~/Applications` (App Store / GIANTS-direct), then Spotlight by bundle id as a last
/// resort (catches a bundle installed anywhere else).
#[cfg(target_os = "macos")]
pub fn detect() -> Option<GameInfo> {
    let steam_dirs = steam_install_dirs();
    let mut search = steam_dirs.clone();
    search.push(PathBuf::from("/Applications"));
    if let Some(h) = dirs::home_dir() {
        search.push(h.join("Applications"));
    }
    let bundles: Vec<PathBuf> = search.iter().flat_map(|d| app_bundles_in(d)).collect();
    let (app, store) = pick_mac_install(&steam_dirs, &bundles)
        .or_else(|| pick_mac_install(&steam_dirs, &spotlight_bundles()))?;
    let install_dir = match store {
        Store::Steam => app
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| app.clone()),
        Store::Standalone => app.clone(),
    };
    Some(GameInfo {
        app_id: APP_ID.to_string(),
        exe: app.to_string_lossy().into_owned(),
        install_dir: install_dir.to_string_lossy().into_owned(),
        store,
    })
}

/// Existing `*.app` directories directly inside `dir` (unreadable / missing → none).
#[cfg(target_os = "macos")]
fn app_bundles_in(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.flatten()
        .map(|e| e.path())
        .filter(|p| is_app_bundle(p) && p.is_dir())
        .collect()
}

/// Bundles Spotlight knows by FS25's bundle id. Silent on any failure (no Spotlight
/// index, `mdfind` missing) — this is only a fallback.
#[cfg(target_os = "macos")]
fn spotlight_bundles() -> Vec<PathBuf> {
    let query = format!("kMDItemCFBundleIdentifier == '{MAC_BUNDLE_ID}'");
    let Ok(out) = std::process::Command::new("mdfind").arg(query).output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(|l| PathBuf::from(l.trim()))
        .filter(|p| is_app_bundle(p) && p.is_dir())
        .collect()
}

/// `true` if `p` is named like a macOS app bundle (`*.app`).
#[cfg(any(target_os = "macos", test))]
fn is_app_bundle(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("app"))
}

/// `true` if a bundle's name looks like FS25 ("Farming Simulator 25.app",
/// "FarmingSimulator2025.app", ...): case and punctuation/spaces ignored.
#[cfg(any(target_os = "macos", test))]
fn is_fs25_bundle_name(p: &Path) -> bool {
    let Some(stem) = p.file_stem() else {
        return false;
    };
    let norm: String = stem
        .to_string_lossy()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect();
    norm.contains("farmingsimulator25") || norm.contains("farmingsimulator2025")
}

/// Pure choice of the macOS install from `bundles` (existing `*.app` paths gathered from
/// the Steam install folders, `/Applications`, or Spotlight), given the Steam FS25 install
/// folders. A bundle directly inside a Steam install folder is the Steam build — any `*.app`
/// counts there, since the exact bundle name Steam ships is unverified (an FS25-looking name
/// wins if there are several). Anywhere else the name must look like FS25, and it's a
/// standalone (App Store / GIANTS) install. Steam wins so a launch stays tracked by Steam.
#[cfg(any(target_os = "macos", test))]
fn pick_mac_install(steam_dirs: &[PathBuf], bundles: &[PathBuf]) -> Option<(PathBuf, Store)> {
    let in_steam = |b: &&PathBuf| {
        b.parent()
            .is_some_and(|parent| steam_dirs.iter().any(|d| d == parent))
    };
    let apps = || bundles.iter().filter(|b| is_app_bundle(b));
    let steam = apps()
        .filter(in_steam)
        .find(|b| is_fs25_bundle_name(b))
        .or_else(|| apps().find(in_steam));
    if let Some(b) = steam {
        return Some((b.clone(), Store::Steam));
    }
    apps()
        .find(|b| is_fs25_bundle_name(b))
        .map(|b| (b.clone(), Store::Standalone))
}

/// Launch FS25: through Steam (`steam://rungameid/<appid>`), or — for a macOS App Store /
/// GIANTS-direct install — by opening the detected `.app` bundle. Guided bisection uses
/// this same entry point.
pub fn launch() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    if let Some(info) = detect().filter(|i| i.store == Store::Standalone) {
        return open_mac_app(&info.exe);
    }
    let url = format!("steam://rungameid/{APP_ID}");
    open_url(&url)
}

/// `open -a <bundle>`: LaunchServices starts the app; we don't wait for it to exit.
#[cfg(target_os = "macos")]
fn open_mac_app(app: &str) -> Result<(), String> {
    std::process::Command::new("open")
        .args(["-a", app])
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn open_url(url: &str) -> Result<(), String> {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", "start", "", url]);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };

    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEAM: &str =
        "/Users/u/Library/Application Support/Steam/steamapps/common/Farming Simulator 25";

    fn p(s: &str) -> PathBuf {
        PathBuf::from(s)
    }

    fn in_steam(name: &str) -> PathBuf {
        Path::new(STEAM).join(name)
    }

    #[test]
    fn steam_bundle_with_any_name_is_steam() {
        let bundles = vec![in_steam("SomeUnexpectedName.app")];
        assert_eq!(
            pick_mac_install(&[p(STEAM)], &bundles),
            Some((bundles[0].clone(), Store::Steam))
        );
    }

    #[test]
    fn steam_prefers_fs25_named_bundle_and_ignores_non_bundles() {
        let bundles = vec![
            in_steam("Uninstaller.app"),
            in_steam("FarmingSimulator2025.app"),
            in_steam("data"),
        ];
        assert_eq!(
            pick_mac_install(&[p(STEAM)], &bundles),
            Some((bundles[1].clone(), Store::Steam))
        );
    }

    #[test]
    fn applications_bundle_is_standalone() {
        let bundles = vec![
            p("/Applications/Safari.app"),
            p("/Applications/Farming Simulator 25.app"),
        ];
        assert_eq!(
            pick_mac_install(&[p(STEAM)], &bundles),
            Some((bundles[1].clone(), Store::Standalone))
        );
    }

    #[test]
    fn unrelated_apps_outside_steam_are_ignored() {
        let bundles = vec![
            p("/Applications/Farming Simulator 22.app"),
            p("/Applications/Steam.app"),
        ];
        assert_eq!(pick_mac_install(&[p(STEAM)], &bundles), None);
        assert_eq!(pick_mac_install(&[], &[]), None);
    }

    #[test]
    fn steam_wins_over_standalone() {
        let bundles = vec![
            p("/Applications/FarmingSimulator2025.app"),
            in_steam("FarmingSimulator2025.app"),
        ];
        assert_eq!(
            pick_mac_install(&[p(STEAM)], &bundles),
            Some((bundles[1].clone(), Store::Steam))
        );
    }

    #[test]
    fn nested_bundle_in_steam_folder_is_not_the_steam_build() {
        // Only bundles directly in the install folder count as Steam; a nested one with an
        // unrelated name is ignored.
        let bundles = vec![Path::new(STEAM).join("sdk").join("Tool.app")];
        assert_eq!(pick_mac_install(&[p(STEAM)], &bundles), None);
    }

    #[test]
    fn fs25_bundle_names() {
        for yes in [
            "Farming Simulator 25.app",
            "FarmingSimulator2025.app",
            "farming-simulator-25.app",
            "Farming_Simulator_2025.APP",
        ] {
            assert!(is_fs25_bundle_name(&p(yes)), "{yes}");
        }
        for no in ["Farming Simulator 22.app", "Steam.app", "FarmingSim.app"] {
            assert!(!is_fs25_bundle_name(&p(no)), "{no}");
        }
        assert!(is_app_bundle(&p("/A/X.App")));
        assert!(!is_app_bundle(&p("/A/X.zip")));
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;

    const GAME_EXE: &str =
        "Z:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\x64\\FarmingSimulator2025Game.exe";

    #[test]
    fn matches_fs25_processes_on_every_platform() {
        // Windows
        assert!(is_fs25_process("FarmingSimulator2025.exe", &[], None));
        assert!(is_fs25_process("FarmingSimulator2025Game.exe", &[], None));
        assert!(is_fs25_process("farmingsimulator2025game.exe", &[], None));
        // Proton: Linux truncates comm to 15 chars; the command line carries the exe path.
        assert!(is_fs25_process("FarmingSimulato", &[GAME_EXE.into()], None));
        assert!(is_fs25_process(
            "FarmingSimulato",
            &[],
            Some("/home/u/.steam/Farming Simulator 25/FarmingSimulator2025")
        ));
        // macOS
        assert!(is_fs25_process("FarmingSimulator2025Game", &[], None));
        assert!(is_fs25_process("Farming Simulator 25", &[], None));
        // Dedicated server when it lives in an FS25 install.
        assert!(is_fs25_process(
            "dedicatedServer.exe",
            &[
                "E:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\dedicatedServer.exe"
                    .into()
            ],
            None
        ));
        // Linux: the command line is just the relative launcher, the exe path names the install.
        assert!(is_fs25_process(
            "dedicatedServer",
            &["./dedicatedServer".into()],
            Some("/opt/Farming Simulator 25/dedicatedServer")
        ));
        // Wine/Proton host process running the game.
        assert!(is_fs25_process(
            "wine64-preloader",
            &[GAME_EXE.into()],
            None
        ));
    }

    #[test]
    fn dedicated_server_we_cannot_identify_blocks_conservatively() {
        // Neither a command line nor an exe path: can't tell, so treat it as FS25.
        assert!(is_fs25_process("dedicatedServer.exe", &[], None));
    }

    #[test]
    fn ignores_unrelated_processes() {
        assert!(!is_fs25_process("silo.exe", &[], None));
        assert!(!is_fs25_process("FarmingSimulator2022.exe", &[], None));
        assert!(!is_fs25_process(
            "explorer.exe",
            &["C:\\Windows\\explorer.exe".into()],
            None
        ));
        // A dedicated server we can place elsewhere is not FS25.
        assert!(!is_fs25_process(
            "dedicatedServer.exe",
            &["C:\\OtherGame\\dedicatedServer.exe".into()],
            None
        ));
        assert!(!is_fs25_process(
            "dedicatedServer.exe",
            &[
                "E:\\SteamLibrary\\steamapps\\common\\Farming Simulator 22\\dedicatedServer.exe"
                    .into()
            ],
            None
        ));
        // A launcher that merely mentions the game on its command line is not the game.
        assert!(!is_fs25_process("steam.exe", &[GAME_EXE.into()], None));
    }

    /// Smoke test against the real process table (catches sysinfo API breakage). It only
    /// asserts that the call completes; the answer depends on whether FS25 happens to be running.
    #[test]
    #[ignore = "scans the live process table"]
    fn fs25_running_smoke() {
        let _ = fs25_running();
    }
}
