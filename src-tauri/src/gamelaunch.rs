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

/// Game executable names inside the install folder, in preference order.
#[cfg(windows)]
const EXES: &[&str] = &[EXE_WINDOWS];
/// FS25 has no native Linux build: a Steam install run through Proton holds the Windows
/// `.exe`. The extensionless name is kept in case a native build ever ships.
#[cfg(all(unix, not(target_os = "macos")))]
const EXES: &[&str] = &[EXE_WINDOWS, "FarmingSimulator2025"];
#[cfg(any(not(target_os = "macos"), test))]
const EXE_WINDOWS: &str = "FarmingSimulator2025.exe";

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
/// `farmingsimulato` (no final r) on purpose: Linux truncates a process name to 15 characters,
/// so the game under Proton is reported as `FarmingSimulato`.
fn is_candidate_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.contains("farmingsimulato")
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

/// Candidate Steam install roots per platform (those with a `steamapps` folder).
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    roots.extend(windows_steam_roots(registry_steam_path()));
    #[cfg(target_os = "macos")]
    if let Some(h) = dirs::home_dir() {
        roots.push(h.join("Library/Application Support/Steam"));
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    if let Some(h) = dirs::home_dir() {
        roots.extend(crate::fsgame::linux_steam_roots(&h));
    }
    roots
        .into_iter()
        .filter(|r| r.join("steamapps").is_dir())
        .collect()
}

/// The Steam client folder Windows' registry names (`HKCU\Software\Valve\Steam\SteamPath`,
/// written by the Steam client, e.g. `d:/steam`). `None` when Steam never ran for this user.
#[cfg(windows)]
fn registry_steam_path() -> Option<PathBuf> {
    let path = windows_registry::CURRENT_USER
        .open(r"Software\Valve\Steam")
        .and_then(|k| k.get_string("SteamPath"))
        .ok()?;
    let path = path.trim();
    (!path.is_empty()).then(|| PathBuf::from(path))
}

/// Windows Steam client roots: the registry's path first (a client on any drive), then the two
/// C: defaults. A default that is the registry's path again (case and slashes aside) is dropped.
#[cfg(any(windows, test))]
fn windows_steam_roots(registry: Option<PathBuf>) -> Vec<PathBuf> {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let mut roots: Vec<PathBuf> = registry.into_iter().collect();
    for d in ["C:/Program Files (x86)/Steam", "C:/Program Files/Steam"] {
        let d = PathBuf::from(d);
        if !roots.iter().any(|r| norm(r) == norm(&d)) {
            roots.push(d);
        }
    }
    roots
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
    steam_install_dirs().into_iter().find_map(|install| {
        let exe = find_game_exe(&install, EXES, |p| p.is_file())?;
        Some(GameInfo {
            app_id: APP_ID.to_string(),
            exe: exe.to_string_lossy().into_owned(),
            install_dir: install.to_string_lossy().into_owned(),
            store: Store::Steam,
        })
    })
}

/// The first of `names` that is a file directly in `install`. `is_file` is injected so this
/// unit-tests without a real install.
#[cfg(any(not(target_os = "macos"), test))]
fn find_game_exe(
    install: &Path,
    names: &[&str],
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    names.iter().map(|n| install.join(n)).find(|p| is_file(p))
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
///
/// With `profile` (an FS25 profile folder, #387, Windows only: see `gameprofile::SUPPORTED`)
/// the game executable is started directly with `-profile "<dir>"`, so FS25 uses that folder's
/// mods, settings, savegames and log. A hands-on test (2026-11-01) confirmed this loads the
/// profile with Steam running. If the executable can't be found or started, the Steam URL with
/// the same arguments is the fallback (`steam://run/<id>//<args>/`, Valve's documented form;
/// Steam asks the user to confirm launch arguments). Never the plain `rungameid` URL: that
/// would start the main profile, with the wrong mods.
pub fn launch(profile: Option<&Path>) -> Result<(), String> {
    if let Some(dir) = profile {
        return launch_profile(dir);
    }
    #[cfg(target_os = "macos")]
    if let Some(info) = detect().filter(|i| i.store == Store::Standalone) {
        return open_mac_app(&info.exe);
    }
    let url = format!("steam://rungameid/{APP_ID}");
    open_url(&url)
}

/// How a profile launch starts the game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchPlan {
    /// Start `program` with `args`, in `cwd` (the install folder).
    Exe {
        program: PathBuf,
        args: Vec<String>,
        cwd: PathBuf,
    },
    /// Open this Steam URL (the fallback).
    SteamUrl(String),
}

