//! Silo core — Tauri command surface. All heavy logic lives in sibling modules so
//! it stays unit-testable (and reusable by a future CLI) without a running app.

pub mod bindings;
pub mod bisect;
pub mod bridge;
pub mod category;
pub mod collection;
pub mod conflicts;
pub mod db;
mod diagnostics;
pub mod fsgame;
mod fslog;
pub mod gamelaunch;
pub mod github;
pub mod icons;
pub mod logscan;
pub mod moddesc;
pub mod modsroot;
pub mod mpsync;
pub mod net;
pub mod notices;
pub mod organize;
pub mod paths;
mod picked;
pub mod provenance;
pub mod relocate;
pub mod savegame;
pub mod scan;
pub mod secrets;
pub mod settings_form;
pub mod siloapi;
pub mod source;
pub mod store;
pub mod update;
pub mod xmlconfig;
pub mod xmltext;

use std::collections::HashSet;
use std::path::PathBuf;
use tauri::{Emitter, Manager};

#[derive(Clone, serde::Serialize)]
struct Progress {
    done: usize,
    total: usize,
}

/// Silo's own GitHub OAuth App client id, baked in so end users get one-click
/// "Connect GitHub" (device flow) with no manual setup. Register ONE OAuth App
/// under the HLLMR account (github.com/settings/applications/new), enable
/// "Device Flow", and paste its Client ID (Iv1.…/Ov23…) here. Client ids are
/// public — safe to embed; device flow needs no secret. Empty = fall back to a
/// user-provided id from Settings.
const SILO_GH_CLIENT_ID: &str = "Ov23lizY2TSJF2P5CRyx";

// The client id is a compile-time toggle: empty = fall back to a user-provided id. For a
// build with it baked in, `is_empty()` is const-false — that's intentional, not a bug.
#[allow(clippy::const_is_empty)]
fn effective_client_id(conn: &rusqlite::Connection) -> Option<String> {
    if !SILO_GH_CLIENT_ID.is_empty() {
        Some(SILO_GH_CLIENT_ID.to_string())
    } else {
        db::get_app_setting(conn, "gh_client_id")
    }
}

fn db_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|d| d.join("silo.db"))
        .map_err(|e| e.to_string())
}

/// Run a command's disk, database, keychain or process work on the blocking pool. A sync
/// command runs on the main thread, so anything slower than a few microseconds there freezes
/// the window. Every command that isn't pure goes through this.
async fn off_main<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())?
}

/// Scan the given roots (or the auto-detected default when omitted/empty).
/// Runs on a blocking thread so the async runtime / UI never stalls.
#[tauri::command]
async fn scan_mods(
    app: tauri::AppHandle,
    roots: Option<Vec<String>>,
) -> Result<scan::ScanResult, String> {
    let roots: Vec<PathBuf> = match roots {
        Some(v) if !v.is_empty() => v.into_iter().map(PathBuf::from).collect(),
        _ => mods_roots(),
    };
    // The organize manifest is per folder; scope the reconcile to the folder being scanned.
    let scan_key = roots.first().map(|r| modsroot::canonical_key(r));
    let active = modsroot::active();

    tauri::async_runtime::spawn_blocking(move || {
        // A mod update Silo was in the middle of when it stopped is finished or rolled back
        // first, so the scan sees the library as it should be.
        let recovered = recover_interrupted_updates();
        // Warm cache: parsed entries persist between launches, keyed by
        // path+mtime+size, so unchanged mods skip archive parsing entirely.
        let mut conn = db::open(&db_path(&app)?)?;
        let cache = db::load_cache(&conn);

        // Each mods folder with its archive(s), and whether the scanned folder's manifest may
        // be rewritten from what's found (see `scan_archives`).
        let mut may_reconcile = true;
        let layouts: Vec<(PathBuf, Vec<PathBuf>)> = roots
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let (archives, ok) = scan_archives(&conn, r, active.as_ref());
                if i == 0 {
                    may_reconcile = ok;
                }
                (r.clone(), archives)
            })
            .collect();

        let emitter = app.clone();
        let out = scan::scan_layouts(layouts, &cache, move |done, total| {
            let _ = emitter.emit("scan:progress", Progress { done, total });
        });

        // Persist freshly-parsed rows; prune mods that vanished from disk.
        let fresh_rows: Vec<(String, u64, u64, String)> = out
            .result
            .mods
            .iter()
            .filter(|m| out.fresh_paths.contains(&m.path))
            .filter_map(|m| {
                serde_json::to_string(m)
                    .ok()
                    .map(|json| (m.path.clone(), m.mtime_ms, m.size, json))
            })
            .collect();
        let _ = db::upsert_many(&mut conn, &fresh_rows);

        let present: HashSet<String> = out.result.mods.iter().map(|m| m.path.clone()).collect();
        let _ = db::prune_missing(&mut conn, &present);

        // Keep the `organized` manifest in step with what's physically in `archive/`, so an
        // archived mod is always activatable even if the DB drifted from disk (e.g. a fresh
        // DB behind a populated archive). The archive layout is `archive/<Category>/<file>`,
        // so the category is the file's parent directory name.
        let organized_rows: Vec<db::OrganizedRow> = out
            .result
            .mods
            .iter()
            .filter(|m| m.organized)
            .filter_map(|m| {
                let p = std::path::Path::new(&m.path);
                let file_name = p.file_name()?.to_string_lossy().into_owned();
                let category = p.parent()?.file_name()?.to_string_lossy().into_owned();
                Some(db::OrganizedRow {
                    tech_name: m.tech_name.clone(),
                    file_name,
                    kind: m.kind.clone(),
                    category,
                    subcategory: None,
                    active: m.active,
                })
            })
            .collect();
        if let Some(key) = scan_key.as_deref().filter(|_| may_reconcile) {
            let _ = db::reconcile_organized(&mut conn, key, &organized_rows);
        }

        let mut result = out.result;
        result.notices = recovered.notices;
        result.problems = recovered.problems;
        Ok::<_, String>(result)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The archive(s) a scan of mods folder `root` reads, and whether that scan may rewrite the
/// folder's `organized` manifest from what it finds. During an unfinished move both archives
/// are read (the current direction's destination first, so it wins for mods already moved) and
/// the manifest is left alone — the files are in flux. Otherwise the folder's own archive: the
/// published one for the active folder (`active`), the stored choice (or the default) for any
/// other. A custom archive that isn't there (its drive unplugged), or any archive that exists
/// but can't be read, scans as empty — so the manifest is left alone then too: rewriting it
/// would forget the whole library.
fn scan_archives(
    conn: &rusqlite::Connection,
    root: &std::path::Path,
    active: Option<&modsroot::Active>,
) -> (Vec<PathBuf>, bool) {
    if let Some(m) = relocate::load_marker(conn, &modsroot::canonical_key(root)) {
        let archives = match m.direction {
            relocate::Direction::Forward => vec![m.to, m.from],
            relocate::Direction::Back => vec![m.from, m.to],
        };
        return (archives, false);
    }
    let layout = match active {
        Some(a) if modsroot::same_folder(&a.layout.mods, root) => a.layout.clone(),
        _ => resolve_active(conn, root).layout,
    };
    let ok = !layout.archive_missing() && relocate::filed_mods(&layout.archive).is_ok();
    (vec![layout.archive], ok)
}

/// Return a mod's icon as a cached PNG `data:` URL (or null if unavailable).
/// Decodes off-thread; cached to the app cache dir on first use.
#[tauri::command]
async fn get_mod_icon(
    app: tauri::AppHandle,
    path: String,
    kind: String,
    icon_filename: Option<String>,
) -> Option<String> {
    let icon = icon_filename?;
    let cache_dir = app.path().app_cache_dir().ok()?.join("icons-v2");
    tauri::async_runtime::spawn_blocking(move || {
        icons::cached_data_url(&cache_dir, std::path::Path::new(&path), &kind, &icon)
    })
    .await
    .ok()
    .flatten()
}

// ── Curation (favorite / hidden / broken) ──
#[tauri::command]
async fn get_curation(app: tauri::AppHandle) -> Result<Vec<db::CurationRow>, String> {
    let db = db_path(&app)?;
    off_main(move || Ok(db::load_curation(&db::open(&db)?))).await
}

#[tauri::command]
async fn set_curation(app: tauri::AppHandle, row: db::CurationRow) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || db::set_curation(&db::open(&db)?, &row)).await
}

// ── GitHub update checking ──
#[tauri::command]
async fn get_mod_repos(app: tauri::AppHandle) -> Result<Vec<db::RepoRow>, String> {
    let db = db_path(&app)?;
    off_main(move || Ok(db::load_repos(&db::open(&db)?))).await
}

#[tauri::command]
async fn set_mod_repo(
    app: tauri::AppHandle,
    tech_name: String,
    owner: String,
    repo: String,
) -> Result<(), String> {
    // Empty owner or repo means "unlink" (db::set_repo deletes the row); anything else must
    // be a real GitHub repository name before it's stored and later used in API calls.
    if !owner.trim().is_empty() && !repo.trim().is_empty() {
        github::check_repo(owner.trim(), repo.trim())?;
    }
    let db = db_path(&app)?;
    off_main(move || db::set_repo(&db::open(&db)?, &tech_name, &owner, &repo)).await
}

