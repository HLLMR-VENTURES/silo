//! Silo core — Tauri command surface. All heavy logic lives in sibling modules so
//! it stays unit-testable (and reusable by a future CLI) without a running app.

pub mod bindings;
pub mod bisect;
pub mod bridge;
pub mod category;
pub mod collection;
pub mod conflicts;
pub mod db;
pub mod fsgame;
pub mod gamelaunch;
pub mod github;
pub mod icons;
pub mod logscan;
pub mod moddesc;
pub mod modsroot;
pub mod mpsync;
pub mod net;
pub mod organize;
pub mod paths;
pub mod provenance;
pub mod savegame;
pub mod scan;
pub mod secrets;
pub mod settings_form;
pub mod siloapi;
pub mod store;
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

/// Return the auto-detected default mod root(s) as strings for the UI.
#[tauri::command]
fn default_mods_paths() -> Vec<String> {
    fsgame::default_mods_paths()
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
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
        _ => fsgame::default_mods_paths(),
    };

    tauri::async_runtime::spawn_blocking(move || {
        // Warm cache: parsed entries persist between launches, keyed by
        // path+mtime+size, so unchanged mods skip archive parsing entirely.
        let mut conn = db::open(&db_path(&app)?)?;
        let cache = db::load_cache(&conn);

        let emitter = app.clone();
        let out = scan::scan_cached(roots, &cache, move |done, total| {
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
        let _ = db::reconcile_organized(&mut conn, &organized_rows);

        Ok::<_, String>(out.result)
    })
    .await
    .map_err(|e| e.to_string())?
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
    let cache_dir = app.path().app_cache_dir().ok()?.join("icons");
    tauri::async_runtime::spawn_blocking(move || {
        icons::cached_data_url(&cache_dir, std::path::Path::new(&path), &kind, &icon)
    })
    .await
    .ok()
    .flatten()
}

// ── Curation (favorite / hidden / broken) ──
#[tauri::command]
fn get_curation(app: tauri::AppHandle) -> Result<Vec<db::CurationRow>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_curation(&conn))
}

#[tauri::command]
fn set_curation(app: tauri::AppHandle, row: db::CurationRow) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    db::set_curation(&conn, &row)
}

// ── GitHub update checking ──
#[tauri::command]
fn get_mod_repos(app: tauri::AppHandle) -> Result<Vec<db::RepoRow>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_repos(&conn))
}

#[tauri::command]
fn set_mod_repo(
    app: tauri::AppHandle,
    tech_name: String,
    owner: String,
    repo: String,
) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    db::set_repo(&conn, &tech_name, &owner, &repo)
}

/// Scan a mod's modDesc.xml for a github.com/owner/repo reference.
#[tauri::command]
fn guess_repo(path: String, kind: String) -> Option<db::RepoRow> {
    let xml = scan::read_moddesc_xml(std::path::Path::new(&path), &kind).ok()?;
    let (owner, repo) = github::find_repo_in_text(&xml)?;
    Some(db::RepoRow {
        tech_name: String::new(),
        owner,
        repo,
    })
}