/// The arguments that start FS25 on the profile folder `dir`. One argument per item: the
/// folder is a single argument however many spaces it has (see [`windows_quote`]).
pub fn profile_args(dir: &Path) -> Vec<String> {
    vec!["-profile".to_string(), dir.to_string_lossy().into_owned()]
}

/// Pure: the plan for starting FS25 on profile `dir`, given the detected install. A Steam
/// install with a game executable: run it directly. Otherwise the Steam URL fallback.
pub fn profile_launch_plan(info: Option<&GameInfo>, dir: &Path) -> LaunchPlan {
    match info {
        Some(i) if i.store == Store::Steam && !i.exe.is_empty() => LaunchPlan::Exe {
            program: PathBuf::from(&i.exe),
            args: profile_args(dir),
            cwd: PathBuf::from(&i.install_dir),
        },
        _ => LaunchPlan::SteamUrl(steam_profile_url(dir)),
    }
}

/// `steam://run/<appid>//-profile "<dir>"/`, percent-encoded. Valve documents
/// `steam://run/<id>//<args>/` with "args are passed to the application as launch parameters".
/// The arguments are first quoted the way a Windows command line quotes them (as typed into
/// Steam's launch options), then every character but letters, digits, `-._~`, `\` and `:` is
/// percent-encoded, the way a browser hands such a link to Steam. So the URL has no space or
/// quote that could split it where Windows puts it on Steam's command line (`"%1"`), and nothing
/// a shell would read. It is opened by the system URL handler, never through a shell (see
/// `open_url_safely`).
pub fn steam_profile_url(dir: &Path) -> String {
    format!(
        "steam://run/{APP_ID}//{}/",
        percent_encode_arg(&windows_command_line(&profile_args(dir)))
    )
}

/// Percent-encode `s` (UTF-8) except unreserved characters and the `\` / `:` of a Windows path.
fn percent_encode_arg(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'\\' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// One argument quoted the way Windows programs split their command line (the Microsoft C
/// runtime rules that `CommandLineToArgvW` follows): left alone when it isn't empty and has no
/// space, tab, newline or quote; otherwise wrapped in quotes, each quote escaped as `\"`, and
/// the backslashes right before a quote (or before the closing quote) doubled. A folder path
/// therefore arrives as exactly one argument, and nothing in it can start another one.
pub fn windows_quote(arg: &str) -> String {
    if !arg.is_empty() && !arg.contains([' ', '\t', '\n', '\x0b', '"']) {
        return arg.to_string();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    let mut backslashes = 0usize;
    for c in arg.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                out.push_str(&"\\".repeat(backslashes * 2 + 1));
                out.push('"');
                backslashes = 0;
            }
            _ => {
                out.push_str(&"\\".repeat(backslashes));
                out.push(c);
                backslashes = 0;
            }
        }
    }
    out.push_str(&"\\".repeat(backslashes * 2));
    out.push('"');
    out
}

