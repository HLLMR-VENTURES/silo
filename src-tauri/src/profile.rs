//! A throwaway profile for demos and screenshots (`docs/SHOWCASE.md`). **Debug builds only.**
//!
//! When a debug build starts with `SILO_PROFILE_DIR` set, every folder Silo would otherwise
//! read or write outside a mods folder moves under that directory, so the app can run against
//! a staged demo library without touching the real FS25 user folder or Silo's own data:
//!
//! | What | Normally | In a profile |
//! |---|---|---|
//! | FS25 user folder (`gameSettings.xml`, `log.txt`, savegames, `mods/`) | `Documents\My Games\FarmingSimulator2025` | `<profile>/FarmingSimulator2025` |
//! | App data (`silo.db` and its snapshots) | `%APPDATA%\com.hllmr.silo` | `<profile>/app-data` |
//! | Icon cache | `%LOCALAPPDATA%\com.hllmr.silo` | `<profile>/cache` |
//! | Log file | `%LOCALAPPDATA%\com.hllmr.silo\logs` | `<profile>/logs` |
//! | Webview storage (`localStorage`) | `%LOCALAPPDATA%\com.hllmr.silo\EBWebView` | `<profile>/webview` |
//! | Account tokens | OS keychain | the profile's `silo.db` only |
//!
//! The bundle identifier also becomes `com.hllmr.silo.profile`, which keeps the single-instance
//! lock (and any Tauri folder this table misses) apart from an installed Silo that's running.
//! Launching the game is refused: FS25 would read the real user folder, not the demo one.
//!
//! **No network.** A profile never talks to SiloAPI, GitHub or the update server. The updater
//! plugin isn't loaded, and commands named in `<profile>/stubs.json` get its canned answer
//! instead of running (see `answer`), so Updates, Browse, provenance and sharing show demo
//! data with no real account or token. Commands not named there run normally against the
//! profile's files; the capture script only drives ones that don't go online.
//!
//! The directory must already exist and hold a `silo-profile.marker` file (the staging script
//! writes it). Anything else stops Silo at startup instead of falling back to the real profile.
//! Release builds ignore the variable entirely: `dir()` is a constant `None` there.

use std::path::{Path, PathBuf};

/// The environment variable that names the profile directory.
pub const ENV: &str = "SILO_PROFILE_DIR";
/// Optional: a port for the webview's DevTools protocol, so a capture script can drive it.
pub const CDP_PORT_ENV: &str = "SILO_PROFILE_CDP_PORT";
/// The file that marks a directory as a Silo profile.
pub const MARKER: &str = "silo-profile.marker";
/// The bundle identifier a profile runs under.
pub const IDENTIFIER: &str = "com.hllmr.silo.profile";
/// Canned command answers inside a profile.
pub const STUBS: &str = "stubs.json";

/// Check a candidate profile directory: absolute, an existing directory, holding the marker.
pub fn validate(raw: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(raw.trim());
    if !p.is_absolute() {
        return Err(format!("{ENV} must be an absolute path, got {raw:?}"));
    }
    if !p.is_dir() {
        return Err(format!("{ENV} is not a directory: {}", p.display()));
    }
    if !p.join(MARKER).is_file() {
        return Err(format!(
            "{ENV} has no {MARKER} (run scripts/showcase/stage.mjs first): {}",
            p.display()
        ));
    }
    Ok(p)
}

/// The profile directory for this run, or `None` for a normal run. Read once. A set but
/// invalid value panics: running against the real profile by mistake is the one outcome
/// this must never have.
#[cfg(debug_assertions)]
pub fn dir() -> Option<&'static Path> {
    static DIR: std::sync::OnceLock<Option<PathBuf>> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let raw = std::env::var(ENV).ok().filter(|v| !v.trim().is_empty())?;
        Some(validate(&raw).unwrap_or_else(|e| panic!("refusing to start: {e}")))
    })
    .as_deref()
}

/// Release builds never use a profile.
#[cfg(not(debug_assertions))]
pub fn dir() -> Option<&'static Path> {
    None
}

/// The profile's canned answers (`stubs.json`), read once at startup. Empty when there's no
/// profile or no file; a file that isn't valid JSON stops Silo, like a bad profile path.
pub fn stubs() -> &'static serde_json::Value {
    static STUBS_JSON: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    STUBS_JSON.get_or_init(|| {
        let Some(p) = dir() else {
            return serde_json::Value::Null;
        };
        match std::fs::read_to_string(p.join(STUBS)) {
            Ok(text) => serde_json::from_str(&text)
                .unwrap_or_else(|e| panic!("refusing to start: {STUBS} isn't valid JSON: {e}")),
            Err(_) => serde_json::Value::Null,
        }
    })
}