// ── Source pages: where each mod came from (#196) ──
// Silo only stores these addresses. No command here (or anywhere) requests them; the
// frontend hands them to the user's browser via `openExternal`.
#[tauri::command]
async fn get_mod_sources(app: tauri::AppHandle) -> Result<Vec<db::SourceRow>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<db::SourceRow>, String> {
        let conn = db::open(&db)?;
        Ok(db::load_sources(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Save (or remove, when `url` is empty or null) a mod's source page. Returns the GitHub repo
/// link it also made, when the page is a GitHub repository and the mod had none.
#[tauri::command]
async fn set_mod_source(
    app: tauri::AppHandle,
    tech_name: String,
    url: Option<String>,
) -> Result<Option<db::RepoRow>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<db::RepoRow>, String> {
        let conn = db::open(&db)?;
        source::save(&conn, &tech_name, url.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Record that the user checked a mod's source page just now. Returns the time (unix secs).
#[tauri::command]
async fn mark_source_checked(app: tauri::AppHandle, tech_name: String) -> Result<i64, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<i64, String> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let conn = db::open(&db)?;
        if db::mark_source_checked(&conn, &tech_name, now)? {
            Ok(now)
        } else {
            Err("Save the page this mod came from first.".to_string())
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Scan a mod's modDesc.xml for a github.com/owner/repo reference.
#[tauri::command]
async fn guess_repo(path: String, kind: String) -> Result<Option<db::RepoRow>, String> {
    off_main(move || {
        let Ok(xml) = scan::read_moddesc_xml(std::path::Path::new(&path), &kind) else {
            return Ok(None);
        };
        Ok(
            github::find_repo_in_text(&xml).map(|(owner, repo)| db::RepoRow {
                tech_name: String::new(),
                owner,
                repo,
            }),
        )
    })
    .await
}

#[tauri::command]
async fn check_mod_update(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    current: String,
) -> Result<github::UpdateInfo, String> {
    github::check_repo(&owner, &repo)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<github::UpdateInfo, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token");
        github::check(&owner, &repo, &current, token.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct GhStatus {
    client_id: Option<String>,
    user: Option<String>,
    /// True when a client id is baked into the app (no user setup needed).
    builtin: bool,
    /// True when the stored token can act (star/watch): a `public_repo` OAuth grant
    /// or a PAT. Read-only (`read:user`) connections report false.
    can_write: bool,
    /// True when the stored token can create/read the user's gists (the `gist` OAuth
    /// grant) — needed to share a Collection. Independent of `can_write`.
    can_gist: bool,
}

/// Does a stored scope string grant `want`? Handles GitHub's two formats (the device
/// response is space-separated, `X-OAuth-Scopes` is comma-separated). The `"*"` sentinel
/// means "scopes unknown — assume yes", used for fine-grained PATs that report none.
fn scope_grants(scopes: &str, want: &str) -> bool {
    scopes == "*" || scopes.split([',', ' ']).map(str::trim).any(|s| s == want)
}

#[tauri::command]
#[allow(clippy::const_is_empty)]
async fn gh_status(app: tauri::AppHandle) -> Result<GhStatus, String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        // Capability comes from the scopes GitHub actually granted. Older connections predate
        // scope capture and only carry the coarse booleans — honour those until the next
        // reconnect writes `gh_scopes`.
        let (can_write, can_gist) = match db::get_app_setting(&conn, "gh_scopes") {
            Some(s) => (
                scope_grants(&s, "public_repo") || scope_grants(&s, "repo"),
                scope_grants(&s, "gist"),
            ),
            None => (
                db::get_app_setting(&conn, "gh_write").as_deref() == Some("1"),
                db::get_app_setting(&conn, "gh_gist").as_deref() == Some("1"),
            ),
        };
        Ok(GhStatus {
            client_id: effective_client_id(&conn),
            user: db::get_app_setting(&conn, "gh_user"),
            builtin: !SILO_GH_CLIENT_ID.is_empty(),
            can_write,
            can_gist,
        })
    })
    .await
}

#[tauri::command]
async fn gh_set_client_id(app: tauri::AppHandle, client_id: String) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        let v = client_id.trim();
        db::set_app_setting(
            &conn,
            "gh_client_id",
            if v.is_empty() { None } else { Some(v) },
        )
    })
    .await
}

#[tauri::command]
async fn gh_device_start(
    app: tauri::AppHandle,
    write: Option<bool>,
    gist: Option<bool>,
) -> Result<github::DeviceCode, String> {
    let db = db_path(&app)?;
    // Ask only for what the user is enabling — a plain sign-in for update-checks stays
    // read-only. `public_repo` is added for star/watch actions, `gist` for Collection
    // sharing. GitHub accumulates scopes across authorizations, so the caller requests
    // the union of what it already holds plus the new capability.
    let scope = build_gh_scope(write.unwrap_or(false), gist.unwrap_or(false));
    tauri::async_runtime::spawn_blocking(move || -> Result<github::DeviceCode, String> {
        let conn = db::open(&db)?;
        let cid = effective_client_id(&conn)
            .ok_or_else(|| "No GitHub OAuth App Client ID configured".to_string())?;
        github::device_start(&cid, &scope)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Build the OAuth scope string for a device-flow request. `read:user` is always
/// present (identity + higher rate limit); `public_repo`/`gist` are added per capability.
fn build_gh_scope(write: bool, gist: bool) -> String {
    let mut scopes = vec!["read:user"];
    if write {
        scopes.push("public_repo");
    }
    if gist {
        scopes.push("gist");
    }
    scopes.join(" ")
}

#[tauri::command]
async fn gh_device_poll(
    app: tauri::AppHandle,
    device_code: String,
) -> Result<github::PollResult, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<github::PollResult, String> {
        let conn = db::open(&db)?;
        let cid =
            effective_client_id(&conn).ok_or_else(|| "No client id configured".to_string())?;
        let res = github::device_poll(&cid, &device_code)?;
        if res.status == "ok" {
            if let Some(tok) = &res.token {
                let user = github::whoami(tok).unwrap_or_default();
                secrets::set(&conn, "gh_token", Some(tok))?;
                db::set_app_setting(&conn, "gh_user", Some(&user))?;
                // Store the scopes GitHub actually granted — capability is derived from
                // these, not from what we asked for. Accumulates across re-auth.
                db::set_app_setting(&conn, "gh_scopes", Some(res.scope.as_deref().unwrap_or("")))?;
            }
        }
        // Never expose the raw token to the frontend.
        Ok(github::PollResult { token: None, ..res })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// PAT fallback for users who'd rather mint a token than OAuth. A classic PAT reports its
/// scopes (`X-OAuth-Scopes`), so we store exactly what it can do. A fine-grained PAT uses
/// a different permission model and reports no scopes — we can't introspect it, so we
/// record the `"*"` sentinel ("assume capable", since the user minted it deliberately) and
/// let a 403 surface a reconnect hint if a specific permission is missing.
#[tauri::command]
async fn gh_set_pat(app: tauri::AppHandle, pat: String) -> Result<String, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let pat = pat.trim().to_string();
        if pat.is_empty() {
            return Err("Empty token".into());
        }
        let (user, scopes) = github::whoami_scoped(&pat)?;
        if user.is_empty() {
            return Err("GitHub did not recognize that token".into());
        }
        let conn = db::open(&db)?;
        secrets::set(&conn, "gh_token", Some(&pat))?;
        db::set_app_setting(&conn, "gh_user", Some(&user))?;
        let stored = if scopes.trim().is_empty() {
            "*".to_string()
        } else {
            scopes
        };
        db::set_app_setting(&conn, "gh_scopes", Some(&stored))?;
        Ok(user)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gh_repo_stats(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
) -> Result<github::RepoStats, String> {
    github::check_repo(&owner, &repo)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<github::RepoStats, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token");
        github::repo_stats(&owner, &repo, token.as_deref())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gh_star(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    on: bool,
) -> Result<bool, String> {
    github::check_repo(&owner, &repo)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        github::set_star(&owner, &repo, &token, on)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gh_watch(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    on: bool,
) -> Result<bool, String> {
    github::check_repo(&owner, &repo)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        github::set_watch(&owner, &repo, &token, on)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What `download_update` tells the frontend: where the previous version is kept, and anything
/// that went wrong after the new version was in place (the update itself stands).
#[derive(Debug, Default, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateOutcome {
    backup: Option<String>,
    problems: Vec<String>,
}

const UPDATE_GAME_RUNNING: &str = "Close Farming Simulator 25 first, then update.";

/// FS25 writes gameSettings.xml (and mods their settings) back when the game exits, so a change
/// made while it runs is silently undone.
const CONFIG_GAME_RUNNING: &str = "Close Farming Simulator 25 first. The game saves its own \
     settings when it exits, so a change made now would be lost.";

fn config_write_refusal(game_running: bool) -> Result<(), String> {
    if game_running {
        Err(CONFIG_GAME_RUNNING.into())
    } else {
        Ok(())
    }
}

/// FS25's `modsDirectoryOverride` decides the active mods folder when the user hasn't chosen
/// one in Silo, so only `point_game_at` (Settings) may change it, never a config edit.
const OVERRIDE_IS_SILOS: &str = "Change the mods folder in Settings.";

/// Pure: refuse an edit of gameSettings.xml that changes the mods-folder override (its folder
/// or whether it's on, or adds or removes it). Every other edit is allowed.
fn override_edit_refusal(old: &str, new: &str) -> Result<(), String> {
    if modsroot::read_override(old) == modsroot::read_override(new) {
        Ok(())
    } else {
        Err(OVERRIDE_IS_SILOS.into())
    }
}

/// When a config write targets FS25's gameSettings.xml, refuse it if it would change the
/// mods-folder override. `old` is the current text when the caller already read it; an
/// unreadable file counts as one without the override.
fn ensure_override_kept(
    user: &std::path::Path,
    target: &std::path::Path,
    old: Option<&str>,
    new: &str,
) -> Result<(), String> {
    if !modsroot::same_folder(target, &user.join("gameSettings.xml")) {
        return Ok(());
    }
    match old {
        Some(old) => override_edit_refusal(old, new),
        None => {
            let old = paths::guarded_xml_read(&[user.to_path_buf()], target).unwrap_or_default();
            override_edit_refusal(&old, new)
        }
    }
}

const UPDATE_NOT_A_LIBRARY_MOD: &str = "Silo only updates mods in your library.";

/// `download_update` may replace only a mod zip directly in the mods folder or filed in an
/// archive category folder; never a backup or any other file under those folders. The folder
/// is resolved once to where it really is, so a category that is a link (or junction) into
/// another folder, e.g. `mods/backups`, is judged by where it leads.
fn update_target_problem(dest: &std::path::Path, layout: &organize::Layout) -> Option<String> {
    let refused = || Some(UPDATE_NOT_A_LIBRARY_MOD.to_string());
    let is_zip = dest
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let Some(parent) = dest.parent().and_then(|p| std::fs::canonicalize(p).ok()) else {
        return refused();
    };
    let in_mods = modsroot::same_folder(&parent, &layout.mods);
    let in_category = parent
        .parent()
        .is_some_and(|a| modsroot::same_folder(a, &layout.archive));
    if is_zip && (in_mods || in_category) {
        None
    } else {
        refused()
    }
}

/// What `download_update` checks again once it holds the library lock, since things may have
/// changed while it downloaded; returns the layout as it is now. An archive move started (and
/// paused) meanwhile comes first: the swap must not replace a file the move already copied.
fn update_recheck(dest: &std::path::Path) -> Result<organize::Layout, String> {
    ensure_no_move()?;
    paths::ensure_write_under(&allowed_roots(), dest)?;
    let layout = primary_layout(None)?;
    if let Some(p) = update_target_problem(dest, &layout) {
        return Err(p);
    }
    if gamelaunch::fs25_running() {
        return Err(UPDATE_GAME_RUNNING.into());
    }
    Ok(layout)
}

/// Update an installed mod from a GitHub release asset (`update` module): download beside it
/// without the library lock, then swap it in under the lock, keeping the previous version in
/// `backups/`. Refused while FS25 runs (its files may be open).
#[tauri::command]
async fn download_update(
    app: tauri::AppHandle,
    path: String,
    asset_url: String,
) -> Result<UpdateOutcome, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<UpdateOutcome, String> {
        if gamelaunch::fs25_running() {
            return Err(UPDATE_GAME_RUNNING.into());
        }
        let dest = PathBuf::from(&path);
        // Defence against a compromised webview naming an arbitrary write target: the path
        // must contain no `..` AND its parent must resolve inside a real mods root (the flat
        // root or its archive, wherever that lives). An absolute path outside them — which
        // `no_traversal` alone would pass — is rejected here.
        paths::no_traversal(&dest)?;
        paths::ensure_write_under(&allowed_roots(), &dest)?;
        // Only a mod zip in the library, never a backup or another file under its folders.
        let layout = primary_layout(None)?;
        if let Some(p) = update_target_problem(&dest, &layout) {
            return Err(p);
        }
        // Public release assets don't need auth, and a download must never carry the user's
        // GitHub credential — keep the token for explicit API actions only.
        let part = update::fetch(&asset_url, &dest)?;
        // A finished download waits out a busy spell instead of being thrown away.
        let _write = library_write_within(PLACE_WAIT)?;
        let layout = update_recheck(&dest)?;
        let mut policy = organize::LinkPolicy::new(&layout);
        // Finish an earlier, interrupted update first (the user asked: every step is retried),
        // so it doesn't block this one; if it can't be finished, say exactly why.
        let mut dirs = update::layout_dirs(&layout);
        if let Some(parent) = dest.parent() {
            if !dirs.iter().any(|d| d == parent) {
                dirs.push(parent.to_path_buf());
            }
        }
        let settled = update::settle(&dirs, &layout.mods, &mut policy, update::Attempt::UserAsked);
        log_recovery(&settled);
        let old = update::old_path(&dest);
        if settled.leftovers.contains(&old) {
            return Err(update::leftover_message(&settled, &[old]));
        }
        github::verify_update_identity(part.path(), &dest)?;
        let done = update::swap(&dest, part.path(), &layout.mods, &mut policy)?;
        if let Some(flat) = &done.relinked {
            if let (Ok(conn), Some(name)) = (db::open(&db), flat.file_name()) {
                let key = modsroot::canonical_key(&layout.mods);
                organize::note_zip_projection(&conn, &key, &name.to_string_lossy(), flat);
            }
        }
        let label = file_label(&dest);
        match &done.backup {
            Some(b) => log::info!("updated {label}; previous version kept at {}", b.display()),
            None => log::info!("updated {label}; previous version not moved to backups"),
        }
        for p in &done.problems {
            log::warn!("{p}");
        }
        Ok(UpdateOutcome {
            backup: done.backup.map(|b| b.to_string_lossy().into_owned()),
            problems: done.problems,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn log_recovery(r: &update::Recovery) {
    for n in &r.notices {
        log::info!("{n}");
    }
    for p in &r.problems {
        log::warn!("{p}");
    }
}

/// Finish or roll back mod updates a crash interrupted (`update::settle`), in the active mods
/// folder and each archive category folder, and return what happened for the user. Only when
/// the library lock is free right now (a scan never waits for it), no archive move is
/// unfinished (its files are in flux), and Silo isn't installing its own update.
fn recover_interrupted_updates() -> update::Recovery {
    if move_in_progress() || self_updating() {
        return update::Recovery::default();
    }
    let Some(active) = modsroot::active() else {
        return update::Recovery::default();
    };
    if active.move_to.is_some() {
        return update::Recovery::default();
    }
    let Some(_write) = lock_result(LIBRARY_WRITE.try_lock()) else {
        return update::Recovery::default();
    };
    let r = settle_layout(&active.layout);
    log_recovery(&r);
    r
}

/// Background recovery over `layout` (costly steps at most once per session per file).
fn settle_layout(layout: &organize::Layout) -> update::Recovery {
    update::settle(
        &update::layout_dirs(layout),
        &layout.mods,
        &mut organize::LinkPolicy::new(layout),
        update::Attempt::Background,
    )
}

/// Resolve update leftovers in the active layout before a library write (call with the library
/// lock held). Refuses while a mod's previous version can't be put back, since the library is
/// short that mod until it is.
fn settle_updates_locked() -> Result<(), String> {
    let Some(active) = modsroot::active() else {
        return Ok(());
    };
    if active.move_to.is_some() {
        return Ok(());
    }
    let r = settle_layout(&active.layout);
    log_recovery(&r);
    if r.stranded.is_empty() {
        Ok(())
    } else {
        Err(update::leftover_message(&r, &r.stranded))
    }
}

// ── SiloAPI (mod browser / discovery) ──

/// The catalog address: the built-in one, or, in a debug build only, one a developer stored with
/// `siloapi_set_base`. Release builds ignore any stored value.
fn effective_siloapi_base(stored: Option<String>, debug: bool) -> String {
    stored
        .filter(|s| debug && !s.trim().is_empty())
        .unwrap_or_else(|| siloapi::DEFAULT_BASE.to_string())
}

fn siloapi_base(app: &tauri::AppHandle) -> Result<String, String> {
    let conn = db::open(&db_path(app)?)?;
    Ok(effective_siloapi_base(
        db::get_app_setting(&conn, "siloapi_base"),
        cfg!(debug_assertions),
    ))
}

#[tauri::command]
async fn siloapi_status(app: tauri::AppHandle) -> Result<String, String> {
    off_main(move || siloapi_base(&app)).await
}

/// Developer-only: point the catalog at another address (e.g. a local SiloAPI). Debug builds
/// only; the catalog decides where Browse and Collections installs download from.
#[cfg(debug_assertions)]
#[tauri::command]
async fn siloapi_set_base(app: tauri::AppHandle, base: String) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        let v = base.trim().trim_end_matches('/');
        // SSRF guard: a custom base drives every catalog request, so validate it before storing.
        // Allow localhost so a developer can point at their own endpoint; block private IPs.
        if !v.is_empty() {
            net::validate_outbound_url(v, true)?;
        }
        db::set_app_setting(
            &conn,
            "siloapi_base",
            if v.is_empty() { None } else { Some(v) },
        )
    })
    .await
}

/// Release builds always use the built-in catalog; same signature so `generate_handler!` stays
/// one list (the argument names are the IPC keys, so they keep their names).
#[cfg(not(debug_assertions))]
#[tauri::command]
async fn siloapi_set_base(app: tauri::AppHandle, base: String) -> Result<(), String> {
    let _ = (app, base);
    Err("Not available in this build.".into())
}

// Mirrors siloapi::browse's filter set; flat args keep the invoke() call shape simple.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn browse_mods(
    app: tauri::AppHandle,
    query: Option<String>,
    category: Option<String>,
    sort: Option<String>,
    tags: Option<Vec<String>>,
    available_by: Option<u32>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<siloapi::BrowsePage, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        siloapi::browse(
            &base,
            query.as_deref(),
            category.as_deref(),
            sort.as_deref(),
            &tags.unwrap_or_default(),
            available_by,
            limit.unwrap_or(40),
            offset.unwrap_or(0),
        )
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The available Browse facets (brand/theme/region/realism/era + counts), for the filter chips.
#[tauri::command]
async fn siloapi_facets(app: tauri::AppHandle) -> Result<siloapi::Facets, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || siloapi::facets(&base))
        .await
        .map_err(|e| e.to_string())?
}

/// One mod's full catalog record (metadata + every source it was seen on).
#[tauri::command]
async fn siloapi_mod_detail(
    app: tauri::AppHandle,
    id: String,
) -> Result<siloapi::BrowseMod, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || siloapi::detail(&base, &id))
        .await
        .map_err(|e| e.to_string())?
}

/// Resolve a *library* mod (by tech name) to its full catalog record, so the library detail
/// drawer can show the same info Browse does — summary, sources, latest version. Returns None
/// when the mod isn't catalogued (dev/obscure mods), in which case the drawer just shows less.
#[tauri::command]
async fn catalog_detail_by_tech(
    app: tauri::AppHandle,
    tech_name: String,
) -> Result<Option<siloapi::BrowseMod>, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Option<siloapi::BrowseMod>, String> {
        let hits = siloapi::lookup(&base, std::slice::from_ref(&tech_name))?;
        match hits.into_iter().find(|h| !h.id.is_empty()) {
            Some(h) => siloapi::detail(&base, &h.id).map(Some),
            None => Ok(None),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Fetch a catalog mod thumbnail hosted on GitHub as a `data:` URL (the CSP keeps the webview
/// off arbitrary image hosts). Cached on disk by URL so each image is fetched at most once.
/// ModHub thumbnails are never fetched from GIANTS' CDN here: they come from SiloAPI's cache.
#[tauri::command]
async fn catalog_image(app: tauri::AppHandle, url: String) -> Result<String, String> {
    let cache_dir = app
        .path()
        .app_cache_dir()
        .ok()
        .map(|d| d.join("catalog_images"));
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        if let Some(dir) = &cache_dir {
            let key = format!("{:x}", md5::compute(url.as_bytes()));
            let file = dir.join(&key);
            if let Ok(cached) = std::fs::read_to_string(&file) {
                return Ok(cached);
            }
            let data = siloapi::fetch_image(&url)?;
            let _ = std::fs::create_dir_all(dir);
            let _ = std::fs::write(&file, &data);
            Ok(data)
        } else {
            siloapi::fetch_image(&url)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Catalog categories with counts, for the Browse filter.
#[tauri::command]
async fn siloapi_categories(app: tauri::AppHandle) -> Result<Vec<siloapi::CategoryCount>, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || siloapi::categories(&base))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn siloapi_stats(app: tauri::AppHandle) -> Result<siloapi::Stats, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || siloapi::stats(&base))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct InstallProgress {
    id: String,
    done: u64,
    total: Option<u64>,
}

/// Download a browsed mod's .zip into the library root and return the filename.
/// Streams the download, emitting `install:progress` events per mod id so the UI can
/// show a bar. The frontend rescans (and auto-files) afterwards.
///
/// The download runs before the library lock is taken (so a slow mod never blocks other
/// changes); the lock is held only to place the finished file.
#[tauri::command]
async fn install_remote_mod(
    app: tauri::AppHandle,
    id: String,
    source: Option<String>,
    root: Option<String>,
) -> Result<String, String> {
    ensure_no_move()?;
    let base = siloapi_base(&app)?;
    let root = primary_root(root)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let resolved = siloapi::resolve_download(&base, &id, source.as_deref())?;
        // The filename comes from the catalog — validate it's a plain basename so a
        // hostile response can't redirect the write outside the mods folder.
        paths::safe_file_name(&resolved.filename)?;
        let dest = root.join(&resolved.filename);
        if path_taken(&dest) {
            return Err(already_in_library(&dest));
        }
        let part = claim_part(&dest)?;
        siloapi::download_to_part(
            &resolved.url,
            part.path(),
            resolved.expected_sha256.as_deref(),
            |done, total| {
                let _ = emitter.emit(
                    "install:progress",
                    InstallProgress {
                        id: id.clone(),
                        done,
                        total,
                    },
                );
            },
        )?;
        // A finished download waits out a busy spell instead of being thrown away.
        let _write = library_write_within(PLACE_WAIT)?;
        place_part(part.path(), &dest)?;
        Ok(resolved.filename)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Downloads whose `.zip.part` is in use, by `same_file_key`. Downloads run outside the
/// library lock, so two at once for the same file must not share (and corrupt) one part file.
static PARTS_IN_FLIGHT: std::sync::Mutex<Vec<PathBuf>> = std::sync::Mutex::new(Vec::new());

/// A download's claim on its part file. Dropping it removes whatever part is left
/// (a failed or abandoned download) and frees the name.
pub struct PartClaim {
    path: PathBuf,
    key: PathBuf,
}

impl PartClaim {
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for PartClaim {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
        let mut held = PARTS_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        held.retain(|k| k != &self.key);
    }
}

/// Paths that name the same file compare equal: on Windows file names are case-insensitive,
/// so the name is lowercased (the parent is kept as given; callers join names onto one root).
fn same_file_key(p: &std::path::Path) -> PathBuf {
    #[cfg(windows)]
    {
        if let Some(name) = p.file_name() {
            let lower = name.to_string_lossy().to_lowercase();
            return match p.parent() {
                Some(parent) => parent.join(lower),
                None => PathBuf::from(lower),
            };
        }
    }
    p.to_path_buf()
}

/// Claim the part file a download of `dest` streams into, refusing while another download
/// of the same file is running.
fn claim_part(dest: &std::path::Path) -> Result<PartClaim, String> {
    claim_part_at(dest, dest.with_extension("zip.part"))
}

/// Claim `path` as the part file a download of `dest` streams into (see [`claim_part`]).
fn claim_part_at(dest: &std::path::Path, path: PathBuf) -> Result<PartClaim, String> {
    let key = same_file_key(&path);
    let mut held = PARTS_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    if held.contains(&key) {
        return Err(format!("{} is already downloading", file_label(dest)));
    }
    held.push(key.clone());
    Ok(PartClaim { path, key })
}

/// Remove a part file left by an earlier run, unless a download in this run has claimed it.
/// The claim list stays locked across the check and the removal, so a download can't claim it
/// in between. Returns whether a file was removed.
fn remove_unclaimed_part(path: &std::path::Path) -> bool {
    let held = PARTS_IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
    if held.contains(&same_file_key(path)) {
        return false;
    }
    let removed = std::fs::remove_file(path).is_ok();
    if removed {
        fslog::changed("remove", path);
    }
    removed
}

/// Anything at `p`, including a broken link (which `Path::exists` would call absent).
fn path_taken(p: &std::path::Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn file_label(p: &std::path::Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

fn already_in_library(dest: &std::path::Path) -> String {
    format!("{} is already in your library", file_label(dest))
}

const DOWNLOAD_GONE: &str = "The download disappeared before Silo could add it. Try again.";

/// Move a finished download into the library at `dest`, never replacing anything there.
/// Hardlink then drop the part (the link fails if `dest` exists). Only when the link fails
/// because the filesystem can't hardlink (`no_hardlink_support`) does it fall back to
/// checking `dest` is free and renaming. A taken `dest` removes the part and refuses.
///
/// Call under the library lock, which keeps Silo's own writes out. The fallback still has a
/// small window between the check and the rename in which another program could create
/// `dest`; on Windows the rename would then replace it. Hardlink-capable filesystems (the
/// usual case) have no such window.
fn place_part(part: &std::path::Path, dest: &std::path::Path) -> Result<(), String> {
    place_part_with(part, dest, |a, b| std::fs::hard_link(a, b))?;
    fslog::moved("install", part, dest);
    Ok(())
}

fn place_part_with(
    part: &std::path::Path,
    dest: &std::path::Path,
    link: impl Fn(&std::path::Path, &std::path::Path) -> std::io::Result<()>,
) -> Result<(), String> {
    let refuse = || {
        let _ = std::fs::remove_file(part);
        Err(already_in_library(dest))
    };
    let io_err = |e: std::io::Error| {
        if e.kind() == std::io::ErrorKind::NotFound {
            DOWNLOAD_GONE.to_string()
        } else {
            e.to_string()
        }
    };
    match link(part, dest) {
        Ok(()) => {
            let _ = std::fs::remove_file(part);
            return Ok(());
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => return refuse(),
        Err(e) if no_hardlink_support(&e) => {} // check + rename below
        Err(e) => return Err(io_err(e)),
    }
    if path_taken(dest) {
        return refuse();
    }
    std::fs::rename(part, dest).map_err(io_err)
}

/// A hardlink error that means "this filesystem / pair of folders can't hardlink", as
/// opposed to a real failure (permissions on Windows, a missing part, a full disk):
/// - `Unsupported` on any platform;
/// - Windows: 1 ERROR_INVALID_FUNCTION (FAT/exFAT), 17 ERROR_NOT_SAME_DEVICE,
///   50 ERROR_NOT_SUPPORTED, 396 (cloud-synced folders such as OneDrive / Proton Drive);
/// - Unix: EPERM (filesystems without links, e.g. FAT on Linux), EXDEV (cross-device),
///   ENOTSUP / EOPNOTSUPP.
fn no_hardlink_support(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::Unsupported {
        return true;
    }
    #[cfg(windows)]
    const CODES: &[i32] = &[1, 17, 50, 396];
    #[cfg(target_os = "linux")]
    const CODES: &[i32] = &[1, 18, 95];
    #[cfg(target_os = "macos")]
    const CODES: &[i32] = &[1, 18, 45, 102];
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    const CODES: &[i32] = &[1, 18];
    e.raw_os_error().is_some_and(|c| CODES.contains(&c))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct LocalMod {
    tech_name: String,
    version: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogUpdate {
    tech_name: String,
    latest: Option<String>,
    has_update: bool,
    download_url: Option<String>,
    source: Option<String>,
}

/// Check the whole library against the SiloAPI catalog in one request (by tech name),
/// returning which mods have a newer version and where to get it. Covers ModHub mods
/// too — not just GitHub-linked ones. Only mods the catalog knows are returned.
#[tauri::command]
async fn catalog_check_updates(
    app: tauri::AppHandle,
    mods: Vec<LocalMod>,
) -> Result<Vec<CatalogUpdate>, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<CatalogUpdate>, String> {
        let names: Vec<String> = mods.iter().map(|m| m.tech_name.clone()).collect();
        let results = siloapi::lookup(&base, &names)?;
        let by_tech: std::collections::HashMap<&str, &siloapi::LookupResult> = results
            .iter()
            .filter_map(|r| r.tech_name.as_deref().map(|t| (t, r)))
            .collect();

        let mut out = Vec::new();
        for m in &mods {
            let Some(r) = by_tech.get(m.tech_name.as_str()) else {
                continue;
            };
            let current = m.version.clone().unwrap_or_default();
            let has_update = r
                .latest_version
                .as_deref()
                .is_some_and(|latest| github::is_newer(latest, &current));
            out.push(CatalogUpdate {
                tech_name: m.tech_name.clone(),
                latest: r.latest_version.clone(),
                has_update,
                download_url: r.download.as_ref().map(|d| d.url.clone()),
                source: r.download.as_ref().map(|d| d.source.clone()),
            });
        }
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Verify one installed mod against its canonical build: hash the local zip, look the mod
/// up in the catalog, fetch the canonical manifest for the local version, and compare.
/// Verified (byte/content match), Modified (with the exact file diff), or Unverified (no
/// hashed build to compare — not proof of anything). Provenance, not antivirus.
#[tauri::command]
async fn verify_mod(
    app: tauri::AppHandle,
    tech_name: String,
    version: Option<String>,
    path: String,
) -> Result<provenance::VerifyResult, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<provenance::VerifyResult, String> {
        let local = provenance::manifest_from_zip(std::path::Path::new(&path))?;

        // Find the catalog id by tech name (one request).
        let hits = siloapi::lookup(&base, std::slice::from_ref(&tech_name))?;
        let Some(hit) = hits
            .into_iter()
            .find(|r| r.tech_name.as_deref() == Some(tech_name.as_str()))
        else {
            return Ok(provenance::VerifyResult::unverified(
                "This mod isn't in the catalog yet — no trusted build to compare against.",
            ));
        };

        // The server does version-equivalence (ModHub `1.2.0` ↔ GitHub `v1.2.0`), so send
        // the local modDesc version verbatim. 404 → no hashed build for this version.
        let ver = version.as_deref().unwrap_or("");
        match siloapi::manifest(&base, &hit.id, ver)? {
            Some(cm) => {
                let canon_entries: Vec<provenance::Entry> = cm
                    .entries
                    .iter()
                    .map(|e| provenance::Entry {
                        path: e.path.clone(),
                        sha256: e.sha256.clone(),
                    })
                    .collect();
                let mut r = provenance::compare(
                    &local,
                    cm.archive_sha256.as_deref(),
                    &cm.manifest_hash,
                    &canon_entries,
                );
                r.matched_version = cm.version;
                Ok(r)
            }
            None => Ok(provenance::VerifyResult::unverified(
                "No hashed build for this version yet — coverage is GitHub-first and growing.",
            )),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn gh_logout(app: tauri::AppHandle) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        secrets::set(&conn, "gh_token", None)?;
        db::set_app_setting(&conn, "gh_user", None)?;
        db::set_app_setting(&conn, "gh_scopes", None)?;
        // Legacy flags from before scope capture — clear them too so nothing lingers.
        db::set_app_setting(&conn, "gh_write", None)?;
        db::set_app_setting(&conn, "gh_gist", None)?;
        Ok(())
    })
    .await
}

// ── Tags ──
#[tauri::command]
async fn get_tags(app: tauri::AppHandle) -> Result<Vec<db::TagRow>, String> {
    let db = db_path(&app)?;
    off_main(move || Ok(db::load_tags(&db::open(&db)?))).await
}

#[tauri::command]
async fn set_tags(
    app: tauri::AppHandle,
    tech_name: String,
    tags: Vec<String>,
) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let mut conn = db::open(&db)?;
        db::set_tags(&mut conn, &tech_name, &tags)
    })
    .await
}

// ── Manual category overrides ──
#[tauri::command]
async fn get_overrides(app: tauri::AppHandle) -> Result<Vec<db::CategoryOverride>, String> {
    let db = db_path(&app)?;
    off_main(move || Ok(db::load_overrides(&db::open(&db)?))).await
}

#[tauri::command]
async fn set_override(app: tauri::AppHandle, row: db::CategoryOverride) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || db::set_override(&db::open(&db)?, &row)).await
}

// ── Organize / projection engine (writes to the game folder) ──

/// The folders Silo is allowed to write to: the active layout the mods-root commands resolved
/// (user choice > FS25 override > default, see `modsroot`) — its mods folder, its archive and,
/// during an archive move, the other archive — or, before they have run, the auto-detected
/// default. This is the single source of truth for "where may we write"; Rust, not the
/// webview, is the authority.
fn allowed_roots() -> Vec<PathBuf> {
    match modsroot::active() {
        Some(a) => {
            let mut v = vec![a.layout.mods, a.layout.archive];
            v.extend(a.move_to);
            v
        }
        None => fsgame::default_mods_paths(),
    }
}

/// The mods folder(s) only — never an archive. Frontend-supplied roots and default scans
/// resolve against this.
fn mods_roots() -> Vec<PathBuf> {
    match modsroot::active() {
        Some(a) => vec![a.layout.mods],
        None => fsgame::default_mods_paths(),
    }
}

/// The active layout (mods folder + archive), validated against a frontend-supplied root.
fn primary_layout(root: Option<String>) -> Result<organize::Layout, String> {
    let mods = primary_root(root)?;
    Ok(match modsroot::active() {
        Some(a) if modsroot::same_folder(&a.layout.mods, &mods) => a.layout,
        _ => organize::Layout::default_for(&mods),
    })
}

const STILL_LOADING: &str = "Silo is still loading your library — try again in a moment.";

/// Refuse writes to the library while an archive move is unfinished — or before the active
/// layout has been worked out (an unfinished move can't be ruled out yet). Once resolved with
/// no usable mods folder there's nothing to move, so choosing a folder stays possible.
fn ensure_no_move() -> Result<(), String> {
    // A move that's starting, copying or finishing counts even before (or after) the layout
    // publishes its marker — it holds the library lock across both ends.
    if move_in_progress() {
        return Err("Finish or cancel the archive move first.".into());
    }
    match modsroot::active() {
        Some(a) if a.move_to.is_some() => Err("Finish or cancel the archive move first.".into()),
        Some(_) => Ok(()),
        None if !modsroot::resolved() => Err(STILL_LOADING.into()),
        None => Ok(()),
    }
}

/// Serializes every write to the library (organize, projections, installs, adopt, archive
/// moves). Commands take it inside their blocking body — never across an await; an archive
/// move only `try_lock`s, so it never waits behind, or starts under, another write.
static LIBRARY_WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Refusal when another change holds the library lock (a write that waited too long, or an
/// archive move that won't start under another write).
const LIBRARY_BUSY: &str =
    "Silo is busy with another change to your library. Try again in a moment.";

/// `try_move_lock`'s refusal while another move holds the library lock. Its own text (not
/// relocate's "already running") so `cancel_falls_back` matches this case and nothing else.
const MOVE_LOCK_HELD: &str = "An archive move is already under way.";

/// Set while an archive move holds the library write lock — including the checks before
/// relocate's own guard flips `is_running()` — so writes refuse at once instead of waiting.
static MOVE_HOLDS_LOCK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A move (or its pre-checks) holds the library lock, or relocate is copying.
fn move_in_progress() -> bool {
    MOVE_HOLDS_LOCK.load(std::sync::atomic::Ordering::SeqCst) || relocate::is_running()
}

/// Set by `force_quit` so the close guard lets the window go after the user said "Quit anyway".
static FORCE_QUIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True while Silo is changing files: a library write holds the lock, or an archive move is
/// running (including between its lock drops). Never waits for the lock.
fn library_busy() -> bool {
    busy_with(&LIBRARY_WRITE, move_in_progress())
}

/// [`library_busy`] against a given lock. A poisoned lock that nobody holds is not busy.
fn busy_with(lock: &std::sync::Mutex<()>, moving: bool) -> bool {
    moving || matches!(lock.try_lock(), Err(std::sync::TryLockError::WouldBlock))
}

/// Whether Silo is changing files right now (the frontend asks before a self-update).
#[tauri::command]
fn app_busy() -> bool {
    library_busy()
}

/// Quit even though Silo is busy, after the user confirmed it. `exit` raises `ExitRequested`,
/// which `hold_quit` lets through once `FORCE_QUIT` is set.
#[tauri::command]
fn force_quit(app: tauri::AppHandle) {
    FORCE_QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
    app.exit(0);
}

/// The library write lock as held by an archive move; clears `MOVE_HOLDS_LOCK` on drop
/// (before the lock itself is released).
struct MoveLock {
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl Drop for MoveLock {
    fn drop(&mut self) {
        MOVE_HOLDS_LOCK.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}

fn lock_result(
    r: std::sync::TryLockResult<std::sync::MutexGuard<'static, ()>>,
) -> Option<std::sync::MutexGuard<'static, ()>> {
    match r {
        Ok(g) => Some(g),
        Err(std::sync::TryLockError::Poisoned(e)) => Some(e.into_inner()),
        Err(std::sync::TryLockError::WouldBlock) => None,
    }
}

/// How long a write waits behind another one before giving up.
const LIBRARY_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// How long a finished download waits to be placed. Longer than `LIBRARY_WAIT` so a brief
/// busy spell never throws away a completed download (move/loading refusals stay instant).
const PLACE_WAIT: std::time::Duration = std::time::Duration::from_secs(600);

/// Take the library write lock, waiting behind another ordinary write (up to `LIBRARY_WAIT`)
/// but never behind an archive move (refused at once, like `ensure_no_move`), then re-check
/// that no move started meanwhile. Hold the guard for the whole write.
fn library_write() -> Result<std::sync::MutexGuard<'static, ()>, String> {
    library_write_within(LIBRARY_WAIT)
}

/// `library_write` with an explicit deadline. Once the lock is held, update leftovers in the
/// active layout are resolved first (`settle_updates_locked`), so no write starts on top of a
/// half-finished mod update.
fn library_write_within(
    max: std::time::Duration,
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    let guard = wait_for_library(max, || {
        ensure_not_self_updating()?;
        ensure_no_move()?;
        if move_in_progress() {
            return Err("Finish or cancel the archive move first.".into());
        }
        Ok(())
    })?;
    settle_updates_locked()?;
    Ok(guard)
}

/// Set while Silo installs its own update (`set_self_updating`): library writes and archive
/// moves refuse, so nothing starts changing files just before Silo restarts.
static SELF_UPDATING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

const SELF_UPDATE_RUNNING: &str = "Silo is installing an update.";

fn self_updating() -> bool {
    SELF_UPDATING.load(std::sync::atomic::Ordering::SeqCst)
}

fn ensure_not_self_updating() -> Result<(), String> {
    if self_updating() {
        Err(SELF_UPDATE_RUNNING.into())
    } else {
        Ok(())
    }
}

/// The frontend marks the start (and, if it fails, the end) of installing Silo's own update.
#[tauri::command]
fn set_self_updating(on: bool) {
    SELF_UPDATING.store(on, std::sync::atomic::Ordering::SeqCst);
}

/// Poll for the library lock until `max` has passed. `ready` runs before every attempt and
/// again once the lock is held; its refusal ends the wait at once.
fn wait_for_library(
    max: std::time::Duration,
    ready: impl Fn() -> Result<(), String>,
) -> Result<std::sync::MutexGuard<'static, ()>, String> {
    let start = std::time::Instant::now();
    loop {
        ready()?;
        if let Some(guard) = lock_result(LIBRARY_WRITE.try_lock()) {
            ready()?;
            return Ok(guard);
        }
        if start.elapsed() >= max {
            return Err(LIBRARY_BUSY.into());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

/// The library write lock for an archive move, without waiting: a busy library refuses.
fn try_move_lock() -> Result<MoveLock, String> {
    ensure_not_self_updating()?;
    match lock_result(LIBRARY_WRITE.try_lock()) {
        Some(g) => {
            // A self-update may have started between the check above and taking the lock.
            if self_updating() {
                drop(g);
                return Err(SELF_UPDATE_RUNNING.into());
            }
            // Order matters: clear requests left from before (a pause or cancel nothing was
            // running to receive) only once the lock is ours, and only then announce the move —
            // a cancel that sees the flag from here on is sent to this move and kept.
            relocate::requests().store(0, std::sync::atomic::Ordering::SeqCst);
            MOVE_HOLDS_LOCK.store(true, std::sync::atomic::Ordering::SeqCst);
            Ok(MoveLock { _guard: g })
        }
        None if move_in_progress() => Err(MOVE_LOCK_HELD.into()),
        None => Err(LIBRARY_BUSY.into()),
    }
}

/// Refuse organize writes and new archive moves while a custom archive's drive is missing
/// (nothing is deleted; the banner offers Retry / Choose another), or while no layout is known.
fn ensure_archive_available() -> Result<(), String> {
    match modsroot::active() {
        None if !modsroot::resolved() => Err(STILL_LOADING.into()),
        None => Err("No mods folder selected".into()),
        Some(a) if a.layout.archive_missing() => Err(archive_unavailable(&a.layout.archive)),
        Some(_) => Ok(()),
    }
}

/// Pure: why nothing can be filed into or moved out of a missing archive, and what to do.
fn archive_unavailable(archive: &std::path::Path) -> String {
    format!(
        "Your library archive at {} isn't available. Reconnect its drive, or use Choose another to point Silo at where it is now.",
        archive.display()
    )
}

/// Resolve a frontend-supplied mods folder to a TRUSTED one. Empty → the first detected
/// folder. Otherwise the path must canonicalize to one of the allowed mods folders (never an
/// archive), or it's rejected — so a compromised webview can't redirect an install/organize
/// into an arbitrary directory. Returns the matched trusted path, never the raw frontend string.
fn primary_root(root: Option<String>) -> Result<PathBuf, String> {
    let allowed = mods_roots();
    match root {
        Some(r) if !r.is_empty() => {
            let want = PathBuf::from(&r)
                .canonicalize()
                .map_err(|_| "That mods folder doesn't exist".to_string())?;
            allowed
                .into_iter()
                .find(|a| a.canonicalize().map(|ac| ac == want).unwrap_or(false))
                .ok_or_else(|| {
                    "refusing a mods folder outside the detected game folder(s)".to_string()
                })
        }
        _ => allowed
            .into_iter()
            .next()
            .ok_or_else(|| "No mods folder detected".to_string()),
    }
}

/// Dry run: what organizing would move (read-only).
#[tauri::command]
async fn plan_organize(
    root: Option<String>,
    mods: Vec<organize::ModInput>,
) -> Result<Vec<organize::PlannedMove>, String> {
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || organize::plan_organize(&root, &mods))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn apply_organize(
    app: tauri::AppHandle,
    root: Option<String>,
    mods: Vec<organize::ModInput>,
) -> Result<organize::Report, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        Ok(organize::apply_organize(
            &conn,
            &layout,
            &layout.mods,
            &mods,
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Adopt the file the user swapped into the mods folder as the mod's new canonical version
/// (old copy kept under backups/). The caller should rescan afterward.
#[tauri::command]
async fn adopt_foreign_file(
    app: tauri::AppHandle,
    root: Option<String>,
    file_name: String,
) -> Result<(), String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        organize::adopt_foreign(&conn, &layout, &file_name)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Restore Silo's managed copy over a file the user swapped in (their file kept under
/// backups/). The caller should rescan afterward.
#[tauri::command]
async fn restore_foreign_file(
    app: tauri::AppHandle,
    root: Option<String>,
    file_name: String,
) -> Result<(), String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        organize::restore_projection(&conn, &layout, &file_name)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn set_active(
    app: tauri::AppHandle,
    root: Option<String>,
    active: Vec<String>,
) -> Result<organize::Report, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        let set: HashSet<String> = active.into_iter().collect();
        Ok(organize::set_active(&conn, &layout, &set))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Whether the mods folder supports hardlinks. False on cloud-synced folders (OneDrive / Google
/// Drive / Proton Drive), where Silo projects the active set as copies instead.
#[tauri::command]
async fn hardlink_support(root: Option<String>) -> Result<bool, String> {
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || Ok(organize::supports_hardlinks(&root)))
        .await
        .map_err(|e| e.to_string())?
}

/// Repair copy-projections into hardlinks (reclaims duplicated space, makes activate/deactivate
/// O(1)). Emits `relink-progress` `(done, total)` events so the UI can show a bar during the
/// one-time hash-and-relink of a large library.
#[tauri::command]
async fn relink_projections(
    app: tauri::AppHandle,
    root: Option<String>,
) -> Result<organize::RelinkReport, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::RelinkReport, String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        Ok(organize::relink_projections(
            &conn,
            &layout,
            |done, total| {
                let _ = emitter.emit("relink-progress", (done, total));
            },
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn flatten(app: tauri::AppHandle, root: Option<String>) -> Result<organize::Report, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let _write = library_write()?;
        ensure_archive_available()?;
        let layout = primary_layout(root)?;
        let conn = db::open(&db)?;
        Ok(organize::flatten(&conn, &layout))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Mods folder selection (#162) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct KnownRoot {
    path: String,
    mod_count: i64,
    current: bool,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ModsRootStatus {
    path: Option<String>,
    source: modsroot::Source,
    game_reads: Option<String>,
    mismatch: bool,
    available: bool,
    /// The FS25 user dir exists but has no `mods/` yet (common on Mac until a first download).
    mods_folder_missing: bool,
    known: Vec<KnownRoot>,
    can_write_game_setting: bool,
    game_setting_problem: Option<String>,
    unresolved_override: Option<String>,
}

fn path_map(loc: &fsgame::GameLocation) -> modsroot::PathMap {
    match &loc.prefix {
        Some(p) => modsroot::PathMap::from_prefix(p),
        None => modsroot::PathMap::identity(),
    }
}

fn loose_zip_count(dir: &std::path::Path) -> i64 {
    std::fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .filter(|e| {
                    e.path()
                        .extension()
                        .map(|x| x.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
                })
                .count() as i64
        })
        .unwrap_or(0)
}

/// The trusted layout for mods folder `mods`: during an unfinished move its archive is where
/// the move started (the other one is `move_to`); otherwise the stored custom archive, or the
/// default. A stored archive that fails `Layout::new`'s checks is ignored (default used).
fn resolve_active(conn: &rusqlite::Connection, mods: &std::path::Path) -> modsroot::Active {
    let key = modsroot::canonical_key(mods);
    let marker = relocate::load_marker(conn, &key);
    let stored = match &marker {
        Some(m) => Some(m.from.clone()),
        None => match db::get_app_setting(conn, &relocate::archive_setting_key(&key)) {
            Some(s) => Some(PathBuf::from(s)),
            // No setting: a lost database may have forgotten a custom archive the notes name.
            None => relocate::rediscover_archive(mods)
                .filter(|found| rediscovered_archive_fits(conn, mods, found))
                .inspect(|found| {
                    if relocate::store_archive_setting(conn, &key, mods, found).is_ok() {
                        notices::push(format!(
                            "Silo found your library archive at {} and is using it again.",
                            found.display()
                        ));
                    }
                }),
        },
    };
    let layout = match stored {
        Some(archive) => organize::Layout::new(mods.to_path_buf(), archive).unwrap_or_else(|e| {
            log::warn!(
                "ignoring the library archive stored for {}: {e}",
                mods.display()
            );
            organize::Layout::default_for(mods)
        }),
        None => organize::Layout::default_for(mods),
    };
    modsroot::Active {
        layout,
        move_to: marker.map(|m| m.to),
    }
}

/// An archive found again from its notes passes the same checks as one the user picks: the
/// `Layout` rules and the fences (other mods folders and their archives, FS25's own folder,
/// Silo's data folder, which is where the database lives).
fn rediscovered_archive_fits(
    conn: &rusqlite::Connection,
    mods: &std::path::Path,
    found: &std::path::Path,
) -> bool {
    let layout = organize::Layout::default_for(mods);
    let app_data = conn
        .path()
        .map(std::path::Path::new)
        .and_then(std::path::Path::parent)
        .filter(|p| !p.as_os_str().is_empty());
    let problems = fence_problems(&layout, found, &fences(conn, &layout, app_data));
    if !problems.is_empty() {
        log::warn!(
            "not using the archive {} named in {}: {}",
            found.display(),
            relocate::ARCHIVE_NOTE,
            problems.join(" ")
        );
    }
    problems.is_empty()
}

/// Resolve the live mods folder, publish it (and its archive) as the trusted write roots, and
/// describe it.
fn compute_mods_root_status(conn: &rusqlite::Connection) -> ModsRootStatus {
    let loc = fsgame::locate();
    let default = loc.as_ref().map(|l| l.user_dir.join("mods"));
    let map = loc.as_ref().map(path_map).unwrap_or_default();
    let (ov, game_setting_problem) = match loc.as_ref() {
        None => (None, Some("FS25 user folder not found".to_string())),
        Some(l) => match std::fs::read_to_string(l.user_dir.join("gameSettings.xml")) {
            Err(_) => (
                None,
                Some("gameSettings.xml not found — start FS25 once to create it".to_string()),
            ),
            Ok(xml) => match modsroot::read_override(&xml) {
                Some(o) => (Some(o), None),
                None => (
                    None,
                    Some("gameSettings.xml has no modsDirectoryOverride setting".to_string()),
                ),
            },
        },
    };
    // Rows from before folders were tracked belong to the default folder: it was the only one.
    if let Some(d) = default.as_ref() {
        let _ = db::claim_unscoped(conn, &modsroot::canonical_key(d));
    }
    let choice = db::get_app_setting(conn, modsroot::CHOICE_KEY).map(PathBuf::from);
    let r = modsroot::resolve(
        choice,
        ov.as_ref(),
        default.clone(),
        &map,
        modsroot::same_folder,
    );
    let available = r.path.as_ref().map(|p| p.is_dir()).unwrap_or(false);
    let mods_folder_missing = !available
        && r.source == modsroot::Source::Default
        && loc.as_ref().map(|l| l.user_dir.is_dir()).unwrap_or(false);
    modsroot::set_active(if available {
        r.path.as_deref().map(|p| resolve_active(conn, p))
    } else {
        None
    });

    let current_key = r.path.as_ref().map(|p| modsroot::canonical_key(p));
    let mut paths: Vec<PathBuf> =
        modsroot::parse_known(db::get_app_setting(conn, modsroot::KNOWN_KEY).as_deref())
            .into_iter()
            .map(PathBuf::from)
            .collect();
    for p in default.iter().chain(r.path.iter()) {
        if p.is_dir() && !paths.iter().any(|k| modsroot::same_folder(k, p)) {
            paths.push(p.clone());
        }
    }
    let known = paths
        .iter()
        .map(|p| {
            let key = modsroot::canonical_key(p);
            KnownRoot {
                path: p.to_string_lossy().into_owned(),
                mod_count: db::count_organized(conn, &key) + loose_zip_count(p),
                current: current_key.as_deref() == Some(key.as_str()),
            }
        })
        .collect();

    ModsRootStatus {
        path: r.path.map(|p| p.to_string_lossy().into_owned()),
        source: r.source,
        game_reads: r.game_reads.map(|p| p.to_string_lossy().into_owned()),
        mismatch: r.mismatch,
        available,
        mods_folder_missing,
        known,
        can_write_game_setting: game_setting_problem.is_none(),
        game_setting_problem,
        unresolved_override: r.unresolved_override,
    }
}

/// Point FS25's `modsDirectoryOverride` at `folder`. `guarded_xml_write` keeps the untouched
/// original (`gameSettings.xml.silo-original.bak`) and the last three versions (`.bak`,
/// `.bak.2`, `.bak.3`), then swaps the new file in atomically.
fn point_game_at(folder: &std::path::Path) -> Result<(), String> {
    let loc = fsgame::locate().ok_or_else(|| "FS25 user folder not found".to_string())?;
    let gs = loc.user_dir.join("gameSettings.xml");
    let roots = [loc.user_dir.clone()];
    let xml = paths::guarded_xml_read(&roots, &gs)?;
    let out = modsroot::write_override(&xml, &path_map(&loc).to_game(folder))?;
    paths::guarded_xml_write(&roots, &gs, &out)
}

/// The live mods folder and everything the Settings / banners need to describe it.
#[tauri::command]
async fn mods_root_status(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Use `path` as the mods folder (remembered, added to the known list). Never moves files.
#[tauri::command]
async fn set_mods_root(app: tauri::AppHandle, path: String) -> Result<ModsRootStatus, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db)?;
        let p = std::path::Path::new(&path);
        let known: Vec<PathBuf> =
            modsroot::parse_known(db::get_app_setting(&conn, modsroot::KNOWN_KEY).as_deref())
                .into_iter()
                .map(PathBuf::from)
                .collect();
        let default = fsgame::user_dir().map(|d| d.join("mods"));
        if !mods_root_choice_allowed(
            p,
            picked::allows(picked::Purpose::ModsFolder, p),
            &known,
            default.as_deref(),
        ) {
            return Err(picked::NOT_PICKED.into());
        }
        modsroot::validate_folder(p)?;
        let known = modsroot::add_known(
            db::get_app_setting(&conn, modsroot::KNOWN_KEY).as_deref(),
            &path,
        );
        db::set_app_setting(&conn, modsroot::KNOWN_KEY, Some(&known))?;
        db::set_app_setting(&conn, modsroot::CHOICE_KEY, Some(&path))?;
        // Used: a later call can't replay it (the folder is in the known list now anyway).
        picked::forget(picked::Purpose::ModsFolder);
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Forget the user's choice and follow the folder FS25 reads.
#[tauri::command]
async fn clear_mods_root(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let conn = db::open(&db)?;
        db::set_app_setting(&conn, modsroot::CHOICE_KEY, None)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Point FS25 at the folder Silo currently manages.
#[tauri::command]
async fn point_game_at_mods_root(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        config_write_refusal(gamelaunch::fs25_running())?;
        let folder = modsroot::active()
            .map(|a| a.layout.mods)
            .ok_or_else(|| "No mods folder selected".to_string())?;
        point_game_at(&folder)?;
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Create `<user dir>/mods` when FS25 hasn't made it yet.
#[tauri::command]
async fn create_mods_folder(app: tauri::AppHandle) -> Result<ModsRootStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let loc = fsgame::locate().ok_or_else(|| "FS25 user folder not found".to_string())?;
        std::fs::create_dir_all(loc.user_dir.join("mods")).map_err(|e| e.to_string())?;
        let conn = db::open(&db)?;
        Ok(compute_mods_root_status(&conn))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// True when a folder holds no mods yet (no `.zip`, no folder with a `modDesc.xml`, no
/// `archive/`), so the UI can ask "use it anyway?".
#[tauri::command]
async fn folder_looks_empty(path: String) -> Result<bool, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = std::path::Path::new(&path);
        let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
        let has_mod = rd.flatten().any(|e| {
            let p = e.path();
            e.file_name() == "archive"
                || p.extension()
                    .map(|x| x.eq_ignore_ascii_case("zip"))
                    .unwrap_or(false)
                || p.join("modDesc.xml").is_file()
        });
        Ok(!has_mod)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Show the native folder picker and remember the choice for `purpose` (see `picked`).
#[tauri::command]
async fn pick_folder(
    window: tauri::Window,
    purpose: picked::Purpose,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    // `blocking_pick_folder` posts the dialog to the main thread and waits for the answer, so
    // it must never run there; `off_main` runs it on the blocking pool.
    off_main(move || {
        let builder = window.dialog().file();
        // Modal to Silo's window where the platform supports it (as the JS dialog does).
        #[cfg(any(windows, target_os = "macos"))]
        let builder = builder.set_parent(&window);
        let Some(fp) = builder.blocking_pick_folder() else {
            return Ok(None);
        };
        let path = fp.into_path().map_err(|e| e.to_string())?;
        picked::remember(purpose, &path);
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

/// The mods folder may change to a folder picked in Silo's picker, one Silo already knows (the
/// known list, which only `set_mods_root` adds to), or FS25's default folder.
fn mods_root_choice_allowed(
    path: &std::path::Path,
    picked: bool,
    known: &[PathBuf],
    default: Option<&std::path::Path>,
) -> bool {
    picked
        || default.is_some_and(|d| modsroot::same_folder(d, path))
        || known.iter().any(|k| modsroot::same_folder(k, path))
}

const FOLDER_MISSING: &str = "That folder isn't there right now.";

/// The folders Settings can open, named by the webview, resolved by Rust.
#[derive(serde::Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
enum KnownFolder {
    ModsFolder,
    Archive,
    GameInstall,
}

fn existing_dir(p: Option<PathBuf>) -> Result<PathBuf, String> {
    p.filter(|p| p.is_dir())
        .ok_or_else(|| FOLDER_MISSING.to_string())
}

fn known_folder_path(which: KnownFolder) -> Result<PathBuf, String> {
    let active = modsroot::active();
    existing_dir(match which {
        KnownFolder::ModsFolder => active.map(|a| a.layout.mods),
        KnownFolder::Archive => active.map(|a| a.layout.archive),
        KnownFolder::GameInstall => gamelaunch::detect().map(|g| PathBuf::from(g.install_dir)),
    })
}

/// Open one of Silo's folders in the file manager. Rust picks the path; the webview only names
/// which folder, so it can't open (or run) anything else.
#[tauri::command]
async fn open_known_folder(app: tauri::AppHandle, which: KnownFolder) -> Result<(), String> {
    use tauri_plugin_opener::OpenerExt;
    // Resolving the game folder reads Steam's files, and the shell call can stall: both off main.
    off_main(move || {
        let path = known_folder_path(which)?;
        app.opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_err(|e| format!("Couldn't open {}: {e}", path.display()))
    })
    .await
}

// ── Library archive location (#166) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveStatus {
    /// The active archive — during an unfinished move, where the move started.
    path: Option<String>,
    is_default: bool,
    /// False when a custom archive's folder is missing (its drive unplugged).
    available: bool,
    /// The unfinished move (paused, or copying right now — see `running`).
    moving: Option<relocate::MoveMarker>,
    /// A move is copying right now (Pause applies, Launch is blocked). `moving` set with
    /// `running` false means the move is paused (Resume / Cancel apply).
    running: bool,
}

/// The active layout and its mods-folder key, or why there isn't one yet.
fn active_layout() -> Result<(organize::Layout, String), String> {
    match modsroot::active() {
        Some(a) => {
            let key = modsroot::canonical_key(&a.layout.mods);
            Ok((a.layout, key))
        }
        None if !modsroot::resolved() => Err(STILL_LOADING.into()),
        None => Err("No mods folder selected".into()),
    }
}

/// A folder's identity key as a path, for component-wise `starts_with` comparisons.
fn key_path(p: &std::path::Path) -> PathBuf {
    PathBuf::from(modsroot::canonical_key(p))
}

/// Folders a new archive must keep clear of, as identity keys (`key_path`).
struct Fences {
    /// The active mods folder.
    mods: PathBuf,
    current_archive: PathBuf,
    /// Every known mods folder except the active one (the active one is `path_problems`').
    other_mods: Vec<PathBuf>,
    /// Those folders' archives (chosen, or their default).
    other_archives: Vec<PathBuf>,
    user_dir: Option<PathBuf>,
    app_data: Option<PathBuf>,
}

fn fences(
    conn: &rusqlite::Connection,
    layout: &organize::Layout,
    app_data: Option<&std::path::Path>,
) -> Fences {
    let user_dir = fsgame::locate().map(|l| l.user_dir);
    let mut others: Vec<PathBuf> =
        modsroot::parse_known(db::get_app_setting(conn, modsroot::KNOWN_KEY).as_deref())
            .into_iter()
            .map(PathBuf::from)
            .collect();
    // FS25's default mods folder counts even when it was never picked.
    others.extend(user_dir.as_ref().map(|u| u.join("mods")));
    let mine = key_path(&layout.mods);
    let mut other_mods: Vec<PathBuf> = Vec::new();
    let mut other_archives: Vec<PathBuf> = Vec::new();
    for p in others {
        let k = key_path(&p);
        if k == mine || other_mods.contains(&k) {
            continue;
        }
        let archive = db::get_app_setting(
            conn,
            &relocate::archive_setting_key(&modsroot::canonical_key(&p)),
        )
        .map(PathBuf::from)
        .unwrap_or_else(|| organize::Layout::default_for(&p).archive);
        other_mods.push(k);
        other_archives.push(key_path(&archive));
    }
    Fences {
        mods: mine,
        current_archive: key_path(&layout.archive),
        other_mods,
        other_archives,
        user_dir: user_dir.as_deref().map(key_path),
        app_data: app_data.map(key_path),
    }
}

/// Pure: where a new archive (`new`, an identity key) runs into a folder it must keep clear
/// of. Equality with the current archive / another archive is `path_problems`' to report.
fn overlap_problems(new: &std::path::Path, f: &Fences) -> Vec<String> {
    if new.parent().is_none() {
        return vec!["Pick a folder, not a whole drive.".into()];
    }
    let overlaps = |a: &PathBuf| new == a || new.starts_with(a) || a.starts_with(new);
    let nests = |a: &PathBuf| new != a && (new.starts_with(a) || a.starts_with(new));
    let mut v = Vec::new();
    if nests(&f.current_archive) {
        v.push(
            "Pick a folder that isn't inside your current archive and doesn't contain it.".into(),
        );
    }
    if f.other_mods.iter().any(overlaps) {
        v.push("That folder overlaps another mods folder Silo knows — pick one outside it.".into());
    }
    if f.other_archives.iter().any(nests) {
        v.push("That folder overlaps another mods folder's archive — pick one outside it.".into());
    }
    // The FS25 user dir, a folder containing it, or anything inside it — except inside a
    // mods folder (the default `<mods>/archive`; a wrong spot there gets its own message).
    let in_mods = new.starts_with(&f.mods) || f.other_mods.iter().any(|m| new.starts_with(m));
    if !in_mods && f.user_dir.as_ref().is_some_and(overlaps) {
        v.push("Pick a folder outside Farming Simulator 25's own folder.".into());
    }
    if f.app_data.as_ref().is_some_and(overlaps) {
        v.push("Pick a folder outside Silo's own data folder.".into());
    }
    v
}

fn foreign_mods_problem(n: usize) -> String {
    let (count, them) = if n == 1 {
        ("1 mod".to_string(), "it")
    } else {
        (format!("{n} mods"), "them")
    };
    format!(
        "This folder already has {count} in subfolders Silo doesn't manage — choose another folder, or move {them} to the folder's top level so Silo can file {them} in."
    )
}

/// Where a folder can't be the archive at all: `path_problems` (already there, inside the mods
/// folder, another folder's archive, `Layout::new`'s rules) plus the `Fences` overlaps. Shared
/// by a move and a re-point. Mostly pure (`same_folder` canonicalizes).
fn fence_problems(layout: &organize::Layout, new: &std::path::Path, f: &Fences) -> Vec<String> {
    let mut v = relocate::path_problems(
        &layout.archive,
        new,
        &layout.mods,
        &f.other_archives,
        modsroot::same_folder,
    );
    v.extend(overlap_problems(&key_path(new), f));
    v
}

/// Everything about the folder itself that rules it out as a move's destination. Checked
/// before any probe writes a file there.
fn target_problems(
    conn: &rusqlite::Connection,
    layout: &organize::Layout,
    new: &std::path::Path,
    app_data: Option<&std::path::Path>,
) -> Vec<String> {
    let f = fences(conn, layout, app_data);
    let mut v = fence_problems(layout, new, &f);
    // Mods filed in `<new>/<subfolder>/` that Silo doesn't know would mix with the library.
    // (Loose mods at the top level are fine — that's adopt.)
    let foreign = relocate::unknown_mods(new, &[]).len();
    if foreign > 0 {
        v.push(foreign_mods_problem(foreign));
    }
    v
}

/// The free-space facts a move depends on (see `move_problems`).
struct Space {
    /// The old and new archive share a drive.
    archive_same_drive: bool,
    /// The whole archive.
    bytes: u64,
    /// The largest single mod.
    largest: u64,
    /// Free on the new archive's drive (unknown never blocks).
    free: Option<u64>,
}

/// Pure inputs to `move_problems`.
struct MoveFacts {
    /// Problems with the folder itself (`target_problems`, the write probe).
    target: Vec<String>,
    /// Mods in the archive the library doesn't know, phrased (`relocate::unknown_messages`).
    unknown: Vec<String>,
    /// Library mods whose archived copy is missing (the move would pause on each).
    missing: usize,
    /// Names already at the destination.
    collisions: Vec<String>,
    space: Space,
}

fn missing_mods_problem(n: usize) -> String {
    if n == 1 {
        "1 mod in your library is missing from the archive — rescan first.".into()
    } else {
        format!("{n} mods in your library are missing from the archive — rescan first.")
    }
}

/// Pure: everything that blocks a move, in the order the dialog lists them: problems with the
/// folder itself, mods the library doesn't know (they'd be left behind), library mods missing
/// from the archive (each would stop the move), names already at the destination, then free
/// space. On one drive each mod's old copy is deleted before the next is copied — and an active
/// mod's hardlink is re-linked to the new copy first, so the delete really frees it — so only the
/// largest mod needs room at a time; across drives the whole archive must fit. The mods folder's
/// drive needs nothing extra: across drives an active mod's hardlink becomes a symlink, or —
/// where only a copy could be made — is left as the mod's standalone copy (`relocate::repoint`).
fn move_problems(f: MoveFacts) -> Vec<String> {
    let mut v = f.target;
    v.extend(f.unknown);
    if f.missing > 0 {
        v.push(missing_mods_problem(f.missing));
    }
    v.extend(f.collisions);
    let s = f.space;
    let needed = if s.archive_same_drive {
        s.largest
    } else {
        s.bytes
    };
    if needed > 0 {
        v.extend(relocate::space_problem(needed, s.free));
    }
    v
}

/// Pure: both mounts known and equal. An unknown mount is NOT the same drive (so the space
/// check still runs) — unlike `organize::same_volume`, which assumes same for link fallback.
fn known_same_mount(a: &Option<(PathBuf, u64)>, b: &Option<(PathBuf, u64)>) -> bool {
    matches!((a, b), (Some((ma, _)), Some((mb, _))) if ma == mb)
}

/// The facts a move to `new` depends on (the dialog's preflight, and the start command's
/// server-side re-check — the webview isn't trusted to have run the preflight).
struct MoveCheck {
    mod_count: usize,
    bytes: u64,
    free: Option<u64>,
    /// `new` shares a drive with the mods folder (hardlinks work).
    same_drive: bool,
    problems: Vec<String>,
}

fn check_move(
    conn: &rusqlite::Connection,
    layout: &organize::Layout,
    key: &str,
    new: &std::path::Path,
    target: Vec<String>,
) -> MoveCheck {
    let rows = db::load_organized(conn, key);
    let archived =
        |r: &db::OrganizedRow| organize::archive_path(&layout.archive, &r.category, &r.file_name);
    let sizes: Vec<u64> = rows
        .iter()
        .map(|r| relocate::size_of(&archived(r)))
        .collect();
    let bytes: u64 = sizes.iter().sum();
    let missing = rows
        .iter()
        .filter(|r| archived(r).symlink_metadata().is_err())
        .count();
    let new_mount = organize::mount_of(new);
    let free = new_mount.as_ref().map(|(_, f)| *f);
    let same_drive = known_same_mount(&organize::mount_of(&layout.mods), &new_mount);
    let archive_same_drive = known_same_mount(&organize::mount_of(&layout.archive), &new_mount);
    let unknown = relocate::unknown_mods(&layout.archive, &rows);
    let problems = move_problems(MoveFacts {
        target,
        unknown: relocate::unknown_messages(
            &unknown,
            &rows,
            "then try again.",
            "rescan, then try again.",
        ),
        missing,
        collisions: relocate::collisions(new, &rows),
        space: Space {
            archive_same_drive,
            bytes,
            largest: sizes.iter().copied().max().unwrap_or(0),
            free,
        },
    });
    MoveCheck {
        mod_count: rows.len(),
        bytes,
        free,
        same_drive,
        problems,
    }
}

/// Pure: the log line for a start / resume / cancel outcome.
fn move_outcome(what: &str, out: &Result<Option<relocate::MoveMarker>, String>) -> String {
    match out {
        Ok(None) => format!("archive move {what}: finished"),
        Ok(Some(m)) => {
            let reason = match (&m.pause_reason, m.errors.is_empty()) {
                (Some(r), _) => r.as_str(),
                (None, false) => "stopped on errors",
                (None, true) => "on request",
            };
            format!(
                "archive move {what}: paused ({reason}; {} error(s))",
                m.errors.len()
            )
        }
        Err(e) => format!("archive move {what}: refused — {e}"),
    }
}

/// Run `f` with the production move context (the command layer's request flag, the real
/// FS25 process check, progress emitted as `archive-move-progress`) under the library write
/// lock — refused, never waited for, when another write holds it — then re-resolve the
/// published layout (the archive setting may have switched, the marker paused or cleared)
/// before releasing the lock. Logs the outcome.
fn with_move_ctx(
    app: &tauri::AppHandle,
    db: &std::path::Path,
    what: &str,
    f: impl FnOnce(
        &mut relocate::RunCtx<'_>,
        &organize::Layout,
    ) -> Result<Option<relocate::MoveMarker>, String>,
) -> Result<Option<relocate::MoveMarker>, String> {
    let run = || {
        let _write = try_move_lock()?;
        let (layout, key) = active_layout()?;
        let conn = db::open(db)?;
        let emitter = app.clone();
        let mut progress = move |p: &relocate::Progress| {
            let _ = emitter.emit("archive-move-progress", p.clone());
        };
        let mut ctx = relocate::RunCtx {
            conn: &conn,
            mods: &layout.mods,
            mods_key: &key,
            game_running: &gamelaunch::fs25_running,
            game_check_interval: std::time::Duration::from_secs(2),
            request: relocate::requests(),
            progress: &mut progress,
        };
        let out = f(&mut ctx, &layout);
        let _ = compute_mods_root_status(&conn);
        out
    };
    let out = run();
    // A cancel that finds a move holding the lock becomes a request (logged there), not a
    // refusal.
    let handed_off = what == "cancel" && matches!(&out, Err(e) if cancel_falls_back(e));
    if !handed_off {
        match &out {
            Ok(None) => log::info!("{}", move_outcome(what, &out)),
            _ => log::warn!("{}", move_outcome(what, &out)),
        }
    }
    out
}

/// Start moving the archive to `new` (already `modsroot::real_folder`-normalized, or for
/// `create`, a normalized mods folder joined with `archive`) and run until it finishes
/// (`None`) or pauses (the marker). Refuses — writing nothing, not even a probe — while a
/// move is unfinished or the folder is ruled out; `create` makes the folder only after that.
fn start_move(
    app: &tauri::AppHandle,
    db: &std::path::Path,
    new: PathBuf,
    create: bool,
) -> Result<Option<relocate::MoveMarker>, String> {
    ensure_no_move()?;
    let app_data = app.path().app_data_dir().ok();
    with_move_ctx(app, db, "start", |ctx, layout| {
        // Nothing to move out of a missing archive — and every mod would look "missing".
        if layout.archive_missing() {
            return Err(archive_unavailable(&layout.archive));
        }
        let target = target_problems(ctx.conn, layout, &new, app_data.as_deref());
        if !target.is_empty() {
            return Err(target.join("\n"));
        }
        if create {
            std::fs::create_dir_all(&new)
                .map_err(|e| format!("Couldn't create {}: {e}", new.display()))?;
        }
        modsroot::validate_folder(&new)?;
        let check = check_move(ctx.conn, layout, ctx.mods_key, &new, Vec::new());
        if !check.problems.is_empty() {
            return Err(check.problems.join("\n"));
        }
        log::info!(
            "moving the library archive {} -> {}",
            layout.archive.display(),
            new.display()
        );
        relocate::start(ctx, layout.archive.clone(), new)
    })
}

#[tauri::command]
async fn archive_status(app: tauri::AppHandle) -> Result<ArchiveStatus, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let running = relocate::is_running();
        if !modsroot::resolved() {
            return Err(STILL_LOADING.to_string());
        }
        let Some(a) = modsroot::active() else {
            return Ok(ArchiveStatus {
                path: None,
                is_default: true,
                available: false,
                moving: None,
                running,
            });
        };
        let conn = db::open(&db)?;
        let key = modsroot::canonical_key(&a.layout.mods);
        let is_default = a.layout.archive_is_default();
        Ok(ArchiveStatus {
            path: Some(a.layout.archive.to_string_lossy().into_owned()),
            is_default,
            available: is_default || a.layout.archive.is_dir(),
            moving: relocate::load_marker(&conn, &key),
            running,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// What moving the archive to `path` involves, and anything that blocks it. Read-only apart
/// from the write probe and the symlink probe (both removed again), which only run when the
/// folder itself isn't ruled out.
#[tauri::command]
async fn archive_move_preflight(
    app: tauri::AppHandle,
    path: String,
) -> Result<relocate::Preflight, String> {
    let db = db_path(&app)?;
    let app_data = app.path().app_data_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        ensure_no_move()?;
        ensure_archive_available()?;
        let (layout, key) = active_layout()?;
        let new = modsroot::real_folder(std::path::Path::new(&path))?;
        let conn = db::open(&db)?;
        let mut target = target_problems(&conn, &layout, &new, app_data.as_deref());
        let probed = target.is_empty()
            && match modsroot::validate_folder(&new) {
                Ok(()) => true,
                Err(e) => {
                    target.push(e);
                    false
                }
            };
        // Refused as a move but fine as a re-point: an archive holding no mods (nothing to
        // move or orphan) and a folder clear of every fence — its own filed mods are welcome.
        let can_repoint = !archive_holds_mods(&layout)
            && fence_problems(&layout, &new, &fences(&conn, &layout, app_data.as_deref()))
                .is_empty();
        let check = check_move(&conn, &layout, &key, &new, target);
        let symlinks_available = probed
            && organize::supports_symlinks(&organize::Layout {
                mods: layout.mods.clone(),
                archive: new.clone(),
            });
        Ok(relocate::Preflight {
            mod_count: check.mod_count,
            bytes: check.bytes,
            free_bytes: check.free,
            same_drive: check.same_drive,
            symlinks_available,
            loose_mods: relocate::loose_mods(&new).len(),
            problems: check.problems,
            can_repoint,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Move the archive to `path`; runs until the move finishes (`null`) or pauses (the marker).
#[tauri::command]
async fn archive_move_start(
    app: tauri::AppHandle,
    path: String,
) -> Result<Option<relocate::MoveMarker>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let new = modsroot::real_folder(std::path::Path::new(&path))?;
        if !picked::allows(picked::Purpose::Archive, &new) {
            return Err(picked::NOT_PICKED.into());
        }
        let moved = start_move(&app, &db, new, false)?;
        // Used: a later call can't replay it. (A refused start keeps it for "use as it is".)
        picked::forget(picked::Purpose::Archive);
        Ok(moved)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resume the paused move (in whichever direction it was going).
#[tauri::command]
async fn archive_move_resume(
    app: tauri::AppHandle,
) -> Result<Option<relocate::MoveMarker>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        with_move_ctx(&app, &db, "resume", |ctx, _| relocate::resume(ctx))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Ask the running move to pause at the next mod boundary.
#[tauri::command]
fn archive_move_pause() {
    relocate::request_pause();
}

/// Cancel: a running move is asked to turn around (returns `null` at once — `archive_status`
/// shows when it lands); a paused one is run back home here.
#[tauri::command]
async fn archive_move_cancel(
    app: tauri::AppHandle,
) -> Result<Option<relocate::MoveMarker>, String> {
    // A move holds the library lock; reach it through the request flag instead.
    if move_in_progress() {
        return Ok(request_turn_around());
    }
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        match with_move_ctx(&app, &db, "cancel", |ctx, _| relocate::cancel(ctx)) {
            // A start/resume took the lock in between (its checks may not have flipped
            // `is_running()` yet): ask it to turn around rather than report "busy".
            Err(e) if cancel_falls_back(&e) => Ok(request_turn_around()),
            other => other,
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Pure: a cancel refused because a move holds the library lock becomes a request to that move.
/// (An ordinary write holding it stays an error, so the cancel isn't silently dropped.)
fn cancel_falls_back(err: &str) -> bool {
    err == MOVE_LOCK_HELD
}

fn request_turn_around() -> Option<relocate::MoveMarker> {
    relocate::request_cancel();
    log::info!("archive move cancel: asked the running move to turn around");
    None
}

/// Move the archive back to the default `<mods>/archive` (created once the checks pass).
#[tauri::command]
async fn archive_use_default(
    app: tauri::AppHandle,
) -> Result<Option<relocate::MoveMarker>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        ensure_no_move()?;
        let (layout, _) = active_layout()?;
        if layout.archive_is_default() {
            return Err("Your library archive is already in its default place.".into());
        }
        ensure_archive_available()?;
        let mods = modsroot::real_folder(&layout.mods)?;
        let default = organize::Layout::default_for(&mods).archive;
        start_move(&app, &db, default, true)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Any mod (zip, or folder with a `modDesc.xml`) filed in one of the archive's category
/// folders — the same candidates a move would carry (Silo's own temps don't count). A missing
/// folder holds none; one that exists but can't be read is assumed to hold mods, so nothing
/// is ever re-pointed away from mods Silo couldn't see.
fn archive_holds_mods(layout: &organize::Layout) -> bool {
    relocate::filed_mods(&layout.archive)
        .map(|v| !v.is_empty())
        .unwrap_or(true)
}

/// Pure: why the archive can't be re-pointed right now, if it can't. Re-pointing (changing
/// where Silo looks, moving nothing) is for a custom archive that isn't there, or an archive
/// that holds no mods (nothing can be orphaned — e.g. the way back after starting fresh with
/// the default). An archive holding mods is moved instead, and never while a move is unfinished.
fn repoint_refusal(
    layout: &organize::Layout,
    unfinished_move: bool,
    holds_mods: bool,
) -> Option<String> {
    if unfinished_move {
        return Some("Finish or cancel the archive move first.".into());
    }
    if !layout.archive_missing() && holds_mods {
        return Some("Your library archive holds mods — use Change… to move them instead.".into());
    }
    None
}

/// Point Silo at another archive folder (`None` = the default `<mods>/archive`) WITHOUT moving
/// anything — for a custom archive that has gone missing (the user moved or restored it
/// elsewhere, or starts fresh with the default; whatever is on the old drive stays there,
/// untouched), or an archive that holds no mods (e.g. back to a restored archive after
/// starting fresh). The same path/overlap fences as a move apply, but mods already filed in the
/// folder are welcome — adopting them is the point. Republishes the layout; the frontend then
/// rescans so the library reconciles against the new archive.
#[tauri::command]
async fn archive_repoint(app: tauri::AppHandle, path: Option<String>) -> Result<(), String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    let app_data = app.path().app_data_dir().ok();
    tauri::async_runtime::spawn_blocking(move || {
        let _write = library_write()?;
        let (layout, key) = active_layout()?;
        let conn = db::open(&db)?;
        let unfinished = relocate::load_marker(&conn, &key).is_some();
        if let Some(why) = repoint_refusal(&layout, unfinished, archive_holds_mods(&layout)) {
            return Err(why);
        }
        let new = match &path {
            Some(p) => {
                let new = modsroot::real_folder(std::path::Path::new(p))?;
                if !picked::allows(picked::Purpose::Archive, &new) {
                    return Err(picked::NOT_PICKED.into());
                }
                new
            }
            None => organize::Layout::default_for(&modsroot::real_folder(&layout.mods)?).archive,
        };
        let problems = fence_problems(&layout, &new, &fences(&conn, &layout, app_data.as_deref()));
        if !problems.is_empty() {
            return Err(problems.join("\n"));
        }
        // A chosen folder must be usable now; the default is created when first filed into.
        if path.is_some() {
            modsroot::validate_folder(&new)?;
        }
        relocate::store_archive_setting(&conn, &key, &layout.mods, &new)?;
        if path.is_some() {
            picked::forget(picked::Purpose::Archive);
        }
        log::info!(
            "re-pointed the library archive {} -> {} (nothing moved)",
            layout.archive.display(),
            new.display()
        );
        let _ = compute_mods_root_status(&conn);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct AdoptPreview {
    plan: Vec<organize::PlannedMove>,
    inputs: Vec<organize::ModInput>,
    /// File names left where they are: their techName is already in the library, or repeats
    /// one earlier in this batch.
    duplicates: Vec<String>,
}

/// Pure: split scanned loose mods into adoptable ones and duplicates (file names) — a techName
/// already in the library, or repeated within the batch (the first by file name wins).
fn split_adopt(
    mut found: Vec<organize::ModInput>,
    known: &HashSet<String>,
) -> (Vec<organize::ModInput>, Vec<String>) {
    found.sort_by(|a, b| a.file_name.cmp(&b.file_name));
    let mut seen: HashSet<String> = HashSet::new();
    let mut inputs = Vec::new();
    let mut duplicates = Vec::new();
    for m in found {
        if known.contains(&m.tech_name) || !seen.insert(m.tech_name.clone()) {
            duplicates.push(m.file_name);
        } else {
            inputs.push(m);
        }
    }
    (inputs, duplicates)
}

/// Pure: the candidates the user picked (by file name), and the picked names that aren't
/// candidates any more.
fn select_adopt(
    candidates: Vec<organize::ModInput>,
    file_names: &[String],
) -> (Vec<organize::ModInput>, Vec<String>) {
    let want: HashSet<&str> = file_names.iter().map(String::as_str).collect();
    let have: HashSet<String> = candidates.iter().map(|c| c.file_name.clone()).collect();
    let missing = file_names
        .iter()
        .filter(|n| !have.contains(n.as_str()))
        .cloned()
        .collect();
    let picked = candidates
        .into_iter()
        .filter(|c| want.contains(c.file_name.as_str()))
        .collect();
    (picked, missing)
}

/// The loose mods at the archive's top level that could be filed into it, read from disk by
/// the server (techName, kind and category never come from the webview), plus duplicates.
fn adopt_candidates(
    conn: &rusqlite::Connection,
    layout: &organize::Layout,
    key: &str,
) -> (Vec<organize::ModInput>, Vec<String>) {
    let known: HashSet<String> = db::load_organized(conn, key)
        .into_iter()
        .map(|r| r.tech_name)
        .collect();
    // Scan the archive's loose top level as if it were a mods folder (no archives).
    let found = scan::scan_layouts(
        vec![(layout.archive.clone(), vec![])],
        &Default::default(),
        |_, _| {},
    );
    let mods = found
        .result
        .mods
        .into_iter()
        .filter_map(|m| {
            let file_name = std::path::Path::new(&m.path)
                .file_name()?
                .to_string_lossy()
                .into_owned();
            Some(organize::ModInput {
                tech_name: m.tech_name,
                file_name,
                kind: m.kind,
                category: m.category,
                subcategory: m.subcategory,
            })
        })
        .collect();
    split_adopt(mods, &known)
}

/// Loose mods at the archive's top level (e.g. a folder that already held mods) that could be
/// filed into it. Read-only.
#[tauri::command]
async fn archive_adopt_preview(app: tauri::AppHandle) -> Result<AdoptPreview, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let (layout, key) = active_layout()?;
        let conn = db::open(&db)?;
        let (inputs, duplicates) = adopt_candidates(&conn, &layout, &key);
        Ok(AdoptPreview {
            plan: organize::plan_organize(&layout.archive, &inputs),
            inputs,
            duplicates,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// File the loose mods the user picked (by file name, from the preview) into the archive's
/// category folders, never overwriting. The candidates are re-read from disk here.
#[tauri::command]
async fn archive_adopt_apply(
    app: tauri::AppHandle,
    file_names: Vec<String>,
) -> Result<organize::Report, String> {
    ensure_no_move()?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _write = library_write()?;
        ensure_archive_available()?;
        let (layout, key) = active_layout()?;
        let conn = db::open(&db)?;
        let (candidates, _) = adopt_candidates(&conn, &layout, &key);
        let (picked, missing) = select_adopt(candidates, &file_names);
        let mut report = organize::apply_organize(&conn, &layout, &layout.archive, &picked);
        for n in missing {
            report.skipped += 1;
            report.errors.push(format!(
                "{n}: no longer a loose mod Silo can file in — left untouched"
            ));
        }
        log::info!(
            "adopted {} loose mod(s) into the library archive ({} skipped, {} errors)",
            report.changed,
            report.skipped,
            report.errors.len()
        );
        Ok(report)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Loadouts (named active-mod sets) ──
#[tauri::command]
async fn get_loadouts(app: tauri::AppHandle) -> Result<Vec<db::Loadout>, String> {
    let db = db_path(&app)?;
    off_main(move || Ok(db::load_loadouts(&db::open(&db)?))).await
}

#[tauri::command]
async fn save_loadout(
    app: tauri::AppHandle,
    id: Option<i64>,
    name: String,
    mods: Vec<String>,
) -> Result<i64, String> {
    let db = db_path(&app)?;
    off_main(move || {
        let mut conn = db::open(&db)?;
        db::save_loadout(&mut conn, id, &name, &mods)
    })
    .await
}

#[tauri::command]
async fn delete_loadout(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let mut conn = db::open(&db)?;
        db::delete_loadout(&mut conn, id)
    })
    .await
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LoadoutFile {
    silo: u32,
    name: String,
    mods: Vec<String>,
}

#[tauri::command]
async fn export_loadout(app: tauri::AppHandle, id: i64, path: String) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        let lo = db::load_loadouts(&conn)
            .into_iter()
            .find(|l| l.id == id)
            .ok_or_else(|| "Loadout not found".to_string())?;
        let file = LoadoutFile {
            silo: 1,
            name: lo.name,
            mods: lo.mods,
        };
        let json = serde_json::to_string_pretty(&file).map_err(|e| e.to_string())?;
        paths::safe_outbound(std::path::Path::new(&path), &["silo"])?;
        std::fs::write(&path, json).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn import_loadout(app: tauri::AppHandle, path: String) -> Result<i64, String> {
    let db = db_path(&app)?;
    off_main(move || {
        paths::safe_inbound(std::path::Path::new(&path), &["silo", "json"])?;
        let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let file: LoadoutFile = serde_json::from_str(&content).map_err(|e| e.to_string())?;
        let mut conn = db::open(&db)?;
        db::save_loadout(&mut conn, None, &file.name, &file.mods)
    })
    .await
}

// ── Conflict detection (over the active set) ──
#[tauri::command]
async fn detect_conflicts(
    mods: Vec<conflicts::ConflictInput>,
) -> Result<Vec<conflicts::Conflict>, String> {
    tauri::async_runtime::spawn_blocking(move || conflicts::detect(&mods))
        .await
        .map_err(|e| e.to_string())
}

// ── Game launch ──
// Off the main thread: detection walks Steam libraries and, on macOS, may shell out to
// Spotlight (`mdfind`); launch re-detects to pick Steam vs. the `.app` bundle.
#[tauri::command]
async fn detect_game() -> Result<Option<gamelaunch::GameInfo>, String> {
    tauri::async_runtime::spawn_blocking(gamelaunch::detect)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
async fn launch_game() -> Result<(), String> {
    if relocate::is_running() {
        return Err("Silo is moving your library archive — pause the move to play.".into());
    }
    tauri::async_runtime::spawn_blocking(gamelaunch::launch)
        .await
        .map_err(|e| e.to_string())?
}

/// Write text to a user-chosen path (used by the diagnostics report export). Confined to a
/// report file type with no traversal, so it can't be turned into an arbitrary-file write.
#[tauri::command]
async fn save_text(path: String, content: String) -> Result<(), String> {
    off_main(move || {
        paths::safe_outbound(std::path::Path::new(&path), &["md", "txt"])?;
        std::fs::write(&path, content).map_err(|e| e.to_string())
    })
    .await
}

/// The diagnostics report (built by the frontend) plus, if asked, the recent log, with the
/// home folder and user name removed from both.
#[tauri::command]
async fn diagnostics_bundle(
    app: tauri::AppHandle,
    report: String,
    include_log: bool,
) -> Result<String, String> {
    let log_dir = app.path().app_log_dir().ok();
    let version = app.package_info().version.to_string();
    off_main(move || {
        let header = format!("Silo {version} on {}", std::env::consts::OS);
        let parts = include_log.then(|| {
            let files = log_dir
                .as_deref()
                .map(diagnostics::log_files)
                .unwrap_or_default();
            diagnostics::LogParts {
                warnings: diagnostics::recent_warnings(&files, 50, 16 * 1024),
                recent: diagnostics::recent_log(&files, 300, 64 * 1024),
            }
        });
        let home = dirs::home_dir().map(|h| h.to_string_lossy().into_owned());
        let user = std::env::var("USERNAME")
            .or_else(|_| std::env::var("USER"))
            .ok();
        Ok(diagnostics::bundle(
            &header,
            &report,
            parts.as_ref(),
            home.as_deref(),
            user.as_deref(),
        ))
    })
    .await
}

/// Put `text` on the system clipboard. From Rust because WebKit (macOS, Linux) refuses the
/// webview's clipboard after an awaited call; the webview itself has no clipboard permission.
#[tauri::command]
async fn copy_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    off_main(move || app.clipboard().write_text(text).map_err(|e| e.to_string())).await
}

/// Clear the scan cache so the next scan re-parses & re-categorizes every mod.
#[tauri::command]
async fn clear_scan_cache(app: tauri::AppHandle) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        db::clear_cache(&db::open(&db)?);
        Ok(())
    })
    .await
}

/// The FS25 user data dir (parent of mods/, savegameN/, game.xml).
#[tauri::command]
async fn user_dir_path() -> Result<Option<String>, String> {
    off_main(|| Ok(fsgame::user_dir().map(|p| p.to_string_lossy().into_owned()))).await
}

/// Generate a filltype-compatibility bridge companion mod into the library root.
/// Returns the created zip filename; the frontend rescans to pick it up.
#[tauri::command]
async fn generate_bridge(spec: bridge::BridgeSpec, root: Option<String>) -> Result<String, String> {
    ensure_no_move()?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let _write = library_write()?;
        let filename = format!("{}.zip", spec.tech_name);
        let dest = root.join(&filename);
        if dest.exists() {
            return Err(format!("{filename} already exists in your mods folder"));
        }
        bridge::generate(&spec, &dest)?;
        fslog::changed("generate", &dest);
        Ok(filename)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Multiplayer mod-set sync ──

/// Hash the host's active set and write a shareable manifest to `path`. Returns the
/// number of mods written.
#[tauri::command]
async fn mp_export(mods: Vec<mpsync::ModRef>, path: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<usize, String> {
        let manifest = mpsync::build_manifest(&mods);
        let json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        paths::safe_outbound(std::path::Path::new(&path), &["silomp", "json"])?;
        std::fs::write(&path, json).map_err(|e| e.to_string())?;
        Ok(manifest.mods.len())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read a shared manifest from `path`, hash the joiner's local set, and diff.
#[tauri::command]
async fn mp_verify_file(
    path: String,
    local: Vec<mpsync::ModRef>,
) -> Result<mpsync::VerifyReport, String> {
    tauri::async_runtime::spawn_blocking(move || -> Result<mpsync::VerifyReport, String> {
        paths::safe_inbound(std::path::Path::new(&path), &["silomp", "json"])?;
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let manifest: mpsync::Manifest = serde_json::from_str(&text)
            .map_err(|_| "That file isn't a valid Silo mod-set manifest".to_string())?;
        let local_hashed = mpsync::hash_mods(&local);
        Ok(mpsync::diff(&manifest.mods, &local_hashed))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Collections (share a mod set) ──

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResult {
    /// The shareable gist URL the user copies.
    url: String,
    /// How many mods went into the collection.
    count: usize,
    /// Dev/unpacked (directory) mods left out — they have no stable bytes to pin.
    omitted: Vec<String>,
}

/// Pin an active set into collection entries: drop dev/unpacked (directory) mods (no stable
/// bytes to pin), and for each packaged mod stamp its version, catalog source/id (one batch
/// lookup, best-effort), and the curator's own provenance hash. Returns the entries sorted by
/// tech-name plus the omitted dir-mod tech-names. Shared by export and update.
fn build_collection_entries(
    base: &str,
    mods: Vec<mpsync::ModRef>,
) -> (Vec<collection::CollectionMod>, Vec<String>) {
    let (zips, dirs): (Vec<_>, Vec<_>) = mods.into_iter().partition(|m| m.kind == "zip");
    let omitted: Vec<String> = dirs.into_iter().map(|m| m.tech_name).collect();

    let names: Vec<String> = zips.iter().map(|m| m.tech_name.clone()).collect();
    let hits = siloapi::lookup(base, &names).unwrap_or_default();
    let by_tech: std::collections::HashMap<&str, &siloapi::LookupResult> = hits
        .iter()
        .filter_map(|r| r.tech_name.as_deref().map(|t| (t, r)))
        .collect();

    let mut entries: Vec<collection::CollectionMod> = zips
        .iter()
        .map(|m| {
            // The curator's own build hash — the trust anchor. Best-effort per mod.
            let manifest_hash = provenance::manifest_from_zip(std::path::Path::new(&m.path))
                .ok()
                .map(|lm| lm.manifest_hash);
            let hit = by_tech.get(m.tech_name.as_str());
            let dl = hit.and_then(|r| r.download.as_ref());
            collection::CollectionMod {
                tech_name: m.tech_name.clone(),
                version: m.version.clone(),
                source: dl.map(|d| d.source.clone()),
                source_url: None,
                manifest_hash,
                installable: hit.map(|_| dl.is_some()),
                catalog_id: hit.map(|r| r.id.clone()),
            }
        })
        .collect();
    entries.sort_by(|a, b| a.tech_name.cmp(&b.tech_name));
    (entries, omitted)
}

/// Export a mod set as a shareable Collection: pin each mod's identity (version +
/// canonical provenance hash of the curator's own build), enrich with a catalog source so
/// an importer can fetch it, and publish it to the user's GitHub — a secret gist (private,
/// `gist` scope) or, when `public`, a public repo with a generated README (`public_repo`
/// scope, forkable/discoverable — the P2 transport).
///
/// The trust field is the LOCAL provenance `manifestHash` — content-addressed, so an
/// importer can verify they got the exact build the curator shared even from a re-zipped
/// source.
#[tauri::command]
async fn collection_export(
    app: tauri::AppHandle,
    name: String,
    description: Option<String>,
    created_at: Option<String>,
    public: Option<bool>,
    mods: Vec<mpsync::ModRef>,
) -> Result<ExportResult, String> {
    let base = siloapi_base(&app)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<ExportResult, String> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("Give the collection a name".into());
        }
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        let author = db::get_app_setting(&conn, "gh_user");

        // Pin the packaged mods (dev/unpacked ones can't be byte-pinned — reported as omitted).
        let (entries, omitted) = build_collection_entries(&base, mods);
        if entries.is_empty() {
            return Err(
                "Nothing to share — a collection needs at least one packaged (.zip) mod".into(),
            );
        }

        let coll = collection::Collection {
            schema: collection::SCHEMA.to_string(),
            name: name.clone(),
            description: description.filter(|s| !s.trim().is_empty()),
            author,
            created_at,
            savegame: None,
            mods: entries,
        };
        let json = collection::to_json(&coll)?;
        // The shareable link is always the silo.hllmr.com/c/ handoff page — never the raw
        // gist/repo URL (which dead-ends recipients on JSON and can't carry a working
        // "Open in Silo" link, since GitHub strips silo:// schemes).
        let url = if public.unwrap_or(false) {
            // P2: a public, forkable repo holding the collection + a human-readable README.
            let repo = github::create_public_repo(
                &token,
                &collection::repo_slug(&name),
                &format!("A Silo FS25 mod collection ({} mods)", coll.mods.len()),
            )?;
            let (owner, rname) = repo
                .full_name
                .split_once('/')
                .map(|(o, r)| (o.to_string(), r.to_string()))
                .unwrap_or_default();
            let page_url = collection::repo_page_url(&owner, &rname);
            let readme = collection::readme(&coll, &page_url, &repo.html_url);
            github::put_repo_file(
                &token,
                &owner,
                &rname,
                collection::FILE_NAME,
                &json,
                "Add collection (via Silo)",
            )?;
            github::put_repo_file(
                &token,
                &owner,
                &rname,
                "README.md",
                &readme,
                "Add README (via Silo)",
            )?;
            page_url
        } else {
            // P1: a secret (unlisted) gist for private / group sharing. Create with the JSON,
            // then attach a README linking the handoff page (its URL needs the new gist id).
            let gist = github::create_secret_gist(
                &token,
                &format!("{name} — a Silo mod collection"),
                collection::FILE_NAME,
                &json,
            )?;
            let page_url = collection::gist_page_url(&gist.id);
            let readme = collection::readme(&coll, &page_url, &gist.html_url);
            // Best-effort: the JSON + share link already work without it.
            let _ = github::update_gist_file(&token, &gist.id, "README.md", &readme);
            page_url
        };
        Ok(ExportResult {
            url,
            count: coll.mods.len(),
            omitted,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Update an existing collection in place: re-pin `mods` (the current active set) and write
/// the new list back to the *same* gist/repo, keeping its name/description/author/date — so
/// the share link never changes. Returns the (unchanged) page URL + new count.
#[tauri::command]
async fn collection_update(
    app: tauri::AppHandle,
    reference: String,
    mods: Vec<mpsync::ModRef>,
) -> Result<ExportResult, String> {
    let base = siloapi_base(&app)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<ExportResult, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        let where_ = collection::parse_collection_ref(&reference)
            .ok_or_else(|| "That collection reference isn't valid".to_string())?;

        // Preserve the collection's identity (name/description/author/date); re-pin the mods.
        let existing = read_collection_json(&reference, Some(&token))?;
        let (entries, omitted) = build_collection_entries(&base, mods);
        if entries.is_empty() {
            return Err(
                "Nothing to update with — the active set has no packaged (.zip) mods".into(),
            );
        }
        let coll = collection::Collection {
            schema: collection::SCHEMA.to_string(),
            name: existing.name,
            description: existing.description,
            author: existing.author,
            created_at: existing.created_at,
            savegame: existing.savegame,
            mods: entries,
        };
        let json = collection::to_json(&coll)?;

        let url = match where_ {
            collection::CollectionRef::Gist(id) => {
                let page_url = collection::gist_page_url(&id);
                let source_url = format!("https://gist.github.com/{id}");
                let readme = collection::readme(&coll, &page_url, &source_url);
                github::update_gist_file(&token, &id, collection::FILE_NAME, &json)?;
                let _ = github::update_gist_file(&token, &id, "README.md", &readme);
                page_url
            }
            collection::CollectionRef::Repo { owner, repo } => {
                let page_url = collection::repo_page_url(&owner, &repo);
                let source_url = format!("https://github.com/{owner}/{repo}");
                let readme = collection::readme(&coll, &page_url, &source_url);
                github::put_repo_file(
                    &token,
                    &owner,
                    &repo,
                    collection::FILE_NAME,
                    &json,
                    "Update collection (via Silo)",
                )?;
                github::put_repo_file(
                    &token,
                    &owner,
                    &repo,
                    "README.md",
                    &readme,
                    "Update README (via Silo)",
                )?;
                page_url
            }
        };
        Ok(ExportResult {
            url,
            count: coll.mods.len(),
            omitted,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Resolve a pasted collection link — a gist (P1) or a public repo (P2) — and read +
/// parse its `silo-collection.json`. Token optional (public sources read anonymously).
fn read_collection_json(
    url_or_id: &str,
    token: Option<&str>,
) -> Result<collection::Collection, String> {
    read_collection_raw(url_or_id, token).map(|(c, _)| c)
}

/// `read_collection_json`, plus the file's exact text (for `collection::fingerprint`, so apply
/// can refuse a collection that changed after its preview).
fn read_collection_raw(
    url_or_id: &str,
    token: Option<&str>,
) -> Result<(collection::Collection, String), String> {
    let reference = collection::parse_collection_ref(url_or_id)
        .ok_or_else(|| "That doesn't look like a GitHub gist or repo link".to_string())?;
    let json = match &reference {
        collection::CollectionRef::Gist(id) => {
            github::read_gist_file(id, collection::FILE_NAME, token)
        }
        collection::CollectionRef::Repo { owner, repo } => {
            github::read_repo_file(owner, repo, collection::FILE_NAME, token)
        }
    }?;
    Ok((collection::parse(&json)?, json))
}

/// Resolve every mod of a collection by techName with one catalog lookup, and fingerprint the
/// file together with that result. Preview and apply both go through here, so apply installs
/// exactly what the preview showed or refuses (`COLLECTION_CHANGED`).
fn resolve_collection(
    base: &str,
    coll: &collection::Collection,
    json: &str,
) -> (Vec<collection::Target>, String) {
    let names: Vec<String> = coll.mods.iter().map(|m| m.tech_name.clone()).collect();
    let hits = siloapi::lookup(base, &names).unwrap_or_default();
    let targets = collection::resolve_targets(&coll.mods, &catalog_matches(&hits));
    let fingerprint = collection::fingerprint(json, &targets);
    (targets, fingerprint)
}

/// The catalog lookup keyed by techName, for `collection::resolve_targets`.
fn catalog_matches(
    hits: &[siloapi::LookupResult],
) -> std::collections::HashMap<String, collection::CatalogMatch> {
    hits.iter()
        .filter_map(|r| {
            Some((
                r.tech_name.clone()?,
                collection::CatalogMatch {
                    id: r.id.clone(),
                    download_source: r.download.as_ref().map(|d| d.source.clone()),
                },
            ))
        })
        .collect()
}

/// A collection the user has published, for the "Your collections" management list.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CollectionSummary {
    /// `"gist"` | `"repo"`.
    kind: String,
    /// Gist id, or `"owner/repo"` — what `collection_delete` takes.
    reference: String,
    name: String,
    mod_count: usize,
    created_at: Option<String>,
    /// The silo.hllmr.com/c/ share link.
    page_url: String,
    /// The raw gist/repo URL on GitHub.
    source_url: String,
    /// Whether Silo can delete it in-app (gists: yes; repos need a scope we don't hold).
    can_delete: bool,
}

/// List the collections the user has published to their GitHub — secret gists (carrying the
/// collection file) and `silo-`-prefixed public repos. Best-effort per item: one that fails
/// to read or parse is skipped rather than failing the whole list.
#[tauri::command]
async fn collections_list(app: tauri::AppHandle) -> Result<Vec<CollectionSummary>, String> {
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<CollectionSummary>, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        let mut out: Vec<CollectionSummary> = Vec::new();

        if let Ok(gists) = github::list_user_gists(&token) {
            for g in gists {
                if !g.filenames.iter().any(|f| f == collection::FILE_NAME) {
                    continue;
                }
                if let Ok(coll) = read_collection_json(&g.id, Some(&token)) {
                    out.push(CollectionSummary {
                        kind: "gist".into(),
                        reference: g.id.clone(),
                        name: coll.name,
                        mod_count: coll.mods.len(),
                        created_at: g.created_at,
                        page_url: collection::gist_page_url(&g.id),
                        source_url: g.html_url,
                        can_delete: true,
                    });
                }
            }
        }

        if let Ok(repos) = github::list_owned_repos(&token) {
            for r in repos {
                if !r.name.starts_with("silo-") {
                    continue;
                }
                if let Ok(coll) = read_collection_json(&r.full_name, Some(&token)) {
                    let (owner, rname) =
                        r.full_name.split_once('/').unwrap_or(("", r.name.as_str()));
                    out.push(CollectionSummary {
                        kind: "repo".into(),
                        reference: r.full_name.clone(),
                        name: coll.name,
                        mod_count: coll.mods.len(),
                        created_at: r.created_at,
                        page_url: collection::repo_page_url(owner, rname),
                        source_url: r.html_url,
                        can_delete: false,
                    });
                }
            }
        }

        // Newest first (gists and repos interleaved by creation time).
        out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Delete a published collection. Gists (the private-share default) delete in-app with the
/// `gist` scope; public repos need `delete_repo` (not requested) so those are removed on GitHub.
#[tauri::command]
async fn collection_delete(
    app: tauri::AppHandle,
    kind: String,
    reference: String,
) -> Result<(), String> {
    // Validate the reference before any request: a gist id must be a real gist id, and a
    // repo reference a real `owner/repo` (repos are refused below, but never on bad input).
    match kind.as_str() {
        "gist" => github::check_gist_id(&reference)?,
        _ => {
            let (owner, repo) = reference
                .split_once('/')
                .unwrap_or((reference.as_str(), ""));
            github::check_repo(owner, repo)?;
        }
    }
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token")
            .ok_or_else(|| "Connect your GitHub account first".to_string())?;
        match kind.as_str() {
            "gist" => github::delete_gist(&token, &reference),
            _ => Err(
                "Silo can delete gist collections here. For a public repo, delete it on GitHub."
                    .into(),
            ),
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

/// One mod in an import plan, tagged with what the importer would do about it.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PlanRow {
    tech_name: String,
    /// Version pinned by the collection.
    version: Option<String>,
    /// The source the importer would use / send the user to.
    source: Option<String>,
    /// For `version_drift`: the version already in the library.
    installed_version: Option<String>,
}

/// A read-only preview of importing a shared collection: what you already have, what's a
/// different version, what Silo can install for you (a directly-downloadable source), and
/// what you'll need to fetch yourself (ModHub gates downloads) or that isn't catalogued.
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportPlan {
    name: String,
    description: Option<String>,
    author: Option<String>,
    will_install: Vec<PlanRow>,
    open_page: Vec<PlanRow>,
    already_present: Vec<PlanRow>,
    version_drift: Vec<PlanRow>,
    unresolved: Vec<PlanRow>,
    /// Hash of the collection file and its catalog matches as previewed (`resolve_collection`);
    /// `collection_apply` refuses if either changed.
    fingerprint: String,
}

/// Fetch a shared collection by gist link/id, parse it, and bucket every mod against the
/// caller's installed set + the catalog — WITHOUT touching a single file. One batch catalog
/// lookup covers the whole list. The apply step (which does install) is a separate command.
#[tauri::command]
async fn collection_import_preview(
    app: tauri::AppHandle,
    url_or_id: String,
    installed: Vec<LocalMod>,
) -> Result<ImportPlan, String> {
    let base = siloapi_base(&app)?;
    let db = db_path(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<ImportPlan, String> {
        let conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token");
        let (coll, json) = read_collection_raw(&url_or_id, token.as_deref())?;

        let installed_by: std::collections::HashMap<&str, Option<&str>> = installed
            .iter()
            .map(|m| (m.tech_name.as_str(), m.version.as_deref()))
            .collect();

        // One lookup resolves catalog presence + best installable source for every mod, by
        // techName, exactly as apply resolves them.
        let (targets, fingerprint) = resolve_collection(&base, &coll, &json);

        let mut plan = ImportPlan {
            name: coll.name.clone(),
            description: coll.description.clone(),
            author: coll.author.clone(),
            will_install: Vec::new(),
            open_page: Vec::new(),
            already_present: Vec::new(),
            version_drift: Vec::new(),
            unresolved: Vec::new(),
            fingerprint,
        };

        for (m, t) in coll.mods.iter().zip(&targets) {
            let row = |installed_version: Option<String>| PlanRow {
                tech_name: m.tech_name.clone(),
                version: m.version.clone(),
                source: t.display_source.clone(),
                installed_version,
            };

            match installed_by.get(m.tech_name.as_str()) {
                Some(have) => {
                    let have = have.map(|s| s.to_string());
                    // A pinned version that differs from what's installed is a drift row;
                    // no pin (or an equal pin) counts as satisfied.
                    let drift = matches!((&m.version, &have), (Some(w), Some(h)) if w != h);
                    if drift {
                        plan.version_drift.push(row(have));
                    } else {
                        plan.already_present.push(row(have));
                    }
                }
                None => {
                    if t.download_source.is_some() {
                        plan.will_install.push(row(None));
                    } else if t.catalog_id.is_some() {
                        // In the catalog, but no direct download (ModHub gates it).
                        plan.open_page.push(row(None));
                    } else {
                        plan.unresolved.push(row(None));
                    }
                }
            }
        }
        Ok(plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The outcome for one mod in an apply run.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyRow {
    tech_name: String,
    /// The installed filename, when Silo downloaded it this run.
    filename: Option<String>,
    /// "installed" (downloaded now) · "present" (already had it) · "skipped" (get it
    /// yourself / not catalogued) · "failed".
    status: String,
    /// Provenance verdict for what landed: "verified" / "modified" / "unverified".
    verdict: Option<String>,
    /// A failure or open-page reason.
    detail: Option<String>,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyReport {
    /// The loadout created from the collection's full mod list; `None` when the library
    /// stayed busy and the finished downloads couldn't be added (those rows say so).
    loadout_id: Option<i64>,
    installed: usize,
    failed: usize,
    rows: Vec<ApplyRow>,
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ApplyProgress {
    done: usize,
    total: usize,
    current: String,
}

/// Import a shared collection: download every directly-installable mod that's missing,
/// verify each against the version the collection pinned, and save the whole set as a
/// loadout. Mods that must be fetched from ModHub (or aren't catalogued) are recorded
/// but never faked — the loadout still lists them so it completes once the user grabs them.
///
/// Every download reuses the same guarded path as a Browse install (basename check, stream
/// to `.part`, archive + identity validation, skip-if-present) — no new file-write surface.
///
/// Two phases: every download runs first, without the library lock, into its own `.part`;
/// then the lock is taken only to place the parts (re-checking each destination is still
/// free) and save the loadout. Any part left over on an error is removed.
#[tauri::command]
async fn collection_apply(
    app: tauri::AppHandle,
    url_or_id: String,
    installed: Vec<LocalMod>,
    fingerprint: String,
    root: Option<String>,
) -> Result<ApplyReport, String> {
    ensure_no_move()?;
    let base = siloapi_base(&app)?;
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<ApplyReport, String> {
        let mut conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token");
        let (coll, json) = read_collection_raw(&url_or_id, token.as_deref())?;
        // Resolve every mod by techName, exactly as the preview did. A collection's own
        // `catalog_id` and source hint never decide what downloads. The fingerprint covers the
        // file and this resolution, so a changed gist or catalog answer is refused.
        let (targets, fp) = resolve_collection(&base, &coll, &json);
        const COLLECTION_CHANGED: &str =
            "This collection changed since you previewed it. Preview it again before installing.";
        if fp != fingerprint {
            return Err(COLLECTION_CHANGED.into());
        }

        let have: std::collections::HashSet<&str> =
            installed.iter().map(|m| m.tech_name.as_str()).collect();

        let total = coll.mods.len();
        let mut rows: Vec<ApplyRow> = Vec::with_capacity(total);
        let mut installed_count = 0usize;
        let mut failed_count = 0usize;
        // Finished downloads waiting to be placed under the lock (their rows are filled then).
        let mut pending: Vec<PendingPlace> = Vec::new();

        for (i, (m, t)) in coll.mods.iter().zip(&targets).enumerate() {
            let _ = emitter.emit(
                "collection:progress",
                ApplyProgress {
                    done: i,
                    total,
                    current: m.tech_name.clone(),
                },
            );

            // Already in the library — don't re-download; still verify if we can.
            if have.contains(m.tech_name.as_str()) {
                rows.push(ApplyRow {
                    tech_name: m.tech_name.clone(),
                    filename: None,
                    status: "present".into(),
                    verdict: None,
                    detail: None,
                });
                continue;
            }

            let Some(cid) = t.catalog_id.as_deref() else {
                rows.push(ApplyRow {
                    tech_name: m.tech_name.clone(),
                    filename: None,
                    status: "skipped".into(),
                    verdict: None,
                    detail: Some("Not in the catalog — find it manually.".into()),
                });
                continue;
            };
            // Catalogued, but no direct download (ModHub gates it): the preview's open-page row.
            if t.download_source.is_none() {
                rows.push(gated_row(&m.tech_name));
                continue;
            }

            match siloapi::resolve_download(&base, cid, t.download_source.as_deref()) {
                Ok(resolved) => {
                    if let Err(e) = paths::safe_file_name(&resolved.filename) {
                        rows.push(fail_row(&m.tech_name, e));
                        failed_count += 1;
                        continue;
                    }
                    let dest = root.join(&resolved.filename);
                    // Already there, or another mod in this collection downloads the same file.
                    let key = same_file_key(&dest);
                    if path_taken(&dest) || pending.iter().any(|p| same_file_key(&p.dest) == key) {
                        rows.push(present_row(&m.tech_name, &resolved.filename));
                        continue;
                    }
                    // Taken only by another download running right now (e.g. from Browse).
                    let Ok(part) = claim_part(&dest) else {
                        rows.push(ApplyRow {
                            tech_name: m.tech_name.clone(),
                            filename: Some(resolved.filename.clone()),
                            status: "skipped".into(),
                            verdict: None,
                            detail: Some("Already downloading elsewhere in Silo.".into()),
                        });
                        continue;
                    };
                    match siloapi::download_to_part(
                        &resolved.url,
                        part.path(),
                        resolved.expected_sha256.as_deref(),
                        |_done, _total| {},
                    ) {
                        Ok(()) => {
                            // Same bytes as the placed file will have; hashed before the lock.
                            let verdict =
                                verdict_against_pin(part.path(), m.manifest_hash.as_deref());
                            pending.push(PendingPlace {
                                row: rows.len(),
                                part,
                                dest,
                                verdict,
                            });
                            rows.push(ApplyRow {
                                tech_name: m.tech_name.clone(),
                                filename: Some(resolved.filename.clone()),
                                status: "installed".into(),
                                verdict: None,
                                detail: None,
                            });
                        }
                        Err(e) => {
                            rows.push(fail_row(&m.tech_name, e));
                            failed_count += 1;
                        }
                    }
                }
                // No direct download (ModHub gates it) — leave it for the user.
                Err(_) => rows.push(gated_row(&m.tech_name)),
            }
        }

        // Phase 2, under the lock: place each download whose destination is still free, then
        // save the loadout. Dropping a `PendingPlace` (any early return) removes its part.
        // Finished downloads wait out a busy spell (`PLACE_WAIT`) rather than being dropped.
        let lock = library_write_within(PLACE_WAIT);
        if let Err(e) = &lock {
            if pending.is_empty() {
                return Err(e.clone()); // nothing downloaded, nothing lost
            }
        }
        let locked = lock.as_ref().map(|_| ()).map_err(String::clone);
        let (placed, place_failed) = place_pending(&mut rows, pending, locked);
        installed_count += placed;
        failed_count += place_failed;

        // The loadout lists the whole collection; applying it activates the ones you have,
        // and it completes automatically once you fetch the rest and rescan. Not saved when
        // the library stayed busy (the report says which downloads couldn't be added).
        let loadout_id = match lock {
            Ok(_write) => Some(db::save_loadout(
                &mut conn,
                None,
                &coll.name,
                &coll.tech_names(),
            )?),
            Err(_) => None,
        };

        let _ = emitter.emit(
            "collection:progress",
            ApplyProgress {
                done: total,
                total,
                current: String::new(),
            },
        );
        Ok(ApplyReport {
            loadout_id,
            installed: installed_count,
            failed: failed_count,
            rows,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// A collection download that finished and waits to be placed under the library lock.
struct PendingPlace {
    /// Index of its row in the report.
    row: usize,
    part: PartClaim,
    dest: PathBuf,
    verdict: String,
}

/// Fill in the rows of finished collection downloads: placed (`installed`, with its verdict),
/// `present` (the file appeared meanwhile; theirs is kept), or `failed`. `locked` is whether
/// the library lock is held; without it every pending row fails with that reason. Each part
/// not placed is removed as its claim drops. Returns (installed, failed).
fn place_pending(
    rows: &mut [ApplyRow],
    pending: Vec<PendingPlace>,
    locked: Result<(), String>,
) -> (usize, usize) {
    let (mut installed, mut failed) = (0, 0);
    for p in pending {
        let row = &mut rows[p.row];
        if let Err(e) = &locked {
            *row = fail_row(&row.tech_name, e.clone());
            failed += 1;
        } else if path_taken(&p.dest) {
            // Appeared while we downloaded (another install): keep theirs.
            row.status = "present".into();
        } else {
            match place_part(p.part.path(), &p.dest) {
                Ok(()) => {
                    installed += 1;
                    row.verdict = Some(p.verdict);
                }
                Err(e) => {
                    *row = fail_row(&row.tech_name, e);
                    failed += 1;
                }
            }
        }
    }
    (installed, failed)
}

fn present_row(tech_name: &str, filename: &str) -> ApplyRow {
    ApplyRow {
        tech_name: tech_name.to_string(),
        filename: Some(filename.to_string()),
        status: "present".into(),
        verdict: None,
        detail: None,
    }
}

fn fail_row(tech_name: &str, detail: String) -> ApplyRow {
    ApplyRow {
        tech_name: tech_name.to_string(),
        filename: None,
        status: "failed".into(),
        verdict: None,
        detail: Some(detail),
    }
}

/// A catalogued mod with no direct download (ModHub gates it), left for the user.
fn gated_row(tech_name: &str) -> ApplyRow {
    ApplyRow {
        tech_name: tech_name.to_string(),
        filename: None,
        status: "skipped".into(),
        verdict: None,
        detail: Some("Download-gated — get it from its page.".into()),
    }
}

/// Hash a freshly-installed zip and compare it to the hash the collection pinned:
/// "verified" (content match), "modified" (differs — not what the curator shared), or
/// "unverified" (the collection carried no hash to check against).
fn verdict_against_pin(zip: &std::path::Path, pinned: Option<&str>) -> String {
    let Some(pinned) = pinned else {
        return "unverified".into();
    };
    match provenance::manifest_from_zip(zip) {
        Ok(local) if local.manifest_hash.eq_ignore_ascii_case(pinned) => "verified".into(),
        Ok(_) => "modified".into(),
        Err(_) => "unverified".into(),
    }
}

/// Parse the FS25 log.txt and report which mods are throwing errors/warnings, whether
/// the last run crashed, and the culprit ranking. Read-only; touches nothing.
#[tauri::command]
async fn scan_log() -> Result<logscan::LogReport, String> {
    let path = fsgame::user_dir()
        .map(|d| d.join("log.txt"))
        .ok_or_else(|| "Could not locate the FS25 user directory".to_string())?;
    tauri::async_runtime::spawn_blocking(move || -> Result<logscan::LogReport, String> {
        if !path.exists() {
            return Err("No log.txt found — launch FS25 at least once first".to_string());
        }
        // Logs are read as lossy UTF-8; FS writes mostly ASCII but paths can carry stray bytes.
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        let text = String::from_utf8_lossy(&bytes);
        Ok(logscan::parse(&text))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Parse inputBinding.xml into a per-device binding map (read-only).
#[tauri::command]
async fn scan_bindings() -> Result<bindings::BindingReport, String> {
    let path = fsgame::user_dir()
        .map(|d| d.join("inputBinding.xml"))
        .ok_or_else(|| "Could not locate the FS25 user directory".to_string())?;
    tauri::async_runtime::spawn_blocking(move || -> Result<bindings::BindingReport, String> {
        if !path.exists() {
            return Err("No inputBinding.xml found — launch FS25 at least once first".to_string());
        }
        let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
        Ok(bindings::parse(&String::from_utf8_lossy(&bytes)))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Guided bisection ──

/// Decide the next split for the current suspect pool (pure; see bisect.rs).
#[tauri::command]
fn bisect_plan(pool: Vec<String>) -> bisect::BisectStep {
    bisect::plan(&pool)
}

/// Narrow the pool by the last round's verdict — kept in one tested place so the
/// test/rest halves can't get swapped by the caller.
#[tauri::command]
fn bisect_narrow(test: Vec<String>, rest: Vec<String>, still_broken: bool) -> Vec<String> {
    bisect::narrow(test, rest, still_broken)
}

/// Persist the user's real active set before bisection perturbs it, so even an app
/// crash mid-diagnosis can restore it (reversibility is non-negotiable — CLAUDE.md).
#[tauri::command]
async fn bisect_snapshot_save(app: tauri::AppHandle, active: Vec<String>) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        let json = serde_json::to_string(&active).map_err(|e| e.to_string())?;
        db::set_app_setting(&conn, "bisect_snapshot", Some(&json))
    })
    .await
}

/// The saved pre-bisection active set, if a bisection is (or was) in progress.
#[tauri::command]
async fn bisect_snapshot_get(app: tauri::AppHandle) -> Result<Option<Vec<String>>, String> {
    let db = db_path(&app)?;
    off_main(move || {
        let conn = db::open(&db)?;
        match db::get_app_setting(&conn, "bisect_snapshot") {
            Some(json) => Ok(Some(
                serde_json::from_str(&json).map_err(|e| e.to_string())?,
            )),
            None => Ok(None),
        }
    })
    .await
}

/// Clear the snapshot once the original set has been restored.
#[tauri::command]
async fn bisect_snapshot_clear(app: tauri::AppHandle) -> Result<(), String> {
    let db = db_path(&app)?;
    off_main(move || db::set_app_setting(&db::open(&db)?, "bisect_snapshot", None)).await
}

/// Read specific values from a config XML by path.
#[tauri::command]
async fn get_config(
    path: String,
    paths: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    off_main(move || {
        let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        let xml = crate::paths::guarded_xml_read(&[user], std::path::Path::new(&path))?;
        Ok(xmlconfig::get_values(&xml, &paths))
    })
    .await
}

/// Apply value edits to a config XML. The write is confined to the FS25 user dir, backs up
/// the original first (required), and swaps the new file in atomically (see
/// `paths::guarded_xml_write`).
#[tauri::command]
async fn set_config(path: String, edits: Vec<xmlconfig::Edit>) -> Result<(), String> {
    off_main(move || {
        config_write_refusal(gamelaunch::fs25_running())?;
        let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        let roots = [user];
        let p = std::path::Path::new(&path);
        let xml = paths::guarded_xml_read(&roots, p)?;
        let out = xmlconfig::set_values(&xml, &edits)?;
        ensure_override_kept(&roots[0], p, Some(&xml), &out)?;
        paths::guarded_xml_write(&roots, p, &out)
    })
    .await
}

// ── Mod settings form ──
#[tauri::command]
async fn mods_with_settings() -> Result<Vec<String>, String> {
    off_main(|| {
        Ok(fsgame::user_dir()
            .map(|d| settings_form::mods_with_settings(&d))
            .unwrap_or_default())
    })
    .await
}

#[tauri::command]
async fn get_mod_settings(mod_name: String) -> Result<Vec<settings_form::SettingsFile>, String> {
    off_main(move || {
        let dir = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        let mut files = Vec::new();
        for path in settings_form::find_files(&dir, &mod_name) {
            files.push(settings_form::load_file(&path)?);
        }
        Ok(files)
    })
    .await
}

#[tauri::command]
async fn save_mod_settings(path: String, edits: Vec<settings_form::Edit>) -> Result<(), String> {
    off_main(move || {
        config_write_refusal(gamelaunch::fs25_running())?;
        let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        let roots = [user];
        let p = std::path::Path::new(&path);
        let raw = paths::guarded_xml_read(&roots, p)?;
        let updated = settings_form::apply_edits(&raw, &edits)?;
        ensure_override_kept(&roots[0], p, Some(&raw), &updated)?;
        paths::guarded_xml_write(&roots, p, &updated)
    })
    .await
}

#[tauri::command]
async fn save_mod_settings_raw(path: String, content: String) -> Result<(), String> {
    off_main(move || {
        config_write_refusal(gamelaunch::fs25_running())?;
        let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        let p = std::path::Path::new(&path);
        ensure_override_kept(&user, p, None, &content)?;
        paths::guarded_xml_write(&[user], p, &content)
    })
    .await
}

// ── Savegames ──
#[tauri::command]
async fn get_savegames() -> Result<Vec<savegame::Savegame>, String> {
    off_main(|| {
        Ok(fsgame::user_dir()
            .map(|dir| savegame::list_savegames(&dir))
            .unwrap_or_default())
    })
    .await
}

#[tauri::command]
async fn backup_savegame(folder: String) -> Result<String, String> {
    off_main(move || {
        let dir = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
        savegame::backup(&dir, &folder)
    })
    .await
}

/// The app version from Cargo/tauri.conf package info — the single source of truth
/// the UI reads so a build always shows the number it was cut from.
#[tauri::command]
fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// Notices from startup work, each returned once (see `notices`).
#[tauri::command]
async fn take_startup_notices() -> Vec<String> {
    notices::take()
}

/// True when this machine can store account tokens in the OS keychain. When false, the UI
/// warns that a connected token would be kept in the local DB unencrypted.
#[tauri::command]
async fn secret_storage_secure() -> Result<bool, String> {
    off_main(|| Ok(secrets::keychain_available())).await
}

/// Files in the flat mods root that occupy an organized mod's name but aren't Silo's projection
/// (a build the user swapped in, a leftover, etc.). Silo never deletes them, but surfaces them
/// so the user knows their intended mod isn't what will load from that name.
// MUST be async + spawn_blocking: it hashes any flat file that isn't provably Silo's hardlink
// (copy-projections force a full content hash), which on a large library is gigabytes of I/O.
// A synchronous command runs on the main thread and would freeze the window ("Not Responding").
#[tauri::command]
async fn detect_foreign_files(
    app: tauri::AppHandle,
    root: Option<String>,
) -> Result<Vec<organize::ForeignFile>, String> {
    let db = db_path(&app)?;
    let layout = primary_layout(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<organize::ForeignFile>, String> {
        let conn = db::open(&db)?;
        Ok(organize::detect_foreign_projections(&conn, &layout))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The log: `app_log_dir()/silo.log` when `to_file` (Windows `%LOCALAPPDATA%\com.hllmr.silo\logs`,
/// macOS `~/Library/Logs/com.hllmr.silo`, Linux `~/.local/share/com.hllmr.silo/logs`), and
/// stderr. Release builds have no console, so the file is the only record of what Silo did. It
/// holds full paths; "Copy diagnostics" removes the home folder and user name (`diagnostics`).
/// Never log secrets (tokens, gist contents) through it.
fn log_plugin<R: tauri::Runtime>(to_file: bool) -> tauri::plugin::TauriPlugin<R> {
    use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};
    let mut targets = vec![Target::new(TargetKind::Stderr)];
    if to_file {
        targets.push(Target::new(TargetKind::LogDir {
            file_name: Some(diagnostics::LOG_NAME.into()),
        }));
    }
    tauri_plugin_log::Builder::new()
        .clear_targets()
        .targets(targets)
        .level(log::LevelFilter::Info)
        .level_for("tao", log::LevelFilter::Warn)
        .level_for("wry", log::LevelFilter::Warn)
        .level_for("ureq", log::LevelFilter::Warn)
        .level_for("rustls", log::LevelFilter::Warn)
        .max_file_size(2_000_000)
        // KeepSome(n) keeps n rotated files besides the live one: five files in all.
        .rotation_strategy(RotationStrategy::KeepSome(4))
        .timezone_strategy(TimezoneStrategy::UseLocal)
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // single-instance MUST be registered first. Its `deep-link` feature forwards a silo://
    // URL to the running instance (rather than spawning a second one); the callback fires on
    // that second launch, so we just surface the existing window — the deep-link plugin has
    // already re-emitted the URL to the frontend's onOpenUrl listener.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            use tauri::Manager;
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.set_focus();
                let _ = w.unminimize();
            }
        }));
    }

    builder
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        // Used by `copy_text` only; no clipboard permission is granted to the webview.
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Started here, not on the builder: a plugin that fails on the builder stops the app,
            // and a log folder Silo can't write (or an old log another program holds open) must
            // never keep Silo from starting. Then it logs to stderr only.
            if let Err(e) = app.handle().plugin(log_plugin(true)) {
                let _ = app.handle().plugin(log_plugin(false));
                log::warn!("couldn't open the log file: {e}");
            }
            log::info!(
                "Silo {} starting on {}",
                app.package_info().version,
                std::env::consts::OS
            );
            // Resolve and publish the active layout (mods folder, archive, any unfinished move)
            // before the UI asks, so the write guards never act on an unknown layout. Off the
            // main thread: it reads gameSettings.xml and the DB.
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                match db_path(&handle).and_then(|p| db::open(&p)) {
                    Ok(conn) => {
                        // A daily copy of the database to fall back on if it's ever damaged.
                        if let Ok(p) = db_path(&handle) {
                            match db::snapshot(
                                &conn,
                                &db::snapshot_dir(&p),
                                5,
                                std::time::Duration::from_secs(20 * 3600),
                            ) {
                                Ok(Some(s)) => {
                                    log::info!("database snapshot saved: {}", s.display())
                                }
                                Ok(None) => {}
                                Err(e) => log::warn!("database snapshot failed: {e}"),
                            }
                        }
                        let _ = compute_mods_root_status(&conn);
                        log::info!("library layout resolved at startup");
                    }
                    Err(e) => {
                        log::warn!("couldn't resolve the library layout at startup: {e}")
                    }
                }
                // Icon thumbnails from the old (unstable) cache naming; `icons-v2` replaced it.
                if let Ok(c) = handle.path().app_cache_dir() {
                    let _ = std::fs::remove_dir_all(c.join("icons"));
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            scan_mods,
            get_mod_icon,
            get_curation,
            set_curation,
            get_tags,
            set_tags,
            get_mod_repos,
            set_mod_repo,
            get_mod_sources,
            set_mod_source,
            mark_source_checked,
            guess_repo,
            check_mod_update,
            gh_status,
            gh_set_client_id,
            gh_device_start,
            gh_device_poll,
            gh_set_pat,
            gh_repo_stats,
            gh_star,
            gh_watch,
            gh_logout,
            download_update,
            siloapi_status,
            siloapi_set_base,
            browse_mods,
            siloapi_facets,
            siloapi_stats,
            siloapi_categories,
            siloapi_mod_detail,
            catalog_detail_by_tech,
            catalog_image,
            install_remote_mod,
            catalog_check_updates,
            verify_mod,
            get_overrides,
            set_override,
            plan_organize,
            apply_organize,
            set_active,
            flatten,
            mods_root_status,
            set_mods_root,
            clear_mods_root,
            point_game_at_mods_root,
            create_mods_folder,
            folder_looks_empty,
            pick_folder,
            open_known_folder,
            archive_status,
            archive_move_preflight,
            archive_move_start,
            archive_move_resume,
            archive_move_pause,
            archive_move_cancel,
            archive_use_default,
            archive_repoint,
            archive_adopt_preview,
            archive_adopt_apply,
            get_loadouts,
            save_loadout,
            delete_loadout,
            export_loadout,
            import_loadout,
            get_savegames,
            backup_savegame,
            detect_conflicts,
            detect_game,
            launch_game,
            save_text,
            diagnostics_bundle,
            copy_text,
            user_dir_path,
            clear_scan_cache,
            scan_log,
            scan_bindings,
            mp_export,
            mp_verify_file,
            collection_export,
            collection_update,
            collection_import_preview,
            collection_apply,
            collections_list,
            collection_delete,
            generate_bridge,
            bisect_plan,
            bisect_narrow,
            bisect_snapshot_save,
            bisect_snapshot_get,
            bisect_snapshot_clear,
            get_config,
            set_config,
            mods_with_settings,
            get_mod_settings,
            save_mod_settings,
            save_mod_settings_raw,
            app_version,
            take_startup_notices,
            secret_storage_secure,
            detect_foreign_files,
            adopt_foreign_file,
            restore_foreign_file,
            relink_projections,
            hardlink_support,
            app_busy,
            force_quit,
            set_self_updating
        ])
        // Closing the main window while Silo is changing files could leave a mod half-moved:
        // keep it open and let the frontend ask (it calls `force_quit` on "Quit anyway").
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if hold_quit() {
                    api.prevent_close();
                    let _ = window.emit("close-blocked", ());
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building Silo")
        // Quitting the app another way (the macOS Quit menu, Cmd+Q) is held the same way.
        .run(|app, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                if hold_quit() {
                    api.prevent_exit();
                    if app.get_webview_window("main").is_some() {
                        let _ = app.emit("close-blocked", ());
                    } else {
                        // No window left to ask in: quit as soon as the change is done.
                        exit_when_idle(app.clone());
                    }
                }
            }
        });
}

/// Set while a thread waits to quit once Silo stops changing files (one at a time).
static EXIT_PENDING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Quit once `library_busy()` clears, from a thread of its own.
fn exit_when_idle(app: tauri::AppHandle) {
    if EXIT_PENDING.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    std::thread::spawn(move || {
        while library_busy() {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        FORCE_QUIT.store(true, std::sync::atomic::Ordering::SeqCst);
        app.exit(0);
    });
}

/// Hold a quit: Silo is changing files and the user hasn't chosen "Quit anyway".
fn hold_quit() -> bool {
    !FORCE_QUIT.load(std::sync::atomic::Ordering::SeqCst) && library_busy()
}

#[cfg(test)]
mod tests {
    use super::{
        archive_holds_mods, build_gh_scope, cancel_falls_back, fence_problems,
        foreign_mods_problem, known_same_mount, move_outcome, move_problems, overlap_problems,
        repoint_refusal, scan_archives, scope_grants, select_adopt, split_adopt, Fences, MoveFacts,
        Space,
    };
    use crate::{
        organize::{Layout, ModInput},
        relocate::MoveMarker,
        siloapi,
    };
    use std::collections::HashSet;
    use std::path::{Path, PathBuf};

    fn fences() -> Fences {
        Fences {
            mods: PathBuf::from("/home/me/fs25/mods"),
            current_archive: PathBuf::from("/hdd/arch"),
            other_mods: vec![
                PathBuf::from("/other/mods"),
                PathBuf::from("/home/me/fs25/mods2"),
            ],
            other_archives: vec![PathBuf::from("/other/mods/archive"), PathBuf::from("/x/a2")],
            user_dir: Some(PathBuf::from("/home/me/fs25")),
            app_data: Some(PathBuf::from("/home/me/.silo")),
        }
    }

    fn has(v: &[String], needle: &str) -> bool {
        v.iter().any(|p| p.contains(needle))
    }

    #[test]
    fn the_mods_folder_changes_only_to_a_picked_known_or_default_folder() {
        let a = std::path::Path::new("/x/mods-a");
        let known = vec![PathBuf::from("/x/mods-known")];
        let d = std::path::Path::new("/x/default");
        let default = Some(d);
        assert!(super::mods_root_choice_allowed(a, true, &known, default));
        assert!(super::mods_root_choice_allowed(
            &known[0], false, &known, default
        ));
        assert!(super::mods_root_choice_allowed(d, false, &known, default));
        assert!(!super::mods_root_choice_allowed(a, false, &known, default));
    }

    #[test]
    fn overlap_allows_a_clear_folder() {
        assert!(overlap_problems(Path::new("/ssd/new-archive"), &fences()).is_empty());
        // The default archive inside the FS25 user dir is fine (only the dir itself, or a
        // folder containing it, is refused).
        assert!(overlap_problems(Path::new("/home/me/fs25/mods/archive"), &fences()).is_empty());
    }

    #[test]
    fn overlap_refuses_a_drive_root_alone() {
        assert_eq!(
            overlap_problems(Path::new("/"), &fences()),
            vec!["Pick a folder, not a whole drive.".to_string()]
        );
    }

    #[test]
    fn overlap_refuses_nesting_with_the_current_archive() {
        let f = fences();
        assert!(has(
            &overlap_problems(Path::new("/hdd/arch/sub"), &f),
            "current archive"
        ));
        assert!(has(
            &overlap_problems(Path::new("/hdd"), &f),
            "current archive"
        ));
        // Equality is path_problems' message, not repeated here.
        assert!(overlap_problems(Path::new("/hdd/arch"), &f).is_empty());
    }

    #[test]
    fn overlap_refuses_other_mods_folders_equal_inside_or_containing() {
        let f = fences();
        for p in ["/other/mods", "/other/mods/sub", "/other"] {
            assert!(
                has(
                    &overlap_problems(Path::new(p), &f),
                    "another mods folder Silo knows"
                ),
                "{p}"
            );
        }
    }

    #[test]
    fn overlap_refuses_nesting_with_other_archives() {
        let f = fences();
        assert!(has(
            &overlap_problems(Path::new("/x/a2/deeper"), &f),
            "archive"
        ));
        assert!(has(&overlap_problems(Path::new("/x"), &f), "archive"));
        // Equal: path_problems' "Another mods folder already uses that archive".
        assert!(!has(&overlap_problems(Path::new("/x/a2"), &f), "archive"));
    }

    #[test]
    fn overlap_refuses_the_user_dir_and_app_data() {
        let f = fences();
        assert!(has(
            &overlap_problems(Path::new("/home/me/fs25"), &f),
            "Farming Simulator 25"
        ));
        assert!(has(
            &overlap_problems(Path::new("/home/me"), &f),
            "Farming Simulator 25"
        ));
        assert!(has(
            &overlap_problems(Path::new("/home/me/.silo"), &f),
            "Silo's own data"
        ));
        assert!(has(
            &overlap_problems(Path::new("/home/me/.silo/a"), &f),
            "Silo's own data"
        ));
    }

    #[test]
    fn overlap_refuses_inside_the_user_dir_except_inside_a_mods_folder() {
        let f = fences();
        for p in ["/home/me/fs25/savegame1", "/home/me/fs25/a/b"] {
            assert!(
                has(&overlap_problems(Path::new(p), &f), "Farming Simulator 25"),
                "{p}"
            );
        }
        // Inside the active mods folder: the default archive is fine (path_problems judges
        // anything else there), and the user-dir message isn't added.
        assert!(overlap_problems(Path::new("/home/me/fs25/mods/archive"), &f).is_empty());
        assert!(!has(
            &overlap_problems(Path::new("/home/me/fs25/mods/other"), &f),
            "Farming Simulator 25"
        ));
        // Inside another known mods folder: refused for that, not repeated as the user dir.
        let p = overlap_problems(Path::new("/home/me/fs25/mods2/sub"), &f);
        assert!(has(&p, "another mods folder Silo knows"));
        assert!(!has(&p, "Farming Simulator 25"));
    }

    #[test]
    fn cancel_falls_back_only_when_a_move_holds_the_lock() {
        assert!(cancel_falls_back(super::MOVE_LOCK_HELD));
        // relocate's own guard refusal is a different case.
        assert!(!cancel_falls_back("An archive move is already running."));
        assert!(!cancel_falls_back(super::LIBRARY_BUSY));
        assert!(!cancel_falls_back("There's no archive move to cancel."));
    }

    #[test]
    fn foreign_mods_problem_pluralizes() {
        assert!(foreign_mods_problem(1).contains("already has 1 mod in subfolders"));
        assert!(foreign_mods_problem(1).contains("move it to"));
        assert!(foreign_mods_problem(4).contains("already has 4 mods in subfolders"));
        assert!(foreign_mods_problem(4).contains("move them to"));
    }

    #[test]
    fn unknown_mount_is_not_the_same_drive() {
        let c = Some((PathBuf::from("C:\\"), 1));
        let d = Some((PathBuf::from("D:\\"), 1));
        assert!(known_same_mount(&c, &c.clone()));
        assert!(!known_same_mount(&c, &d));
        assert!(!known_same_mount(&c, &None));
        assert!(!known_same_mount(&None, &None));
    }

    #[test]
    fn move_outcome_names_finished_paused_and_refused() {
        let m = MoveMarker::new("/a".into(), "/b".into());
        assert_eq!(
            move_outcome("start", &Ok(None)),
            "archive move start: finished"
        );
        assert!(move_outcome("resume", &Ok(Some(m.clone())))
            .contains("paused (on request; 0 error(s))"));
        let mut e = m.clone();
        e.errors.push("x".into());
        assert!(
            move_outcome("resume", &Ok(Some(e))).contains("paused (stopped on errors; 1 error(s))")
        );
        let mut g = m;
        g.pause_reason = Some("Farming Simulator 25 started".into());
        assert!(move_outcome("start", &Ok(Some(g))).contains("(Farming Simulator 25 started; 0"));
        assert_eq!(
            move_outcome("cancel", &Err("nope".into())),
            "archive move cancel: refused — nope"
        );
    }

    fn input(tech: &str, file: &str) -> ModInput {
        ModInput {
            tech_name: tech.into(),
            file_name: file.into(),
            kind: "zip".into(),
            category: "Tractors".into(),
            subcategory: None,
        }
    }

    #[test]
    fn split_adopt_drops_known_and_repeated_tech_names() {
        let known: HashSet<String> = ["FS25_Known".to_string()].into_iter().collect();
        let (inputs, dups) = split_adopt(
            vec![
                input("FS25_B", "FS25_B.zip"),
                input("FS25_Known", "FS25_Known.zip"),
                // Same techName twice: the first by file name wins.
                input("FS25_A", "FS25_A.zip"),
                input("FS25_A", "FS25_A"),
            ],
            &known,
        );
        let files: Vec<&str> = inputs.iter().map(|m| m.file_name.as_str()).collect();
        assert_eq!(files, ["FS25_A", "FS25_B.zip"]);
        assert_eq!(dups, ["FS25_A.zip", "FS25_Known.zip"]);
    }

    #[test]
    fn select_adopt_keeps_only_server_candidates() {
        let cands = vec![input("FS25_A", "FS25_A.zip"), input("FS25_B", "FS25_B.zip")];
        let (picked, missing) = select_adopt(
            cands,
            &["FS25_B.zip".to_string(), "..\\evil.zip".to_string()],
        );
        assert_eq!(picked.len(), 1);
        assert_eq!(picked[0].tech_name, "FS25_B");
        assert_eq!(missing, ["..\\evil.zip"]);
    }

    fn space(archive_same_drive: bool, free: Option<u64>) -> Space {
        Space {
            archive_same_drive,
            bytes: 10 << 30,
            largest: 2 << 30,
            free,
        }
    }

    fn facts(space: Space) -> MoveFacts {
        MoveFacts {
            target: vec![],
            unknown: vec![],
            missing: 0,
            collisions: vec![],
            space,
        }
    }

    #[test]
    fn move_problems_lists_everything_in_order() {
        let p = move_problems(MoveFacts {
            target: vec!["path".into()],
            unknown: vec!["FS25_B.zip is filed twice".into(), "3 mods unknown".into()],
            missing: 2,
            collisions: vec!["FS25_A: x already exists".into()],
            space: space(false, Some(1 << 30)),
        });
        assert_eq!(p.len(), 6, "{p:?}");
        assert_eq!(p[0], "path");
        assert_eq!(p[1], "FS25_B.zip is filed twice");
        assert_eq!(p[2], "3 mods unknown");
        assert!(p[3].starts_with("2 mods in your library are missing from the archive"));
        assert!(p[3].contains("rescan first"));
        assert_eq!(p[4], "FS25_A: x already exists");
        assert!(p[5].starts_with("Not enough free space"));
    }

    #[test]
    fn missing_problem_pluralizes() {
        let p = move_problems(MoveFacts {
            missing: 1,
            ..facts(space(false, None))
        });
        assert!(
            p[0].starts_with("1 mod in your library is missing"),
            "{p:?}"
        );
    }

    #[test]
    fn move_problems_on_the_same_drive_need_room_for_one_mod_at_a_time() {
        // Each mod's old copy (and any hardlink to it, re-linked first) is gone before the next
        // is copied, so the whole archive never needs to fit twice — only the largest mod.
        assert!(move_problems(facts(space(true, Some(4 << 30)))).is_empty());
        let p = move_problems(facts(space(true, Some(1 << 30))));
        assert!(p[0].starts_with("Not enough free space"), "{p:?}");
        // Unknown free space never blocks.
        assert!(move_problems(facts(space(false, None))).is_empty());
        assert!(move_problems(facts(space(true, None))).is_empty());
    }

    // Against a lock of its own: other tests hold `LIBRARY_WRITE`, so asserting "not busy" on
    // the real one would race them.
    #[test]
    fn library_busy_reflects_the_write_lock() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        assert!(!super::busy_with(&LOCK, false));
        let g = LOCK.lock().unwrap();
        assert!(super::busy_with(&LOCK, false));
        drop(g);
        assert!(!super::busy_with(&LOCK, false));
        assert!(super::busy_with(&LOCK, true), "a running move counts");
    }

    #[test]
    fn a_poisoned_but_free_lock_is_not_busy() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _ = std::thread::spawn(|| {
            let _g = LOCK.lock().unwrap();
            panic!("poison the lock (expected in this test)");
        })
        .join();
        assert!(LOCK.is_poisoned());
        assert!(!super::busy_with(&LOCK, false));
    }

    #[test]
    fn ensure_no_move_refuses_while_a_move_holds_the_lock() {
        use std::sync::atomic::Ordering::SeqCst;
        super::MOVE_HOLDS_LOCK.store(true, SeqCst);
        let r = super::ensure_no_move();
        super::MOVE_HOLDS_LOCK.store(false, SeqCst);
        assert_eq!(
            r,
            Err("Finish or cancel the archive move first.".to_string())
        );
    }

    #[test]
    fn a_finished_update_download_is_not_placed_while_a_move_is_unfinished() {
        use std::sync::atomic::Ordering::SeqCst;
        super::MOVE_HOLDS_LOCK.store(true, SeqCst);
        let r = super::update_recheck(Path::new("/nowhere/mods/FS25_A.zip"));
        super::MOVE_HOLDS_LOCK.store(false, SeqCst);
        assert_eq!(
            r.err(),
            Some("Finish or cancel the archive move first.".to_string())
        );
    }

    #[test]
    fn the_library_lock_gives_up_instead_of_waiting_forever() {
        use std::time::Duration;
        let (tx, rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel::<()>();
        // Hold the lock briefly from another thread (poison-tolerant, never panics holding it).
        let holder = std::thread::spawn(move || {
            let _g = super::LIBRARY_WRITE
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            tx.send(()).unwrap();
            let _ = done_rx.recv_timeout(Duration::from_secs(5));
        });
        rx.recv().unwrap();
        // The wait loop itself, with the move/loading checks stubbed out: `library_write`'s
        // checks read global state other tests flip in parallel, so asserting through it would
        // test those checks (often "still loading"), not the deadline.
        let start = std::time::Instant::now();
        let waited = super::wait_for_library(Duration::from_millis(300), || Ok(()));
        let waited_for = start.elapsed();
        done_tx.send(()).unwrap();
        holder.join().unwrap();
        assert_eq!(waited.err(), Some(super::LIBRARY_BUSY.to_string()));
        assert!(waited_for >= Duration::from_millis(300), "{waited_for:?}");
        assert!(waited_for < Duration::from_secs(2), "{waited_for:?}");
    }

    #[test]
    fn placing_falls_back_to_rename_only_without_hardlink_support() {
        use std::io::{Error, ErrorKind};
        let base = temp_base("place_fallback");
        let mods = base.join("mods");
        let no_links = |_: &Path, _: &Path| Err(Error::from(ErrorKind::Unsupported));
        // No hardlinks, destination free: renamed into place.
        let dest = mods.join("FS25_A.zip");
        let part = dest.with_extension("zip.part");
        std::fs::write(&part, b"a").unwrap();
        super::place_part_with(&part, &dest, no_links).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"a");
        assert!(!part.exists());
        // No hardlinks, destination taken: refused, theirs untouched, the part removed.
        std::fs::write(&part, b"b").unwrap();
        let err = super::place_part_with(&part, &dest, no_links).unwrap_err();
        assert_eq!(err, "FS25_A.zip is already in your library");
        assert_eq!(std::fs::read(&dest).unwrap(), b"a");
        assert!(!part.exists());
        // A real link failure is reported, never papered over with a rename.
        let free = mods.join("FS25_B.zip");
        let part = free.with_extension("zip.part");
        std::fs::write(&part, b"c").unwrap();
        let broken = |_: &Path, _: &Path| Err(Error::other("disk on fire"));
        let err = super::place_part_with(&part, &free, broken).unwrap_err();
        assert_eq!(err, "disk on fire");
        assert!(!free.exists());
        // The part vanished: a plain message.
        let gone = |_: &Path, _: &Path| Err(Error::from(ErrorKind::NotFound));
        let err = super::place_part_with(&part, &free, gone).unwrap_err();
        assert_eq!(err, super::DOWNLOAD_GONE);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_busy_library_fails_the_pending_downloads_and_removes_their_parts() {
        let base = temp_base("place_pending_busy");
        let dest = base.join("mods").join("FS25_Wait.zip");
        let part = super::claim_part(&dest).unwrap();
        std::fs::write(part.path(), b"done").unwrap();
        let part_path = part.path().to_path_buf();
        let mut rows = vec![super::ApplyRow {
            tech_name: "FS25_Wait".into(),
            filename: Some("FS25_Wait.zip".into()),
            status: "installed".into(),
            verdict: None,
            detail: None,
        }];
        let pending = vec![super::PendingPlace {
            row: 0,
            part,
            dest: dest.clone(),
            verdict: "verified".into(),
        }];
        let (installed, failed) =
            super::place_pending(&mut rows, pending, Err(super::LIBRARY_BUSY.to_string()));
        assert_eq!((installed, failed), (0, 1));
        assert_eq!(rows[0].status, "failed");
        assert_eq!(rows[0].detail.as_deref(), Some(super::LIBRARY_BUSY));
        assert!(!dest.exists());
        assert!(!part_path.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn claims_ignore_case_on_windows() {
        let base = temp_base("claim_case");
        let mods = base.join("mods");
        let _first = super::claim_part(&mods.join("FS25_Foo.zip")).unwrap();
        assert!(super::claim_part(&mods.join("fs25_foo.zip")).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn placing_a_download_never_overwrites_the_library() {
        let base = temp_base("place_part");
        let mods = base.join("mods");
        // Destination free: the part becomes the mod.
        let dest = mods.join("FS25_New.zip");
        let part = dest.with_extension("zip.part");
        std::fs::write(&part, b"new").unwrap();
        super::place_part(&part, &dest).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), b"new");
        assert!(!part.exists());
        // Destination taken: refused, the existing file untouched, the part removed.
        let taken = mods.join("FS25_Have.zip");
        std::fs::write(&taken, b"mine").unwrap();
        let part = taken.with_extension("zip.part");
        std::fs::write(&part, b"theirs").unwrap();
        let err = super::place_part(&part, &taken).unwrap_err();
        assert_eq!(err, "FS25_Have.zip is already in your library");
        assert_eq!(std::fs::read(&taken).unwrap(), b"mine");
        assert!(!part.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_download_claims_its_part_file_until_done() {
        let base = temp_base("claim_part");
        let dest = base.join("mods").join("FS25_Busy.zip");
        let claim = super::claim_part(&dest).unwrap();
        assert_eq!(claim.path(), dest.with_extension("zip.part").as_path());
        std::fs::write(claim.path(), b"half").unwrap();
        // A second download of the same file can't share (and corrupt) the part.
        assert!(super::claim_part(&dest).is_err());
        let part = claim.path().to_path_buf();
        drop(claim);
        // Dropping the claim removes any leftover part and frees the name.
        assert!(!part.exists());
        assert!(super::claim_part(&dest).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    fn temp_base(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("silo_lib_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("mods")).unwrap();
        base
    }

    fn game_settings(active: &str, dir: &str, port: &str) -> String {
        format!(
            "<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\" ?>\n<gameSettings \
             revision=\"15\">\n    <modsDirectoryOverride active=\"{active}\" \
             directory=\"{dir}\"/>\n    <defaultMultiplayerPort>{port}</defaultMultiplayerPort>\n\
             </gameSettings>\n"
        )
    }

    #[test]
    fn a_config_edit_can_not_move_the_mods_folder() {
        let old = game_settings("false", "C:/Temp", "10823");
        assert_eq!(
            super::override_edit_refusal(&old, &game_settings("false", "D:/Elsewhere", "10823")),
            Err(super::OVERRIDE_IS_SILOS.to_string()),
            "changed directory"
        );
        assert_eq!(
            super::override_edit_refusal(&old, &game_settings("true", "C:/Temp", "10823")),
            Err(super::OVERRIDE_IS_SILOS.to_string()),
            "changed active"
        );
        assert_eq!(
            super::override_edit_refusal(&old, &game_settings("false", "C:/Temp", "10900")),
            Ok(()),
            "an unrelated edit"
        );
        let plain =
            "<gameSettings><defaultMultiplayerPort>1</defaultMultiplayerPort></gameSettings>";
        let plain2 =
            "<gameSettings><defaultMultiplayerPort>2</defaultMultiplayerPort></gameSettings>";
        assert_eq!(
            super::override_edit_refusal(plain, plain2),
            Ok(()),
            "no override element"
        );
        assert_eq!(
            super::override_edit_refusal(plain, &old),
            Err(super::OVERRIDE_IS_SILOS.to_string()),
            "adding the element"
        );
    }

    #[test]
    fn only_game_settings_writes_are_checked_for_the_override() {
        let base = temp_base("override_kept");
        let gs = base.join("gameSettings.xml");
        std::fs::write(&gs, game_settings("false", "C:/Temp", "1")).unwrap();
        let moved = game_settings("true", "D:/Elsewhere", "1");
        assert_eq!(
            super::ensure_override_kept(&base, &gs, None, &moved),
            Err(super::OVERRIDE_IS_SILOS.to_string()),
            "the file is read when the caller hasn't"
        );
        let other = base.join("modSettings").join("FS25_X.xml");
        assert_eq!(
            super::ensure_override_kept(&base, &other, None, &moved),
            Ok(())
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_known_folders_that_exist_can_be_opened() {
        let base = temp_base("known_folder");
        std::fs::create_dir_all(base.join("mods")).unwrap();
        assert_eq!(
            super::existing_dir(Some(base.join("mods"))).unwrap(),
            base.join("mods")
        );
        assert_eq!(
            super::existing_dir(Some(base.join("gone"))).unwrap_err(),
            super::FOLDER_MISSING
        );
        assert_eq!(
            super::existing_dir(None).unwrap_err(),
            super::FOLDER_MISSING
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_update_only_replaces_a_mod_zip_in_the_library() {
        let base = temp_base("update_target");
        let mods = base.join("mods");
        let layout = Layout::default_for(&mods);
        let cat = layout.archive.join("Vehicles");
        for d in [&mods, &cat, &mods.join("backups"), &mods.join("sub")] {
            std::fs::create_dir_all(d).unwrap();
        }
        let ok = [mods.join("FS25_A.zip"), cat.join("FS25_B.zip")];
        let bad = [
            mods.join("backups").join("FS25_A.zip"),
            mods.join("FS25_A.txt"),
            layout.archive.join("FS25_C.zip"),
            mods.join("sub").join("FS25_D.zip"),
        ];
        for p in ok.iter().chain(&bad) {
            std::fs::write(p, b"zip").unwrap();
        }
        for p in &ok {
            assert_eq!(super::update_target_problem(p, &layout), None, "{p:?}");
        }
        for p in &bad {
            assert!(super::update_target_problem(p, &layout).is_some(), "{p:?}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_update_can_not_reach_a_backup_through_a_linked_category() {
        let base = temp_base("update_target_link");
        let mods = base.join("mods");
        let layout = Layout::default_for(&mods);
        let backups = mods.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        std::fs::create_dir_all(&layout.archive).unwrap();
        std::fs::write(backups.join("FS25_A.zip"), b"zip").unwrap();
        // A "category" that is a junction (Windows) or symlink (unix) into `mods/backups`.
        let linked = layout.archive.join("Linked");
        crate::organize::link_dir_native(&backups, &linked).unwrap();
        let dest = linked.join("FS25_A.zip");
        assert!(dest.is_file());
        assert!(
            super::update_target_problem(&dest, &layout).is_some(),
            "the category leads into backups"
        );
        let _ = std::fs::remove_dir(&linked);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_scan_never_forgets_the_library_while_a_custom_archive_is_missing() {
        let base = temp_base("scanmissing");
        let conn = crate::db::open(&base.join("silo.db")).unwrap();
        let mods = base.join("mods");
        let gone = base.join("unplugged");
        let active = crate::modsroot::Active {
            layout: Layout {
                mods: mods.clone(),
                archive: gone.clone(),
            },
            move_to: None,
        };
        let (archives, reconcile) = scan_archives(&conn, &mods, Some(&active));
        assert_eq!(archives, vec![gone.clone()]);
        assert!(
            !reconcile,
            "a missing custom archive must not drop organized rows"
        );
        // Plugged back in: reconcile as usual.
        std::fs::create_dir_all(&gone).unwrap();
        assert!(scan_archives(&conn, &mods, Some(&active)).1);
        // The default archive may simply not exist yet — that's an empty library, not a
        // missing drive.
        let default = crate::modsroot::Active {
            layout: Layout::default_for(&mods),
            move_to: None,
        };
        assert!(scan_archives(&conn, &mods, Some(&default)).1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_lost_database_finds_a_custom_archive_from_its_notes() {
        let base = temp_base("rediscover");
        let (mods, hdd) = (base.join("mods"), base.join("hdd"));
        std::fs::create_dir_all(&hdd).unwrap();
        crate::relocate::note_archive(&mods, Some(&hdd));
        // A brand-new database (in its own data folder): it knows nothing about the archive.
        let conn = crate::db::open(&base.join("appdata").join("silo.db")).unwrap();
        assert_eq!(super::resolve_active(&conn, &mods).layout.archive, hdd);
        let key = crate::modsroot::canonical_key(&mods);
        assert_eq!(
            crate::db::get_app_setting(&conn, &crate::relocate::archive_setting_key(&key)),
            Some(hdd.to_string_lossy().into_owned()),
            "stored again, so the next start doesn't need the notes"
        );
        let notices = crate::notices::take_mentioning(&hdd.display().to_string());
        assert!(
            notices
                .iter()
                .any(|n| n.contains("found your library archive")),
            "{notices:?}"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_rediscovered_archive_must_clear_the_fences() {
        let base = temp_base("rediscover_fenced");
        let mods = base.join("mods");
        let app_data = base.join("appdata");
        let inside_app_data = app_data.join("archive");
        std::fs::create_dir_all(&inside_app_data).unwrap();
        crate::relocate::note_archive(&mods, Some(&inside_app_data));
        let conn = crate::db::open(&app_data.join("silo.db")).unwrap();
        assert_eq!(
            super::resolve_active(&conn, &mods).layout.archive,
            crate::organize::Layout::default_for(&mods).archive,
            "an archive inside Silo's own data folder isn't taken up"
        );
        let key = crate::modsroot::canonical_key(&mods);
        assert_eq!(
            crate::db::get_app_setting(&conn, &crate::relocate::archive_setting_key(&key)),
            None
        );
        assert!(crate::notices::take_mentioning(&inside_app_data.display().to_string()).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_scan_of_another_folder_reads_its_stored_archive() {
        let base = temp_base("scanother");
        let conn = crate::db::open(&base.join("silo.db")).unwrap();
        let mods = base.join("mods");
        let other = base.join("other-mods");
        let hdd = base.join("hdd");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::create_dir_all(&hdd).unwrap();
        let key = crate::modsroot::canonical_key(&other);
        crate::relocate::store_archive_setting(&conn, &key, &other, &hdd).unwrap();
        let active = crate::modsroot::Active {
            layout: Layout::default_for(&mods),
            move_to: None,
        };
        let (archives, reconcile) = scan_archives(&conn, &other, Some(&active));
        assert_eq!(archives, vec![hdd]);
        assert!(reconcile);
        // No stored archive → that folder's default.
        let third = base.join("third");
        std::fs::create_dir_all(&third).unwrap();
        assert_eq!(
            scan_archives(&conn, &third, Some(&active)).0,
            vec![third.join("archive")]
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_scan_mid_move_reads_both_archives_and_never_reconciles() {
        let base = temp_base("scanmove");
        let conn = crate::db::open(&base.join("silo.db")).unwrap();
        let mods = base.join("mods");
        let key = crate::modsroot::canonical_key(&mods);
        let m = MoveMarker::new(mods.join("archive"), base.join("hdd"));
        crate::relocate::save_marker(&conn, &key, &m).unwrap();
        let (archives, reconcile) = scan_archives(&conn, &mods, None);
        assert_eq!(archives, vec![base.join("hdd"), mods.join("archive")]);
        assert!(!reconcile);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn repoint_is_for_a_missing_or_empty_archive_with_no_move() {
        let base = temp_base("repointgate");
        let mods = base.join("mods");
        let gone = Layout {
            mods: mods.clone(),
            archive: base.join("unplugged"),
        };
        assert_eq!(repoint_refusal(&gone, false, false), None);
        assert!(
            repoint_refusal(&gone, true, false).is_some(),
            "a move is unfinished"
        );
        let here = Layout {
            mods: mods.clone(),
            archive: base.join("hdd"),
        };
        std::fs::create_dir_all(&here.archive).unwrap();
        assert!(
            repoint_refusal(&here, false, true).is_some(),
            "an archive holding mods is moved, not re-pointed"
        );
        // Holding no mods, nothing can be orphaned: re-pointing is fine (the way back after
        // starting fresh with the default).
        assert_eq!(repoint_refusal(&here, false, false), None);
        assert_eq!(
            repoint_refusal(&Layout::default_for(&mods), false, false),
            None
        );
        assert!(repoint_refusal(&here, true, false).is_some());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_archive_holds_mods_only_when_a_category_has_one() {
        let base = temp_base("holdsmods");
        let layout = Layout::default_for(&base.join("mods"));
        assert!(!archive_holds_mods(&layout), "not created yet");
        let tools = layout.archive.join("Tools");
        std::fs::create_dir_all(&tools).unwrap();
        std::fs::write(tools.join(".FS25_A.zip.silo-tmp"), b"PK").unwrap();
        std::fs::write(tools.join("FS25_B.zip.silo-move-tmp"), b"PK").unwrap();
        std::fs::write(tools.join("notes.txt"), b"mine").unwrap();
        assert!(
            !archive_holds_mods(&layout),
            "Silo temps and other files don't count"
        );
        std::fs::write(tools.join("FS25_C.zip"), b"PK").unwrap();
        assert!(archive_holds_mods(&layout));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_unreadable_archive_holds_mods_and_is_never_reconciled() {
        let base = temp_base("unreadable");
        let conn = crate::db::open(&base.join("silo.db")).unwrap();
        let mods = base.join("mods");
        // Something exists at the default archive's path, but it can't be listed.
        std::fs::write(mods.join("archive"), b"not a folder").unwrap();
        let layout = Layout::default_for(&mods);
        assert!(
            archive_holds_mods(&layout),
            "unreadable ⇒ assume it holds mods (no re-point)"
        );
        let active = crate::modsroot::Active {
            layout,
            move_to: None,
        };
        let (_, reconcile) = scan_archives(&conn, &mods, Some(&active));
        assert!(
            !reconcile,
            "an unreadable archive must never wipe the manifest"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn repoint_fences_refuse_the_mods_folder_but_accept_a_filed_archive() {
        let base = temp_base("repointfence");
        let mods = base.join("mods");
        let restored = base.join("restored");
        std::fs::create_dir_all(restored.join("Tools")).unwrap();
        std::fs::write(restored.join("Tools/FS25_A.zip"), b"PK").unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: base.join("unplugged"),
        };
        let f = Fences {
            mods: super::key_path(&mods),
            current_archive: super::key_path(&layout.archive),
            other_mods: vec![],
            other_archives: vec![],
            user_dir: None,
            app_data: None,
        };
        // Mods filed in subfolders are the point of a re-point, not a problem.
        assert!(fence_problems(&layout, &restored, &f).is_empty());
        assert!(!fence_problems(&layout, &mods.join("sub"), &f).is_empty());
        assert!(!fence_problems(&layout, &mods, &f).is_empty());
        assert!(!fence_problems(&layout, &base, &f).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn scope_is_read_only_by_default() {
        assert_eq!(build_gh_scope(false, false), "read:user");
    }

    #[test]
    fn scope_adds_capabilities_additively() {
        assert_eq!(build_gh_scope(true, false), "read:user public_repo");
        assert_eq!(build_gh_scope(false, true), "read:user gist");
        // The union — enabling one capability while holding the other keeps both.
        assert_eq!(build_gh_scope(true, true), "read:user public_repo gist");
    }

    #[test]
    fn scope_grants_reads_both_github_formats() {
        // Device-flow response: space-separated.
        assert!(scope_grants("read:user public_repo gist", "gist"));
        assert!(scope_grants("read:user public_repo gist", "public_repo"));
        assert!(!scope_grants("read:user", "gist"));
        // X-OAuth-Scopes header: comma-separated (with spaces).
        assert!(scope_grants("gist, public_repo, read:user", "public_repo"));
        // No partial matches.
        assert!(!scope_grants("read:user public", "public_repo"));
    }

    #[test]
    fn scope_star_sentinel_grants_everything() {
        // Fine-grained PATs report no scopes; "*" means assume capable.
        assert!(scope_grants("*", "gist"));
        assert!(scope_grants("*", "public_repo"));
    }

    #[test]
    fn a_stored_catalog_address_is_ignored_in_release_builds() {
        assert_eq!(
            super::effective_siloapi_base(Some("https://evil.example".into()), false),
            siloapi::DEFAULT_BASE
        );
        assert_eq!(
            super::effective_siloapi_base(Some("http://localhost:3000".into()), true),
            "http://localhost:3000"
        );
        assert_eq!(
            super::effective_siloapi_base(None, true),
            siloapi::DEFAULT_BASE
        );
    }

    #[test]
    fn fs25_settings_writes_wait_for_the_game_to_close() {
        assert_eq!(
            super::config_write_refusal(true).unwrap_err(),
            super::CONFIG_GAME_RUNNING
        );
        assert!(super::config_write_refusal(false).is_ok());
    }

    #[test]
    fn only_pure_commands_run_on_the_main_thread() {
        // A sync #[tauri::command] runs on the main thread: disk, DB, keychain or process-list
        // work there freezes the window ("Not Responding"). Only these touch nothing.
        const PURE: &[&str] = &[
            "bisect_plan",
            "bisect_narrow",
            "app_busy",
            "force_quit",
            "set_self_updating",
            "archive_move_pause",
            "app_version",
        ];
        let src = include_str!("lib.rs");
        let mut sync = Vec::new();
        let mut lines = src.lines();
        while let Some(line) = lines.next() {
            if line.trim() != "#[tauri::command]" {
                continue;
            }
            for next in lines.by_ref() {
                let t = next.trim();
                if t.starts_with("#[") {
                    continue; // another attribute
                }
                let t = t.strip_prefix("pub ").unwrap_or(t);
                if let Some(rest) = t.strip_prefix("fn ") {
                    sync.push(rest.split(['(', '<']).next().unwrap_or("").to_string());
                }
                break;
            }
        }
        let unexpected: Vec<&String> = sync
            .iter()
            .filter(|n| !PURE.contains(&n.as_str()))
            .collect();
        assert!(
            unexpected.is_empty(),
            "make these async and run their work with off_main: {unexpected:?}"
        );
    }
}