#[tauri::command]
async fn check_mod_update(
    app: tauri::AppHandle,
    owner: String,
    repo: String,
    current: String,
) -> Result<github::UpdateInfo, String> {
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
fn gh_status(app: tauri::AppHandle) -> Result<GhStatus, String> {
    let conn = db::open(&db_path(&app)?)?;
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
}

#[tauri::command]
fn gh_set_client_id(app: tauri::AppHandle, client_id: String) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    let v = client_id.trim();
    db::set_app_setting(
        &conn,
        "gh_client_id",
        if v.is_empty() { None } else { Some(v) },
    )
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

#[tauri::command]
async fn download_update(
    _app: tauri::AppHandle,
    path: String,
    asset_url: String,
) -> Result<(), String> {
    // No DB/token needed: a public release asset is downloaded unauthenticated.
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let dest = std::path::Path::new(&path);
        // Defence against a compromised webview naming an arbitrary write target: the path
        // must contain no `..` AND its parent must resolve inside a real mods root (the flat
        // root or its `archive/`). An absolute path outside the mods folder — which
        // `no_traversal` alone would pass — is rejected here.
        paths::no_traversal(dest)?;
        paths::ensure_write_under(&allowed_roots(), dest)?;
        // Public release assets don't need auth, and a download must never carry the user's
        // GitHub credential — keep the token for explicit API actions only.
        github::download_zip(&asset_url, None, dest)
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── SiloAPI (mod browser / discovery) ──

fn siloapi_base(app: &tauri::AppHandle) -> Result<String, String> {
    let conn = db::open(&db_path(app)?)?;
    Ok(db::get_app_setting(&conn, "siloapi_base")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| siloapi::DEFAULT_BASE.to_string()))
}

#[tauri::command]
fn siloapi_status(app: tauri::AppHandle) -> Result<String, String> {
    siloapi_base(&app)
}

#[tauri::command]
fn siloapi_set_base(app: tauri::AppHandle, base: String) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    let v = base.trim().trim_end_matches('/');
    // SSRF guard: a custom base drives every catalog request, so validate it before storing.
    // Allow localhost so a self-hoster/dev can point at their own endpoint; block private IPs.
    if !v.is_empty() {
        net::validate_outbound_url(v, true)?;
    }
    db::set_app_setting(
        &conn,
        "siloapi_base",
        if v.is_empty() { None } else { Some(v) },
    )
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

/// Fetch a catalog mod thumbnail as a `data:` URL (the webview can't set the referer the
/// Giants CDN now requires). Cached on disk by URL so each image is fetched at most once.
/// STOPGAP: the polite long-term fix is SiloAPI caching + serving these during its sweeps.
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
#[tauri::command]
async fn install_remote_mod(
    app: tauri::AppHandle,
    id: String,
    source: Option<String>,
    root: Option<String>,
) -> Result<String, String> {
    let base = siloapi_base(&app)?;
    let root = primary_root(root)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let resolved = siloapi::resolve_download(&base, &id, source.as_deref())?;
        // The filename comes from the catalog — validate it's a plain basename so a
        // hostile response can't redirect the write outside the mods folder.
        paths::safe_file_name(&resolved.filename)?;
        let dest = root.join(&resolved.filename);
        if dest.exists() {
            return Err(format!("{} is already in your library", resolved.filename));
        }
        siloapi::download_to(
            &resolved.url,
            &dest,
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
        Ok(resolved.filename)
    })
    .await
    .map_err(|e| e.to_string())?
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

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct LibraryTags {
    tech_name: String,
    tags: Vec<siloapi::Tag>,
}

/// Fetch catalog semantic tags for the installed library in one batch (silo-api#9), so the
/// Library can build the same facet dropdowns as Browse. Best-effort — mods with no catalog
/// match (or an older server) come back absent; only tagged mods are returned.
#[tauri::command]
async fn catalog_library_tags(
    app: tauri::AppHandle,
    tech_names: Vec<String>,
) -> Result<Vec<LibraryTags>, String> {
    let base = siloapi_base(&app)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<LibraryTags>, String> {
        let results = siloapi::lookup(&base, &tech_names)?;
        Ok(results
            .into_iter()
            .filter_map(|r| {
                let tech = r.tech_name?;
                if r.tags.is_empty() {
                    None
                } else {
                    Some(LibraryTags {
                        tech_name: tech,
                        tags: r.tags,
                    })
                }
            })
            .collect())
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
fn gh_logout(app: tauri::AppHandle) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    secrets::set(&conn, "gh_token", None)?;
    db::set_app_setting(&conn, "gh_user", None)?;
    db::set_app_setting(&conn, "gh_scopes", None)?;
    // Legacy flags from before scope capture — clear them too so nothing lingers.
    db::set_app_setting(&conn, "gh_write", None)?;
    db::set_app_setting(&conn, "gh_gist", None)?;
    Ok(())
}

// ── Tags ──
#[tauri::command]
fn get_tags(app: tauri::AppHandle) -> Result<Vec<db::TagRow>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_tags(&conn))
}