/// The canned answer for `command`, if `stubs` names it. Each entry is one of:
/// - `{ "value": <json> }`: always this answer;
/// - `{ "byArg": "techName", "cases": { "<arg value>": <json> }, "value": <json> }`: the case
///   matching that argument, else `value` (else the command runs normally);
/// - `{ "error": "message" }`: the command fails with that message.
///
/// @param stubs the parsed `stubs.json`
/// @param command the invoked command's name
/// @param args the command's JSON arguments
pub fn answer(
    stubs: &serde_json::Value,
    command: &str,
    args: &serde_json::Value,
) -> Option<Result<serde_json::Value, String>> {
    let entry = stubs.get(command)?;
    if let Some(e) = entry.get("error").and_then(|e| e.as_str()) {
        return Some(Err(e.to_string()));
    }
    if let Some(arg) = entry.get("byArg").and_then(|a| a.as_str()) {
        let key = args.get(arg).and_then(|v| v.as_str()).unwrap_or("");
        if let Some(case) = entry.get("cases").and_then(|c| c.get(key)) {
            return Some(Ok(case.clone()));
        }
    }
    entry.get("value").cloned().map(Ok)
}

/// True while running in a profile.
pub fn active() -> bool {
    dir().is_some()
}

/// The FS25 user folder inside a profile.
pub fn fs25_user_dir(profile: &Path) -> PathBuf {
    profile.join("FarmingSimulator2025")
}

/// The webview's storage folder inside a profile.
pub fn webview_dir(profile: &Path) -> PathBuf {
    profile.join("webview")
}

/// `--remote-debugging-port=N` for the webview, when a valid port is set. WebView2 replaces
/// its default arguments when given any, so callers append this to wry's defaults.
pub fn cdp_arg(raw: Option<&str>) -> Option<String> {
    let port: u16 = raw?.trim().parse().ok().filter(|p| *p >= 1024)?;
    Some(format!("--remote-debugging-port={port}"))
}

/// Silo's app-data folder (`silo.db`): the profile's when one is active.
pub fn app_data_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<PathBuf> {
    use tauri::Manager;
    match dir() {
        Some(p) => Ok(p.join("app-data")),
        None => app.path().app_data_dir(),
    }
}

/// Silo's cache folder (icon thumbnails): the profile's when one is active.
pub fn app_cache_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<PathBuf> {
    use tauri::Manager;
    match dir() {
        Some(p) => Ok(p.join("cache")),
        None => app.path().app_cache_dir(),
    }
}

/// Silo's log folder: the profile's when one is active.
pub fn app_log_dir<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::Result<PathBuf> {
    use tauri::Manager;
    match dir() {
        Some(p) => Ok(p.join("logs")),
        None => app.path().app_log_dir(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_profile_needs_an_absolute_marked_directory() {
        let tmp = std::env::temp_dir().join(format!("silo-profile-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        let raw = tmp.to_string_lossy().into_owned();

        assert!(validate("relative/profile")
            .unwrap_err()
            .contains("absolute"));
        assert!(validate(&tmp.join("missing").to_string_lossy())
            .unwrap_err()
            .contains("not a directory"));
        assert!(validate(&raw).unwrap_err().contains(MARKER));

        std::fs::write(tmp.join(MARKER), "").unwrap();
        assert_eq!(validate(&raw).unwrap(), tmp);
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn stubs_answer_by_name_and_by_argument() {
        let stubs = serde_json::json!({
            "gh_status": { "value": { "user": "demo-farmer" } },
            "verify_mod": {
                "byArg": "techName",
                "cases": { "FS25_A": { "status": "verified" } },
                "value": { "status": "unverified" }
            },
            "browse_mods": { "error": "offline" },
            "only_cases": { "byArg": "id", "cases": { "x": 1 } }
        });
        let none = serde_json::json!({});
        assert_eq!(
            answer(&stubs, "gh_status", &none),
            Some(Ok(serde_json::json!({ "user": "demo-farmer" })))
        );
        let a = serde_json::json!({ "techName": "FS25_A" });
        let b = serde_json::json!({ "techName": "FS25_B" });
        assert_eq!(
            answer(&stubs, "verify_mod", &a),
            Some(Ok(serde_json::json!({ "status": "verified" })))
        );
        assert_eq!(
            answer(&stubs, "verify_mod", &b),
            Some(Ok(serde_json::json!({ "status": "unverified" })))
        );
        assert_eq!(
            answer(&stubs, "browse_mods", &none),
            Some(Err("offline".into()))
        );
        // Not named, or no case and no default: the command runs normally.
        assert_eq!(answer(&stubs, "scan_mods", &none), None);
        assert_eq!(
            answer(&stubs, "only_cases", &serde_json::json!({ "id": "y" })),
            None
        );
        assert_eq!(answer(&serde_json::Value::Null, "gh_status", &none), None);
    }

    #[test]
    fn the_devtools_port_must_be_a_real_unprivileged_port() {
        assert_eq!(
            cdp_arg(Some("9333")).as_deref(),
            Some("--remote-debugging-port=9333")
        );
        assert_eq!(
            cdp_arg(Some(" 9333 ")).as_deref(),
            Some("--remote-debugging-port=9333")
        );
        assert_eq!(cdp_arg(Some("80")), None);
        assert_eq!(cdp_arg(Some("99999")), None);
        assert_eq!(cdp_arg(Some("x")), None);
        assert_eq!(cdp_arg(None), None);
    }
}