/// The arguments joined into a Windows command line tail, each quoted by [`windows_quote`].
pub fn windows_command_line(args: &[String]) -> String {
    args.iter()
        .map(|a| windows_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// How long after a profile launch another one is refused: the new process can take a moment to
/// show up in the process list, and Steam no longer stands between two clicks on Play.
pub const PROFILE_LAUNCH_HOLD: std::time::Duration = std::time::Duration::from_secs(10);

pub const ALREADY_RUNNING: &str = "Farming Simulator 25 is already running.";
pub const JUST_STARTED: &str = "Farming Simulator 25 is starting. Give it a moment.";

/// When the last profile launch was started (one at a time: the lock is held through the spawn).
static LAST_PROFILE_LAUNCH: std::sync::Mutex<Option<std::time::Instant>> =
    std::sync::Mutex::new(None);

/// True for [`PROFILE_LAUNCH_HOLD`] after a profile launch: the game may be starting without
/// showing in the process list yet, so writes to its files wait as they do while it runs.
pub fn profile_launch_pending() -> bool {
    LAST_PROFILE_LAUNCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_some_and(|t| t.elapsed() < PROFILE_LAUNCH_HOLD)
}

/// Pure: why a profile launch must not start now. The game started directly (not through Steam)
/// could otherwise run twice: when it's already running, or when it was started `since` ago,
/// within [`PROFILE_LAUNCH_HOLD`], and may not be in the process list yet.
pub fn profile_launch_refusal(
    running: bool,
    since: Option<std::time::Duration>,
) -> Option<&'static str> {
    if running {
        return Some(ALREADY_RUNNING);
    }
    since
        .filter(|d| *d < PROFILE_LAUNCH_HOLD)
        .map(|_| JUST_STARTED)
}

/// Start FS25 on profile `dir`. Refuses a folder that isn't there (an unplugged drive): FS25
/// would start a new, empty profile there instead of the user's setup. One launch at a time,
/// checked against the process list right before the spawn ([`profile_launch_refusal`]).
fn launch_profile(dir: &Path) -> Result<(), String> {
    if !crate::gameprofile::SUPPORTED {
        return Err(
            "Launching Farming Simulator 25 with a separate profile works on Windows only for now."
                .into(),
        );
    }
    if !dir.is_dir() {
        return Err(format!(
            "This mods folder's Farming Simulator 25 profile isn't available: {}",
            dir.display()
        ));
    }
    let mut last = LAST_PROFILE_LAUNCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(why) = profile_launch_refusal(fs25_running(), last.map(|t| t.elapsed())) {
        return Err(why.into());
    }
    let started = launch_profile_now(dir);
    if started.is_ok() {
        *last = Some(std::time::Instant::now());
    }
    started
}

fn launch_profile_now(dir: &Path) -> Result<(), String> {
    match profile_launch_plan(detect().as_ref(), dir) {
        LaunchPlan::Exe { program, args, cwd } => match spawn_exe(&program, &args, &cwd) {
            Ok(()) => Ok(()),
            Err(e) => {
                log::warn!(
                    "couldn't start {} directly ({e}); asking Steam instead",
                    program.display()
                );
                open_url_safely(&steam_profile_url(dir))
            }
        },
        LaunchPlan::SteamUrl(url) => open_url_safely(&url),
    }
}

/// Start the game executable with `args`, not through a shell. On Windows each argument goes on
/// the command line exactly as [`windows_quote`] writes it (`raw_arg`), so what runs is what the
/// unit tests check.
fn spawn_exe(program: &Path, args: &[String], cwd: &Path) -> Result<(), String> {
    let mut cmd = std::process::Command::new(program);
    cmd.current_dir(cwd);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        for a in args {
            cmd.raw_arg(windows_quote(a));
        }
    }
    #[cfg(not(windows))]
    cmd.args(args);
    cmd.spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// Open a URL that carries a folder path with the system URL handler, without a shell:
/// `cmd /C start` would read `&`, `|`, `^` or `%` in a folder name as shell syntax.
/// (`tauri-plugin-opener` hands the URL to `Start-Process` in an environment variable.)
fn open_url_safely(url: &str) -> Result<(), String> {
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
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

    /// Splits a Windows command line tail the way the Microsoft C runtime does
    /// (`CommandLineToArgvW` rules for arguments after the program name): the reference the
    /// quoting is checked against.
    fn split_windows_args(line: &str) -> Vec<String> {
        let mut args = Vec::new();
        let mut cur = String::new();
        let mut in_arg = false;
        let mut quoted = false;
        let chars: Vec<char> = line.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' {
                let start = i;
                while i < chars.len() && chars[i] == '\\' {
                    i += 1;
                }
                let n = i - start;
                if i < chars.len() && chars[i] == '"' {
                    cur.push_str(&"\\".repeat(n / 2));
                    if n % 2 == 1 {
                        cur.push('"');
                        i += 1;
                    }
                } else {
                    cur.push_str(&"\\".repeat(n));
                }
                in_arg = true;
                continue;
            }
            if c == '"' {
                if quoted && chars.get(i + 1) == Some(&'"') {
                    cur.push('"');
                    i += 2;
                    continue;
                }
                quoted = !quoted;
                in_arg = true;
            } else if (c == ' ' || c == '\t') && !quoted {
                if in_arg {
                    args.push(std::mem::take(&mut cur));
                    in_arg = false;
                }
            } else {
                cur.push(c);
                in_arg = true;
            }
            i += 1;
        }
        if in_arg {
            args.push(cur);
        }
        args
    }

    #[test]
    fn windows_quote_leaves_plain_args_and_quotes_spaces() {
        assert_eq!(windows_quote("-profile"), "-profile");
        assert_eq!(windows_quote(r"F:\fs25-silo-smoke"), r"F:\fs25-silo-smoke");
        assert_eq!(
            windows_quote(r"F:\FS25 Profiles\Seasons run"),
            r#""F:\FS25 Profiles\Seasons run""#
        );
        assert_eq!(windows_quote(""), r#""""#);
        // A trailing backslash would escape the closing quote: it's doubled.
        assert_eq!(windows_quote(r"C:\My Games\"), r#""C:\My Games\\""#);
        // A quote can't be in a Windows path, but if one came it stays inside the argument.
        assert_eq!(windows_quote(r#"a "b"#), r#""a \"b""#);
        assert_eq!(windows_quote(r#"a\"b"#), r#""a\\\"b""#);
    }

    #[test]
    fn profile_args_split_back_to_exactly_two_arguments() {
        for dir in [
            r"F:\fs25-silo-smoke",
            r"F:\FS25 Profiles\Seasons run",
            r"C:\Users\Jo Doe\Documents\FS25 & Maps (test)\prof",
            r"D:\x\  two  spaces\",
            r#"D:\odd" -profile "C:\other"#,
            r"\\nas\share\FS 25\prof",
            "D:\\tab\there",
        ] {
            let args = profile_args(Path::new(dir));
            let line = windows_command_line(&args);
            assert_eq!(
                split_windows_args(&line),
                vec!["-profile".to_string(), dir.to_string()],
                "{line}"
            );
        }
    }

    fn steam_game(exe: &str, install: &str) -> GameInfo {
        GameInfo {
            app_id: APP_ID.into(),
            exe: exe.into(),
            install_dir: install.into(),
            store: Store::Steam,
        }
    }

    #[test]
    fn profile_launch_runs_the_exe_with_the_profile() {
        let install = r"E:\SteamLibrary\steamapps\common\Farming Simulator 25";
        let exe = format!(r"{install}\FarmingSimulator2025.exe");
        let dir = Path::new(r"F:\FS25 Profiles\smoke");
        assert_eq!(
            profile_launch_plan(Some(&steam_game(&exe, install)), dir),
            LaunchPlan::Exe {
                program: PathBuf::from(&exe),
                args: vec!["-profile".into(), r"F:\FS25 Profiles\smoke".into()],
                cwd: PathBuf::from(install),
            }
        );
    }

    #[test]
    fn a_profile_launch_never_starts_the_game_twice() {
        use std::time::Duration;
        assert_eq!(profile_launch_refusal(false, None), None);
        assert_eq!(profile_launch_refusal(true, None), Some(ALREADY_RUNNING));
        // Just started, maybe not in the process list yet: held.
        assert_eq!(
            profile_launch_refusal(false, Some(Duration::from_secs(1))),
            Some(JUST_STARTED)
        );
        assert_eq!(
            profile_launch_refusal(false, Some(PROFILE_LAUNCH_HOLD)),
            None
        );
        assert_eq!(
            profile_launch_refusal(true, Some(PROFILE_LAUNCH_HOLD * 3)),
            Some(ALREADY_RUNNING)
        );
    }

    #[test]
    fn profile_launch_falls_back_to_the_steam_url_with_arguments() {
        let dir = Path::new(r"F:\FS25 Profiles\smoke");
        // `-profile "F:\FS25 Profiles\smoke"`, encoded: no space or quote left in the URL.
        let url = r"steam://run/2300320//-profile%20%22F:\FS25%20Profiles\smoke%22/";
        assert_eq!(steam_profile_url(dir), url);
        assert_eq!(
            profile_launch_plan(None, dir),
            LaunchPlan::SteamUrl(url.into())
        );
        // A standalone (macOS) install isn't started this way.
        let mut mac = steam_game("/Applications/FS25.app", "/Applications/FS25.app");
        mac.store = Store::Standalone;
        assert_eq!(
            profile_launch_plan(Some(&mac), dir),
            LaunchPlan::SteamUrl(url.into())
        );
        // A folder without spaces needs no quotes.
        assert_eq!(
            steam_profile_url(Path::new(r"F:\fs25-silo-smoke")),
            r"steam://run/2300320//-profile%20F:\fs25-silo-smoke/"
        );
        // Shell and URL syntax in a folder name is encoded, never left as is.
        let odd = steam_profile_url(Path::new(r"F:\A&B %PATH% ^|<>/#?\é"));
        assert!(
            !odd["steam://run/2300320//".len()..odd.len() - 1]
                .contains([' ', '"', '&', '|', '^', '<', '>', '/', '#', '?', 'é']),
            "{odd}"
        );
        assert!(odd.contains("%25PATH%25"), "{odd}");
        assert!(odd.contains("%C3%A9"), "{odd}");
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

    #[test]
    fn windows_roots_without_registry_are_the_c_defaults() {
        assert_eq!(
            windows_steam_roots(None),
            vec![
                p("C:/Program Files (x86)/Steam"),
                p("C:/Program Files/Steam")
            ]
        );
    }

    #[test]
    fn windows_roots_put_the_registry_path_first() {
        // Steam client on another drive: found via the registry, defaults still tried after it.
        assert_eq!(
            windows_steam_roots(Some(p("d:/steam"))),
            vec![
                p("d:/steam"),
                p("C:/Program Files (x86)/Steam"),
                p("C:/Program Files/Steam")
            ]
        );
    }

    #[test]
    fn windows_roots_drop_a_default_the_registry_already_names() {
        // Steam writes SteamPath lowercased with forward slashes.
        assert_eq!(
            windows_steam_roots(Some(p("c:/program files (x86)/steam"))),
            vec![
                p("c:/program files (x86)/steam"),
                p("C:/Program Files/Steam")
            ]
        );
        // Backslashes and a trailing separator are the same folder too.
        assert_eq!(
            windows_steam_roots(Some(p(r"C:\Program Files\Steam\"))),
            vec![
                p(r"C:\Program Files\Steam\"),
                p("C:/Program Files (x86)/Steam")
            ]
        );
    }

    #[test]
    fn game_exe_matching() {
        let install = p("/lib/steamapps/common/Farming Simulator 25");
        let only = |name: &'static str| {
            let want = install.join(name);
            move |q: &Path| q == want
        };
        let linux = [EXE_WINDOWS, "FarmingSimulator2025"];

        // Proton install: only the Windows .exe is there.
        assert_eq!(
            find_game_exe(&install, &linux, only(EXE_WINDOWS)),
            Some(install.join(EXE_WINDOWS))
        );
        // A native build's extensionless binary is still found.
        assert_eq!(
            find_game_exe(&install, &linux, only("FarmingSimulator2025")),
            Some(install.join("FarmingSimulator2025"))
        );
        // Earlier names win when several exist.
        assert_eq!(
            find_game_exe(&install, &linux, |_| true),
            Some(install.join(EXE_WINDOWS))
        );
        // Nothing there, or only a name not in the list.
        assert_eq!(find_game_exe(&install, &linux, |_| false), None);
        assert_eq!(
            find_game_exe(&install, &[EXE_WINDOWS], only("FarmingSimulator2025")),
            None
        );
    }

    /// The real per-platform list finds a Proton / Windows install (the .exe alone).
    #[cfg(not(target_os = "macos"))]
    #[test]
    fn platform_exe_list_finds_the_windows_exe() {
        let install = p("/lib/steamapps/common/Farming Simulator 25");
        let exe = install.join(EXE_WINDOWS);
        assert_eq!(
            find_game_exe(&install, EXES, |q| q == exe),
            Some(exe.clone())
        );
    }
}

#[cfg(test)]
mod process_tests {
    use super::*;

    const GAME_EXE: &str =
        "Z:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\x64\\FarmingSimulator2025Game.exe";

    /// What [`fs25_running`] decides for one process: the name-only pre-filter, then the
    /// classifier. Positive cases go through both, so a classifier branch the pre-filter
    /// never lets reach it fails here.
    fn seen_as_fs25(name: &str, cmd: &[String], exe: Option<&str>) -> bool {
        is_candidate_name(name) && is_fs25_process(name, cmd, exe)
    }

    #[test]
    fn matches_fs25_processes_on_every_platform() {
        // Windows
        assert!(seen_as_fs25("FarmingSimulator2025.exe", &[], None));
        assert!(seen_as_fs25("FarmingSimulator2025Game.exe", &[], None));
        assert!(seen_as_fs25("farmingsimulator2025game.exe", &[], None));
        // Proton: Linux truncates comm to 15 chars; the command line carries the exe path.
        assert!(seen_as_fs25("FarmingSimulato", &[GAME_EXE.into()], None));
        assert!(seen_as_fs25(
            "FarmingSimulato",
            &[],
            Some("/home/u/.steam/Farming Simulator 25/FarmingSimulator2025")
        ));
        // macOS
        assert!(seen_as_fs25("FarmingSimulator2025Game", &[], None));
        assert!(seen_as_fs25("Farming Simulator 25", &[], None));
        // Dedicated server when it lives in an FS25 install.
        assert!(seen_as_fs25(
            "dedicatedServer.exe",
            &[
                "E:\\SteamLibrary\\steamapps\\common\\Farming Simulator 25\\dedicatedServer.exe"
                    .into()
            ],
            None
        ));
        // Linux: the command line is just the relative launcher, the exe path names the install.
        assert!(seen_as_fs25(
            "dedicatedServer",
            &["./dedicatedServer".into()],
            Some("/opt/Farming Simulator 25/dedicatedServer")
        ));
        // Wine/Proton host process running the game.
        assert!(seen_as_fs25("wine64-preloader", &[GAME_EXE.into()], None));
    }

    #[test]
    fn truncated_linux_name_alone_is_detected() {
        // A Linux process table where the game shows up only under its 15-char name (#345).
        type Proc = (&'static str, Vec<String>, Option<&'static str>);
        let table: Vec<Proc> = vec![
            (
                "systemd",
                vec!["/sbin/init".into()],
                Some("/usr/lib/systemd/systemd"),
            ),
            (
                "steam",
                vec![GAME_EXE.into()],
                Some("/home/u/.steam/steam/ubuntu12_32/steam"),
            ),
            ("FarmingSimulato", vec![GAME_EXE.into()], None),
        ];
        // Same two passes as fs25_running: name pre-filter, then the classifier.
        let detected = |procs: &[Proc]| {
            procs
                .iter()
                .filter(|(name, _, _)| is_candidate_name(name))
                .any(|(name, cmd, exe)| is_fs25_process(name, cmd, *exe))
        };
        assert!(detected(&table));
        // Without the game in the table nothing is detected.
        assert!(!detected(&table[..2]));
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