#[tauri::command]
fn set_tags(app: tauri::AppHandle, tech_name: String, tags: Vec<String>) -> Result<(), String> {
    let mut conn = db::open(&db_path(&app)?)?;
    db::set_tags(&mut conn, &tech_name, &tags)
}

// ── Manual category overrides ──
#[tauri::command]
fn get_overrides(app: tauri::AppHandle) -> Result<Vec<db::CategoryOverride>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_overrides(&conn))
}

#[tauri::command]
fn set_override(app: tauri::AppHandle, row: db::CategoryOverride) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    db::set_override(&conn, &row)
}

// ── Organize / projection engine (writes to the game folder) ──

/// The mod roots Silo is allowed to write to. Today that's the auto-detected game mod
/// folder(s); a future custom-root feature will add to this set through a native folder
/// picker. This is the single source of truth for "where may we write" — Rust, not the
/// webview, is the authority.
fn allowed_roots() -> Vec<PathBuf> {
    fsgame::default_mods_paths()
}

/// Resolve a frontend-supplied root to a TRUSTED one. Empty → the first detected root.
/// Otherwise the path must canonicalize to one of the allowed roots, or it's rejected —
/// so a compromised webview can't redirect an install/organize into an arbitrary directory.
/// Returns the matched allowed root (the trusted path), never the raw frontend string.
fn primary_root(root: Option<String>) -> Result<PathBuf, String> {
    let allowed = allowed_roots();
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
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let conn = db::open(&db)?;
        Ok(organize::apply_organize(&conn, &root, &mods))
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
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = db::open(&db)?;
        organize::adopt_foreign(&conn, &root, &file_name)
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
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<(), String> {
        let conn = db::open(&db)?;
        organize::restore_projection(&conn, &root, &file_name)
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
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let conn = db::open(&db)?;
        let set: HashSet<String> = active.into_iter().collect();
        Ok(organize::set_active(&conn, &root, &set))
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
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::RelinkReport, String> {
        let conn = db::open(&db)?;
        Ok(organize::relink_projections(&conn, &root, |done, total| {
            let _ = emitter.emit("relink-progress", (done, total));
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn flatten(app: tauri::AppHandle, root: Option<String>) -> Result<organize::Report, String> {
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<organize::Report, String> {
        let conn = db::open(&db)?;
        Ok(organize::flatten(&conn, &root))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn get_organized(app: tauri::AppHandle) -> Result<Vec<db::OrganizedRow>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_organized(&conn))
}

// ── Loadouts (named active-mod sets) ──
#[tauri::command]
fn get_loadouts(app: tauri::AppHandle) -> Result<Vec<db::Loadout>, String> {
    let conn = db::open(&db_path(&app)?)?;
    Ok(db::load_loadouts(&conn))
}

#[tauri::command]
fn save_loadout(
    app: tauri::AppHandle,
    id: Option<i64>,
    name: String,
    mods: Vec<String>,
) -> Result<i64, String> {
    let mut conn = db::open(&db_path(&app)?)?;
    db::save_loadout(&mut conn, id, &name, &mods)
}

#[tauri::command]
fn delete_loadout(app: tauri::AppHandle, id: i64) -> Result<(), String> {
    let mut conn = db::open(&db_path(&app)?)?;
    db::delete_loadout(&mut conn, id)
}

#[derive(serde::Serialize, serde::Deserialize)]
struct LoadoutFile {
    silo: u32,
    name: String,
    mods: Vec<String>,
}

#[tauri::command]
fn export_loadout(app: tauri::AppHandle, id: i64, path: String) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
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
}

#[tauri::command]
fn import_loadout(app: tauri::AppHandle, path: String) -> Result<i64, String> {
    paths::safe_inbound(std::path::Path::new(&path), &["silo", "json"])?;
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let file: LoadoutFile = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut conn = db::open(&db_path(&app)?)?;
    db::save_loadout(&mut conn, None, &file.name, &file.mods)
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
    tauri::async_runtime::spawn_blocking(gamelaunch::launch)
        .await
        .map_err(|e| e.to_string())?
}

/// Write text to a user-chosen path (used by the diagnostics report export). Confined to a
/// report file type with no traversal, so it can't be turned into an arbitrary-file write.
#[tauri::command]
fn save_text(path: String, content: String) -> Result<(), String> {
    paths::safe_outbound(std::path::Path::new(&path), &["md", "txt"])?;
    std::fs::write(&path, content).map_err(|e| e.to_string())
}

/// Clear the scan cache so the next scan re-parses & re-categorizes every mod.
#[tauri::command]
fn clear_scan_cache(app: tauri::AppHandle) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    db::clear_cache(&conn);
    Ok(())
}

/// The FS25 user data dir (parent of mods/, savegameN/, game.xml).
#[tauri::command]
fn user_dir_path() -> Option<String> {
    fsgame::user_dir().map(|p| p.to_string_lossy().into_owned())
}

/// Generate a filltype-compatibility bridge companion mod into the library root.
/// Returns the created zip filename; the frontend rescans to pick it up.
#[tauri::command]
async fn generate_bridge(spec: bridge::BridgeSpec, root: Option<String>) -> Result<String, String> {
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<String, String> {
        let filename = format!("{}.zip", spec.tech_name);
        let dest = root.join(&filename);
        if dest.exists() {
            return Err(format!("{filename} already exists in your mods folder"));
        }
        bridge::generate(&spec, &dest)?;
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
    collection::parse(&json)
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
        let coll = read_collection_json(&url_or_id, token.as_deref())?;

        let installed_by: std::collections::HashMap<&str, Option<&str>> = installed
            .iter()
            .map(|m| (m.tech_name.as_str(), m.version.as_deref()))
            .collect();

        // One lookup resolves catalog presence + best installable source for every mod.
        let names: Vec<String> = coll.mods.iter().map(|m| m.tech_name.clone()).collect();
        let hits = siloapi::lookup(&base, &names).unwrap_or_default();
        let by_tech: std::collections::HashMap<&str, &siloapi::LookupResult> = hits
            .iter()
            .filter_map(|r| r.tech_name.as_deref().map(|t| (t, r)))
            .collect();

        let mut plan = ImportPlan {
            name: coll.name.clone(),
            description: coll.description.clone(),
            author: coll.author.clone(),
            will_install: Vec::new(),
            open_page: Vec::new(),
            already_present: Vec::new(),
            version_drift: Vec::new(),
            unresolved: Vec::new(),
        };

        for m in &coll.mods {
            let hit = by_tech.get(m.tech_name.as_str());
            let installable_source = hit
                .and_then(|r| r.download.as_ref())
                .map(|d| d.source.clone())
                .or_else(|| m.source.clone());
            let row = |installed_version: Option<String>| PlanRow {
                tech_name: m.tech_name.clone(),
                version: m.version.clone(),
                source: installable_source.clone(),
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
                    let installable = hit.is_some_and(|r| r.download.is_some());
                    if installable {
                        plan.will_install.push(row(None));
                    } else if hit.is_some() {
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
    /// The loadout created from the collection's full mod list.
    loadout_id: i64,
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
#[tauri::command]
async fn collection_apply(
    app: tauri::AppHandle,
    url_or_id: String,
    installed: Vec<LocalMod>,
    root: Option<String>,
) -> Result<ApplyReport, String> {
    let base = siloapi_base(&app)?;
    let db = db_path(&app)?;
    let root = primary_root(root)?;
    let emitter = app.clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<ApplyReport, String> {
        let mut conn = db::open(&db)?;
        let token = secrets::get(&conn, "gh_token");
        let coll = read_collection_json(&url_or_id, token.as_deref())?;

        let have: std::collections::HashSet<&str> =
            installed.iter().map(|m| m.tech_name.as_str()).collect();

        // Fill catalog ids for anything the collection didn't carry one for.
        let names: Vec<String> = coll.mods.iter().map(|m| m.tech_name.clone()).collect();
        let hits = siloapi::lookup(&base, &names).unwrap_or_default();
        let id_by_tech: std::collections::HashMap<&str, &str> = hits
            .iter()
            .filter_map(|r| r.tech_name.as_deref().map(|t| (t, r.id.as_str())))
            .collect();

        let total = coll.mods.len();
        let mut rows: Vec<ApplyRow> = Vec::with_capacity(total);
        let mut installed_count = 0usize;
        let mut failed_count = 0usize;

        for (i, m) in coll.mods.iter().enumerate() {
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

            let catalog_id = m
                .catalog_id
                .as_deref()
                .or_else(|| id_by_tech.get(m.tech_name.as_str()).copied());
            let Some(cid) = catalog_id else {
                rows.push(ApplyRow {
                    tech_name: m.tech_name.clone(),
                    filename: None,
                    status: "skipped".into(),
                    verdict: None,
                    detail: Some("Not in the catalog — find it manually.".into()),
                });
                continue;
            };

            match siloapi::resolve_download(&base, cid, m.source.as_deref()) {
                Ok(resolved) => {
                    if let Err(e) = paths::safe_file_name(&resolved.filename) {
                        rows.push(fail_row(&m.tech_name, e));
                        failed_count += 1;
                        continue;
                    }
                    let dest = root.join(&resolved.filename);
                    if dest.exists() {
                        rows.push(ApplyRow {
                            tech_name: m.tech_name.clone(),
                            filename: Some(resolved.filename.clone()),
                            status: "present".into(),
                            verdict: None,
                            detail: None,
                        });
                        continue;
                    }
                    match siloapi::download_to(
                        &resolved.url,
                        &dest,
                        resolved.expected_sha256.as_deref(),
                        |_done, _total| {},
                    ) {
                        Ok(()) => {
                            installed_count += 1;
                            rows.push(ApplyRow {
                                tech_name: m.tech_name.clone(),
                                filename: Some(resolved.filename.clone()),
                                status: "installed".into(),
                                verdict: Some(verdict_against_pin(
                                    &dest,
                                    m.manifest_hash.as_deref(),
                                )),
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
                Err(_) => {
                    rows.push(ApplyRow {
                        tech_name: m.tech_name.clone(),
                        filename: None,
                        status: "skipped".into(),
                        verdict: None,
                        detail: Some("Download-gated — get it from its page.".into()),
                    });
                }
            }
        }

        // The loadout lists the whole collection; applying it activates the ones you have,
        // and it completes automatically once you fetch the rest and rescan.
        let tech_names = coll.tech_names();
        let loadout_id = db::save_loadout(&mut conn, None, &coll.name, &tech_names)?;

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

fn fail_row(tech_name: &str, detail: String) -> ApplyRow {
    ApplyRow {
        tech_name: tech_name.to_string(),
        filename: None,
        status: "failed".into(),
        verdict: None,
        detail: Some(detail),
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
fn bisect_snapshot_save(app: tauri::AppHandle, active: Vec<String>) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    let json = serde_json::to_string(&active).map_err(|e| e.to_string())?;
    db::set_app_setting(&conn, "bisect_snapshot", Some(&json))
}

/// The saved pre-bisection active set, if a bisection is (or was) in progress.
#[tauri::command]
fn bisect_snapshot_get(app: tauri::AppHandle) -> Result<Option<Vec<String>>, String> {
    let conn = db::open(&db_path(&app)?)?;
    match db::get_app_setting(&conn, "bisect_snapshot") {
        Some(json) => Ok(Some(
            serde_json::from_str(&json).map_err(|e| e.to_string())?,
        )),
        None => Ok(None),
    }
}

/// Clear the snapshot once the original set has been restored.
#[tauri::command]
fn bisect_snapshot_clear(app: tauri::AppHandle) -> Result<(), String> {
    let conn = db::open(&db_path(&app)?)?;
    db::set_app_setting(&conn, "bisect_snapshot", None)
}

/// Read specific values from a config XML by path.
#[tauri::command]
fn get_config(
    path: String,
    paths: Vec<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    let xml = crate::paths::guarded_xml_read(&[user], std::path::Path::new(&path))?;
    Ok(xmlconfig::get_values(&xml, &paths))
}

/// Apply value edits to a config XML. The write is confined to the FS25 user dir, backs up
/// the original first (required), and swaps the new file in atomically (see
/// `paths::guarded_xml_write`).
#[tauri::command]
fn set_config(path: String, edits: Vec<xmlconfig::Edit>) -> Result<(), String> {
    let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    let roots = [user];
    let p = std::path::Path::new(&path);
    let xml = paths::guarded_xml_read(&roots, p)?;
    let out = xmlconfig::set_values(&xml, &edits)?;
    paths::guarded_xml_write(&roots, p, &out)
}

// ── Mod settings form ──
#[tauri::command]
fn mods_with_settings() -> Vec<String> {
    match fsgame::user_dir() {
        Some(dir) => settings_form::mods_with_settings(&dir),
        None => Vec::new(),
    }
}

#[tauri::command]
fn get_mod_settings(mod_name: String) -> Result<Vec<settings_form::SettingsFile>, String> {
    let dir = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    let mut files = Vec::new();
    for path in settings_form::find_files(&dir, &mod_name) {
        files.push(settings_form::load_file(&path)?);
    }
    Ok(files)
}

#[tauri::command]
fn save_mod_settings(path: String, edits: Vec<settings_form::Edit>) -> Result<(), String> {
    let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    let roots = [user];
    let p = std::path::Path::new(&path);
    let raw = paths::guarded_xml_read(&roots, p)?;
    let updated = settings_form::apply_edits(&raw, &edits)?;
    paths::guarded_xml_write(&roots, p, &updated)
}

#[tauri::command]
fn save_mod_settings_raw(path: String, content: String) -> Result<(), String> {
    let user = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    paths::guarded_xml_write(&[user], std::path::Path::new(&path), &content)
}

// ── Savegames ──
#[tauri::command]
fn get_savegames() -> Result<Vec<savegame::Savegame>, String> {
    match fsgame::user_dir() {
        Some(dir) => Ok(savegame::list_savegames(&dir)),
        None => Ok(Vec::new()),
    }
}

#[tauri::command]
fn backup_savegame(folder: String) -> Result<String, String> {
    let dir = fsgame::user_dir().ok_or_else(|| "No FS25 user dir".to_string())?;
    savegame::backup(&dir, &folder)
}

/// The app version from Cargo/tauri.conf package info — the single source of truth
/// the UI reads so a build always shows the number it was cut from.
#[tauri::command]
fn app_version(app: tauri::AppHandle) -> String {
    app.package_info().version.to_string()
}

/// True when this machine can store account tokens in the OS keychain. When false, the UI
/// warns that a connected token would be kept in the local DB unencrypted.
#[tauri::command]
fn secret_storage_secure() -> bool {
    secrets::keychain_available()
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
    let root = primary_root(root)?;
    tauri::async_runtime::spawn_blocking(move || -> Result<Vec<organize::ForeignFile>, String> {
        let conn = db::open(&db)?;
        Ok(organize::detect_foreign_projections(&conn, &root))
    })
    .await
    .map_err(|e| e.to_string())?
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
        .invoke_handler(tauri::generate_handler![
            default_mods_paths,
            scan_mods,
            get_mod_icon,
            get_curation,
            set_curation,
            get_tags,
            set_tags,
            get_mod_repos,
            set_mod_repo,
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
            catalog_library_tags,
            verify_mod,
            get_overrides,
            set_override,
            plan_organize,
            apply_organize,
            set_active,
            flatten,
            get_organized,
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
            secret_storage_secure,
            detect_foreign_files,
            adopt_foreign_file,
            restore_foreign_file,
            relink_projections,
            hardlink_support
        ])
        .run(tauri::generate_context!())
        .expect("error while running Silo");
}

#[cfg(test)]
mod tests {
    use super::{build_gh_scope, scope_grants};

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
}
