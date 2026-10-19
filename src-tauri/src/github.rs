//! GitHub-based mod update checking. Research (2026-07) found no official GIANTS/
//! ModHub API or RSS — but a large, growing share of FS25 mods are on GitHub with a
//! strong `releases/latest` convention. This queries the GitHub REST API for a
//! repo's latest release and compares it to the installed version.
//!
//! Unauthenticated (60 req/hr per IP) is fine for on-demand checks; a PAT can lift
//! that to 5000/hr later. All network stays in Rust (the webview CSP blocks it).

use serde::Serialize;

// ── Input validation + safe API URLs ──
// Owner/repo/gist ids reach these functions from the webview, a pasted link, or a modDesc.
// They are checked against GitHub's own naming rules and then placed into API URLs one
// percent-encoded path segment at a time, so a value like `r/collaborators/x` can never
// reach a different endpoint than the one the caller meant.

/// A GitHub user or org name: ASCII letters, digits and `-`, 1 to 39 chars.
pub fn valid_owner(s: &str) -> bool {
    !s.is_empty() && s.len() <= 39 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}

/// A GitHub repo name: ASCII letters, digits and `.` `_` `-`, 1 to 100 chars, not `.`/`..`.
pub fn valid_repo(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 100
        && s != "."
        && s != ".."
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// A gist id is an opaque hex string. Historically 32 chars; a lenient hex range survives
/// GitHub changing the length.
pub fn valid_gist_id(s: &str) -> bool {
    (20..=40).contains(&s.len()) && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// `Ok` when `owner/repo` is a valid GitHub repository name, otherwise a user-facing error.
pub fn check_repo(owner: &str, repo: &str) -> Result<(), String> {
    if valid_owner(owner) && valid_repo(repo) {
        Ok(())
    } else {
        Err(format!(
            "\"{owner}/{repo}\" isn't a valid GitHub repository name."
        ))
    }
}

/// `Ok` when `id` is a valid gist id, otherwise a user-facing error.
pub fn check_gist_id(id: &str) -> Result<(), String> {
    if valid_gist_id(id) {
        Ok(())
    } else {
        Err("That isn't a valid gist ID.".to_string())
    }
}

/// Build `https://api.github.com/<segments...>`, percent-encoding each segment (including
/// any `/`, `?` or `#` inside one), so the host and the endpoint are fixed by the caller.
/// Refuses an empty, `.`/`..`, or control-character segment (see `net::safe_path_segment`).
pub(crate) fn api_url(segments: &[&str]) -> Result<url::Url, String> {
    if !segments.iter().all(|s| crate::net::safe_path_segment(s)) {
        return Err("That GitHub address isn't valid.".to_string());
    }
    let mut u = url::Url::parse("https://api.github.com/").expect("static URL");
    u.path_segments_mut()
        .expect("https URL has a path")
        .clear()
        .extend(segments);
    Ok(u)
}

/// The Contents API URL for `path` in `owner/repo`. `path` may hold `/` separators for a
/// nested file; each piece becomes its own encoded segment.
fn contents_url(owner: &str, repo: &str, path: &str) -> Result<url::Url, String> {
    let mut segs = vec!["repos", owner, repo, "contents"];
    segs.extend(path.split('/').filter(|p| !p.is_empty()));
    api_url(&segs)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseInfo {
    pub tag: String,
    pub name: Option<String>,
    pub published_at: Option<String>,
    pub html_url: Option<String>,
    pub asset_url: Option<String>,
    pub asset_name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub has_update: bool,
    pub current: String,
    pub release: ReleaseInfo,
}

/// Fetch the latest release for `owner/repo`. A token (when present) raises the
/// rate limit to 5000/hr and allows private repos.
pub fn latest_release(owner: &str, repo: &str, token: Option<&str>) -> Result<ReleaseInfo, String> {
    check_repo(owner, repo)?;
    let url = api_url(&["repos", owner, repo, "releases", "latest"])?;
    let mut req = crate::net::api()
        .get(url.as_str())
        .set("Accept", "application/vnd.github+json");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    let resp = req.call().map_err(|e| match e {
        ureq::Error::Status(404, _) => {
            "This mod's GitHub page has no releases yet, so Silo can't check it for updates."
                .to_string()
        }
        ureq::Error::Status(403, r) => forbidden_text(
            r.header("x-ratelimit-remaining"),
            r.header("retry-after").is_some(),
            token.is_some(),
        ),
        other => transport_text(other),
    })?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;

    let tag = v["tag_name"].as_str().unwrap_or("").to_string();
    if tag.is_empty() {
        return Err("Latest release has no tag".into());
    }
    // Prefer a .zip asset (the mod itself).
    let mut asset_url = None;
    let mut asset_name = None;
    if let Some(assets) = v["assets"].as_array() {
        for a in assets {
            let name = a["name"].as_str().unwrap_or("");
            if name.to_lowercase().ends_with(".zip") {
                asset_url = a["browser_download_url"].as_str().map(String::from);
                asset_name = Some(name.to_string());
                break;
            }
        }
    }
    Ok(ReleaseInfo {
        tag,
        name: v["name"].as_str().map(String::from),
        published_at: v["published_at"].as_str().map(String::from),
        html_url: v["html_url"].as_str().map(String::from),
        asset_url,
        asset_name,
    })
}

/// Compare a release tag against the installed version, tolerating a leading `v`
/// and `.`/`-`/`+` separators.
pub fn is_newer(latest_tag: &str, current: &str) -> bool {
    let norm = |s: &str| {
        s.trim_start_matches(['v', 'V'])
            .split(['.', '-', '+', '_'])
            .filter_map(|p| p.parse::<u64>().ok())
            .collect::<Vec<u64>>()
    };
    let a = norm(latest_tag);
    let b = norm(current);
    for i in 0..a.len().max(b.len()) {
        let x = a.get(i).copied().unwrap_or(0);
        let y = b.get(i).copied().unwrap_or(0);
        if x != y {
            return x > y;
        }
    }
    false
}

/// Check a repo and compare to the installed version.
pub fn check(
    owner: &str,
    repo: &str,
    current: &str,
    token: Option<&str>,
) -> Result<UpdateInfo, String> {
    let release = latest_release(owner, repo, token)?;
    Ok(UpdateInfo {
        has_update: is_newer(&release.tag, current),
        current: current.to_string(),
        release,
    })
}

// ── OAuth Device Flow (RFC 8628) — no client secret, ideal for desktop ──

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PollResult {
    /// "ok" | "pending" | "slow_down" | "expired" | "denied" | "error"
    pub status: String,
    pub token: Option<String>,
    /// The scopes GitHub actually granted (space-separated), from the token response.
    pub scope: Option<String>,
    pub error: Option<String>,
}

/// Step 1: request a device + user code for the given OAuth App client id.
/// `scope` is space-separated OAuth scopes. Read-only identity is `read:user`;
/// to let the user star/watch through Silo we also need `public_repo` (GitHub's
/// docs: "Also required for starring public repositories").
pub fn device_start(client_id: &str, scope: &str) -> Result<DeviceCode, String> {
    let resp = crate::net::api()
        .post("https://github.com/login/device/code")
        .set("Accept", "application/json")
        .send_form(&[("client_id", client_id), ("scope", scope)])
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    if let Some(err) = v["error"].as_str() {
        return Err(format!(
            "{}: {}",
            err,
            v["error_description"]
                .as_str()
                .unwrap_or("device code request failed")
        ));
    }
    Ok(DeviceCode {
        device_code: v["device_code"].as_str().unwrap_or("").to_string(),
        user_code: v["user_code"].as_str().unwrap_or("").to_string(),
        verification_uri: v["verification_uri"]
            .as_str()
            .unwrap_or("https://github.com/login/device")
            .to_string(),
        interval: v["interval"].as_u64().unwrap_or(5),
        expires_in: v["expires_in"].as_u64().unwrap_or(900),
    })
}

/// Step 2 (polled): exchange the device code for a token once the user approves.
pub fn device_poll(client_id: &str, device_code: &str) -> Result<PollResult, String> {
    let resp = crate::net::api()
        .post("https://github.com/login/oauth/access_token")
        .set("Accept", "application/json")
        .send_form(&[
            ("client_id", client_id),
            ("device_code", device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .map_err(|e| e.to_string())?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    if let Some(tok) = v["access_token"].as_str() {
        return Ok(PollResult {
            status: "ok".into(),
            token: Some(tok.to_string()),
            // GitHub echoes the scopes it actually granted — the source of truth for
            // what this token can do (vs. what we asked for).
            scope: v["scope"].as_str().map(str::to_string),
            error: None,
        });
    }
    let err = v["error"].as_str().unwrap_or("error");
    let status = match err {
        "authorization_pending" => "pending",
        "slow_down" => "slow_down",
        "expired_token" => "expired",
        "access_denied" => "denied",
        _ => "error",
    };
    Ok(PollResult {
        status: status.into(),
        token: None,
        scope: None,
        error: Some(err.to_string()),
    })
}

/// True only for GitHub-owned hosts. The token is attached ONLY to these — never to an
/// arbitrary URL handed in from the webview/catalog, so a hostile `downloadUrl` can't
/// harvest the user's credential.
///
/// The host comes from a real URL parser (the same one the HTTP client uses), so tricks
/// like userinfo (`github.com@evil`) or a backslash read as a path separator can't make
/// the string check and the actual request disagree.
pub fn is_github_host(url: &str) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    // Require HTTPS — never attach a bearer credential to a plaintext `http://github.com`
    // URL, where it would be sent in the clear before any TLS redirect.
    if u.scheme() != "https" {
        return false;
    }
    match u.host_str().map(|h| h.to_ascii_lowercase()) {
        Some(h) => {
            matches!(
                h.as_str(),
                "github.com" | "api.github.com" | "codeload.github.com"
            ) || h.ends_with(".githubusercontent.com")
        }
        None => false,
    }
}

/// Updates come only from GitHub release assets: the URL must pass the outbound-URL guard
/// (https, no private/loopback address) and name a GitHub host, or nothing is fetched.
pub(crate) fn check_update_url(url: &str) -> Result<(), String> {
    crate::net::validate_outbound_url(url, false)?;
    if !is_github_host(url) {
        return Err("Updates can only be downloaded from GitHub.".into());
    }
    Ok(())
}

/// Verify a downloaded update is actually the mod it's replacing, before overwriting the
/// installed archive. Two guards:
///
/// 1. **Structural** — the new zip must contain a readable root `modDesc.xml`. Rejects a source
///    archive, docs bundle, or unrelated release asset that merely happens to be a valid zip.
/// 2. **Identity** — when the current file is itself a readable mod, the new one must identify as
///    the *same* mod: matching `uniqueType` if both declare one, otherwise matching author+title.
///
/// Only a *provable* mismatch refuses the update. Missing/uncomparable identity fields (or an
/// unreadable current file) fall through to the structural guard, so a legitimate update is never
/// blocked just because a modDesc omits a field.
pub(crate) fn verify_update_identity(
    new_zip: &std::path::Path,
    dest: &std::path::Path,
) -> Result<(), String> {
    let new_xml = crate::scan::read_moddesc_xml(new_zip, "zip").map_err(|_| {
        "the downloaded file isn't a Farming Simulator mod (no modDesc.xml). It may be a source \
         archive or the wrong release asset."
            .to_string()
    })?;
    let new_md = crate::moddesc::parse(&new_xml);

    // Compare against the currently-installed mod only if we can read it as a mod.
    let cur_md = if dest.exists() {
        crate::scan::read_moddesc_xml(dest, "zip")
            .ok()
            .map(|xml| crate::moddesc::parse(&xml))
    } else {
        None
    };
    let Some(cur_md) = cur_md else { return Ok(()) };

    // Prefer uniqueType — GIANTS' explicit identity primitive — when both declare one.
    if let (Some(cur_ut), Some(new_ut)) =
        (cur_md.unique_type.as_deref(), new_md.unique_type.as_deref())
    {
        if !cur_ut.trim().eq_ignore_ascii_case(new_ut.trim()) {
            return Err(format!(
                "refusing to replace this mod: the download declares uniqueType {new_ut:?}, but \
                 the installed mod is {cur_ut:?}, so it's a different mod"
            ));
        }
        return Ok(());
    }

    // Otherwise fall back to author + title (stable across versions of the same mod).
    if let (Some(ca), Some(ct), Some(na), Some(nt)) = (
        cur_md.author.as_deref(),
        cur_md.title.as_deref(),
        new_md.author.as_deref(),
        new_md.title.as_deref(),
    ) {
        let same =
            ca.trim().eq_ignore_ascii_case(na.trim()) && ct.trim().eq_ignore_ascii_case(nt.trim());
        if !same {
            return Err(format!(
                "refusing to replace this mod: the download identifies as {nt:?} by {na:?}, not \
                 the installed {ct:?} by {ca:?}"
            ));
        }
    }
    Ok(())
}

/// The authenticated user's login name (verifies a token).
pub fn whoami(token: &str) -> Result<String, String> {
    Ok(whoami_scoped(token)?.0)
}

/// The authenticated user's login plus the token's granted scopes (the `X-OAuth-Scopes`
/// response header, comma-separated). OAuth tokens and classic PATs populate it; a
/// fine-grained PAT uses a different permission model and reports an empty header — the
/// caller decides how to treat "unknown" scopes.
pub fn whoami_scoped(token: &str) -> Result<(String, String), String> {
    let resp = crate::net::api()
        .get("https://api.github.com/user")
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(gh_err)?;
    let scopes = resp.header("X-OAuth-Scopes").unwrap_or("").to_string();
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok((v["login"].as_str().unwrap_or("").to_string(), scopes))
}

// ── GitHub source card: live public reads + user-owned actions ──
//
// The interaction lands on GitHub through the user's own token; Silo only reflects
// state. Reads here are fetched live when the drawer opens (per SiloAPI ENRICHMENT.md,
// rich per-source read fields are the client's job, not the aggregate API's).

/// Public repo signals for the GitHub card. `you_starred` / `you_watching` are only
/// meaningful (non-null) when a token is supplied.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoStats {
    pub full_name: String,
    pub html_url: String,
    pub stars: u64,
    pub forks: u64,
    pub watchers: u64,
    pub open_issues: u64,
    pub archived: bool,
    pub pushed_at: Option<String>,
    pub you_starred: Option<bool>,
    pub you_watching: Option<bool>,
}

fn gh_get(url: &str, token: Option<&str>) -> Result<serde_json::Value, String> {
    let mut req = crate::net::api()
        .get(url)
        .set("Accept", "application/vnd.github+json");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    req.call()
        .map_err(gh_err)?
        .into_json()
        .map_err(|e| e.to_string())
}

/// A 403 from the releases API: a rate limit only when GitHub says so (no requests left, or a
/// Retry-After), with the "connect" hint only for a user who isn't connected; otherwise GitHub
/// refused this repo (blocked, or the token can't see it).
fn forbidden_text(remaining: Option<&str>, retry_after: bool, has_token: bool) -> String {
    if remaining == Some("0") || retry_after {
        if has_token {
            "GitHub rate limit hit. Try again in a while.".to_string()
        } else {
            "GitHub rate limit hit (60 an hour without a connected account). Try again later."
                .to_string()
        }
    } else {
        "GitHub refused access to this repo's releases.".to_string()
    }
}

/// Map a ureq error to a friendly string, calling out the scope case so the UI can
/// prompt the user to re-connect with actions enabled.
fn gh_err(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(401, _) => {
            "GitHub rejected the token. Reconnect your account.".to_string()
        }
        ureq::Error::Status(403, _) => {
            "This token can't perform that action. Reconnect GitHub and allow the extra \
             permission."
                .to_string()
        }
        ureq::Error::Status(404, _) => {
            "Silo couldn't find this on GitHub. It may have been renamed, made private or deleted."
                .to_string()
        }
        other => transport_text(other),
    }
}

/// A ureq error as text, except a refused header: ureq quotes the whole header line in that
/// error, so for `Authorization` it would carry the token into the UI and the log.
fn transport_text(e: ureq::Error) -> String {
    match &e {
        ureq::Error::Transport(t) if t.kind() == ureq::ErrorKind::BadHeader => {
            "GitHub request had a malformed header, so it wasn't sent.".to_string()
        }
        _ => e.to_string(),
    }
}

/// The longest token GitHub issues is well under this (fine-grained PATs are 93 characters).
const PAT_MAX_LEN: usize = 255;

/// A pasted personal access token, trimmed of surrounding whitespace and the invisible
/// characters a copy can bring along (a byte-order mark, a zero-width space). What's left must
/// be one run of printable ASCII: anything else can't be a GitHub token, and would make the
/// HTTP request fail with an error that quotes it. The error never repeats the input.
pub fn clean_pat(input: &str) -> Result<String, String> {
    let pat = input.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}' || c == '\u{200b}');
    if pat.is_empty() {
        return Err("Paste a token first.".to_string());
    }
    if pat.len() > PAT_MAX_LEN || !pat.bytes().all(|b| b.is_ascii_graphic()) {
        return Err(
            "That doesn't look like a GitHub token. Copy it again as one line, \
                    with no spaces or line breaks."
                .to_string(),
        );
    }
    Ok(pat.to_string())
}

/// GET /repos/{owner}/{repo} plus (when authenticated) the user's star/watch state.
pub fn repo_stats(owner: &str, repo: &str, token: Option<&str>) -> Result<RepoStats, String> {
    check_repo(owner, repo)?;
    let v = gh_get(api_url(&["repos", owner, repo])?.as_str(), token)?;
    if v["full_name"].as_str().unwrap_or("").is_empty() {
        return Err(
            "Silo couldn't find this on GitHub. It may have been renamed, made private or deleted."
                .into(),
        );
    }
    let (you_starred, you_watching) = match token {
        Some(t) => (
            star_state(owner, repo, t).ok(),
            watch_state(owner, repo, t).ok(),
        ),
        None => (None, None),
    };
    Ok(RepoStats {
        full_name: v["full_name"].as_str().unwrap_or("").to_string(),
        html_url: v["html_url"].as_str().unwrap_or("").to_string(),
        stars: v["stargazers_count"].as_u64().unwrap_or(0),
        forks: v["forks_count"].as_u64().unwrap_or(0),
        // `watchers_count` mirrors stars on the REST API; `subscribers_count` is the
        // real "watching" number.
        watchers: v["subscribers_count"].as_u64().unwrap_or(0),
        open_issues: v["open_issues_count"].as_u64().unwrap_or(0),
        archived: v["archived"].as_bool().unwrap_or(false),
        pushed_at: v["pushed_at"].as_str().map(String::from),
        you_starred,
        you_watching,
    })
}

/// A GET that treats 204 as true and 404 as false — the shape GitHub uses for
/// "is this thing in the user's set?" checks.
fn present_check(url: &str, token: &str) -> Result<bool, String> {
    match crate::net::api()
        .get(url)
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(_) => Ok(true),
        Err(ureq::Error::Status(404, _)) => Ok(false),
        Err(e) => Err(gh_err(e)),
    }
}

/// GET /user/starred/{owner}/{repo} — 204 starred, 404 not.
pub fn star_state(owner: &str, repo: &str, token: &str) -> Result<bool, String> {
    check_repo(owner, repo)?;
    present_check(api_url(&["user", "starred", owner, repo])?.as_str(), token)
}

/// GET /repos/{owner}/{repo}/subscription — 200 watching, 404 not.
pub fn watch_state(owner: &str, repo: &str, token: &str) -> Result<bool, String> {
    check_repo(owner, repo)?;
    present_check(
        api_url(&["repos", owner, repo, "subscription"])?.as_str(),
        token,
    )
}

/// PUT/DELETE /user/starred/{owner}/{repo}. Needs `public_repo` (OAuth) or a PAT with
/// the Starring permission. Returns the new starred state (= `on`).
pub fn set_star(owner: &str, repo: &str, token: &str, on: bool) -> Result<bool, String> {
    check_repo(owner, repo)?;
    let url = api_url(&["user", "starred", owner, repo])?;
    let req = if on {
        crate::net::api().put(url.as_str())
    } else {
        crate::net::api().delete(url.as_str())
    }
    .set("Accept", "application/vnd.github+json")
    .set("Content-Length", "0")
    .set("Authorization", &format!("Bearer {token}"));
    req.call().map_err(gh_err)?;
    Ok(on)
}

/// PUT (subscribe) / DELETE (unsubscribe) /repos/{owner}/{repo}/subscription.
pub fn set_watch(owner: &str, repo: &str, token: &str, on: bool) -> Result<bool, String> {
    check_repo(owner, repo)?;
    let url = api_url(&["repos", owner, repo, "subscription"])?;
    if on {
        crate::net::api()
            .put(url.as_str())
            .set("Accept", "application/vnd.github+json")
            .set("Authorization", &format!("Bearer {token}"))
            .send_json(ureq::json!({ "subscribed": true, "ignored": false }))
            .map_err(gh_err)?;
    } else {
        crate::net::api()
            .delete(url.as_str())
            .set("Accept", "application/vnd.github+json")
            .set("Content-Length", "0")
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(gh_err)?;
    }
    Ok(on)
}

// ── Gists (Collections transport) ─────────────────────────────────────────────
// A shared Collection is written to the user's own GitHub as a *secret* gist. Secret
// here means unlisted (not indexed/discoverable) — NOT access-controlled: anyone with
// the link can read it. The UI is responsible for saying so. Creating/reading a gist
// the user owns needs the `gist` OAuth scope (see `build_gh_scope`).

/// A gist we created or read — just the fields Collections needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GistRef {
    pub id: String,
    /// The shareable web URL the user copies.
    pub html_url: String,
}

/// POST /gists — create a secret gist holding a single named file. Needs `gist` scope.
pub fn create_secret_gist(
    token: &str,
    description: &str,
    filename: &str,
    content: &str,
) -> Result<GistRef, String> {
    let body = ureq::json!({
        "description": description,
        "public": false,
        "files": { filename: { "content": content } },
    });
    let v = crate::net::api()
        .post("https://api.github.com/gists")
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(body)
        .map_err(gh_err)?
        .into_json::<serde_json::Value>()
        .map_err(|e| e.to_string())?;
    let id = v["id"].as_str().unwrap_or_default().to_string();
    let html_url = v["html_url"].as_str().unwrap_or_default().to_string();
    if id.is_empty() {
        return Err("GitHub did not return a gist id".into());
    }
    Ok(GistRef { id, html_url })
}

/// PATCH /gists/{id} — add or replace a named file in a gist we own. Used to attach a README
/// after creation (the README links the handoff page, whose URL needs the server-assigned id).
pub fn update_gist_file(
    token: &str,
    id: &str,
    filename: &str,
    content: &str,
) -> Result<(), String> {
    check_gist_id(id)?;
    crate::net::api()
        .request("PATCH", api_url(&["gists", id])?.as_str())
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(ureq::json!({ "files": { filename: { "content": content } } }))
        .map_err(gh_err)?;
    Ok(())
}

/// One of the user's gists, from the list endpoint — enough to spot Silo collections
/// (a gist carrying the collection file) without reading each one's content.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GistListItem {
    pub id: String,
    pub description: String,
    pub created_at: Option<String>,
    pub html_url: String,
    pub filenames: Vec<String>,
}

/// GET /gists — the authenticated user's own gists (first page, newest first). Needs `gist`.
pub fn list_user_gists(token: &str) -> Result<Vec<GistListItem>, String> {
    let v = gh_get("https://api.github.com/gists?per_page=100", Some(token))?;
    let arr = v
        .as_array()
        .ok_or("Unexpected gists response from GitHub")?;
    Ok(arr
        .iter()
        .map(|g| GistListItem {
            id: g["id"].as_str().unwrap_or_default().to_string(),
            description: g["description"].as_str().unwrap_or_default().to_string(),
            created_at: g["created_at"].as_str().map(String::from),
            html_url: g["html_url"].as_str().unwrap_or_default().to_string(),
            filenames: g["files"]
                .as_object()
                .map(|o| o.keys().cloned().collect())
                .unwrap_or_default(),
        })
        .collect())
}

/// DELETE /gists/{id} — unpublish a gist we own. Needs `gist`.
pub fn delete_gist(token: &str, id: &str) -> Result<(), String> {
    check_gist_id(id)?;
    crate::net::api()
        .delete(api_url(&["gists", id])?.as_str())
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(gh_err)?;
    Ok(())
}

/// One of the user's owned repos, from the list endpoint — enough to spot collection repos
/// (name-prefixed `silo-`) before reading each one's collection file.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoListItem {
    pub full_name: String,
    pub name: String,
    pub html_url: String,
    pub created_at: Option<String>,
}

/// GET /user/repos (owned, newest first) — needs the `public_repo`/repo scope Silo already
/// requests. Deleting a repo would need the separate `delete_repo` scope (not requested), so
/// collection repos are list-only in-app; deletion happens on GitHub.
pub fn list_owned_repos(token: &str) -> Result<Vec<RepoListItem>, String> {
    let v = gh_get(
        "https://api.github.com/user/repos?per_page=100&affiliation=owner&sort=created&direction=desc",
        Some(token),
    )?;
    let arr = v
        .as_array()
        .ok_or("Unexpected repos response from GitHub")?;
    Ok(arr
        .iter()
        .map(|r| RepoListItem {
            full_name: r["full_name"].as_str().unwrap_or_default().to_string(),
            name: r["name"].as_str().unwrap_or_default().to_string(),
            html_url: r["html_url"].as_str().unwrap_or_default().to_string(),
            created_at: r["created_at"].as_str().map(String::from),
        })
        .collect())
}

/// GET /gists/{id} and return the named file's content. Reads are unauthenticated-capable,
/// but pass the owner token when available (higher rate limit; required for a secret gist
/// the anonymous API won't return). Errors if the file is absent from the gist.
pub fn read_gist_file(id: &str, filename: &str, token: Option<&str>) -> Result<String, String> {
    check_gist_id(id)?;
    let v = gh_get(api_url(&["gists", id])?.as_str(), token)?;
    let file = &v["files"][filename];
    if file.is_null() {
        return Err(format!(
            "That gist has no {filename}. It may not be a Silo shared set."
        ));
    }
    // GitHub inlines file content, but truncates very large files and gives a raw_url.
    // Our collection JSON is tiny, so this is defensive; follow raw_url if it happens.
    if file["truncated"].as_bool().unwrap_or(false) {
        if let Some(raw) = file["raw_url"].as_str() {
            return fetch_gist_raw(raw, token);
        }
    }
    file["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Gist file had no readable content".to_string())
}

/// Fetch a gist file's raw content (the `raw_url` GitHub returns for a truncated file).
/// Guarded by `is_github_host` — the URL comes from GitHub's own response, but we never
/// attach a bearer token to a non-GitHub host regardless.
fn fetch_gist_raw(url: &str, token: Option<&str>) -> Result<String, String> {
    if !is_github_host(url) {
        return Err("Refusing to fetch gist content from a non-GitHub host".into());
    }
    let mut req = crate::net::api().get(url);
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    req.call()
        .map_err(gh_err)?
        .into_string()
        .map_err(|e| e.to_string())
}

/// Read a file from a public repo's default branch via the Contents API. The
/// `application/vnd.github.raw` media type returns the file bytes directly (no base64),
/// and the API resolves the default branch for us. Token optional — public repos read
/// anonymously; pass it for the higher rate limit. This is the P2 (public-repo) transport
/// counterpart to `read_gist_file`.
pub fn read_repo_file(
    owner: &str,
    repo: &str,
    path: &str,
    token: Option<&str>,
) -> Result<String, String> {
    check_repo(owner, repo)?;
    let url = contents_url(owner, repo, path)?;
    let mut req = crate::net::api()
        .get(url.as_str())
        .set("Accept", "application/vnd.github.raw");
    if let Some(t) = token {
        req = req.set("Authorization", &format!("Bearer {t}"));
    }
    req.call()
        .map_err(gh_err)?
        .into_string()
        .map_err(|e| e.to_string())
}

/// A repo we created — the fields Collections needs.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub full_name: String,
    pub html_url: String,
}

/// POST /user/repos — create a public repo for a shared collection (P2). `auto_init` is
/// off so we own the first commit and can create both files cleanly via the Contents API;
/// needs the `public_repo` scope Silo already requests for star/watch.
pub fn create_public_repo(token: &str, name: &str, description: &str) -> Result<RepoRef, String> {
    let body = ureq::json!({
        "name": name,
        "description": description,
        "private": false,
        "auto_init": false,
    });
    let v = crate::net::api()
        .post("https://api.github.com/user/repos")
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(body)
        .map_err(|e| match e {
            ureq::Error::Status(422, _) => {
                "You already have a repository with that name. Rename the collection or \
                 delete the old repo."
                    .to_string()
            }
            other => gh_err(other),
        })?
        .into_json::<serde_json::Value>()
        .map_err(|e| e.to_string())?;
    let full_name = v["full_name"].as_str().unwrap_or_default().to_string();
    let html_url = v["html_url"].as_str().unwrap_or_default().to_string();
    if full_name.is_empty() {
        return Err("GitHub did not return the created repository".into());
    }
    Ok(RepoRef {
        full_name,
        html_url,
    })
}

/// PUT /repos/{owner}/{repo}/contents/{path} — create a file on the `main` branch. The
/// Contents API requires base64-encoded content; on an auto_init:false repo the first PUT
/// bootstraps the branch. Needs `public_repo` scope.
pub fn put_repo_file(
    token: &str,
    owner: &str,
    repo: &str,
    path: &str,
    content: &str,
    message: &str,
) -> Result<(), String> {
    use base64::Engine;
    check_repo(owner, repo)?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let mut body = ureq::json!({
        "message": message,
        "content": encoded,
        "branch": "main",
    });
    // Creating a file needs no sha; *replacing* an existing one requires its current blob
    // sha, or GitHub 422s. Best-effort lookup — absent (new file / empty repo) → create.
    if let Some(sha) = repo_file_sha(token, owner, repo, path) {
        body["sha"] = serde_json::Value::String(sha);
    }
    crate::net::api()
        .put(contents_url(owner, repo, path)?.as_str())
        .set("Accept", "application/vnd.github+json")
        .set("Authorization", &format!("Bearer {token}"))
        .send_json(body)
        .map_err(gh_err)?;
    Ok(())
}

/// The current blob sha of a repo file, if it exists — needed to replace it via the Contents
/// API. Best-effort: any error (missing file, empty repo, network) yields `None` → treat as new.
fn repo_file_sha(token: &str, owner: &str, repo: &str, path: &str) -> Option<String> {
    check_repo(owner, repo).ok()?;
    let v = gh_get(contents_url(owner, repo, path).ok()?.as_str(), Some(token)).ok()?;
    v["sha"].as_str().map(String::from)
}

/// Best-effort scan of arbitrary text (a modDesc.xml) for the first
/// `github.com/owner/repo` reference. Skips non-repo GitHub paths and names that aren't
/// valid GitHub owner/repo names.
pub fn find_repo_in_text(text: &str) -> Option<(String, String)> {
    let ident = |s: &str| {
        s.chars()
            .take_while(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
            .collect::<String>()
    };
    let skip = [
        "sponsors",
        "settings",
        "orgs",
        "topics",
        "about",
        "features",
        "marketplace",
    ];

    let mut from = 0;
    while let Some(i) = text[from..].find("github.com/") {
        let after = &text[from + i + "github.com/".len()..];
        let mut segs = after.split('/');
        let owner = ident(segs.next().unwrap_or(""));
        let mut repo = ident(segs.next().unwrap_or(""));
        if let Some(stripped) = repo.strip_suffix(".git") {
            repo = stripped.to_string();
        }
        // Only a real GitHub name counts; otherwise keep scanning for a later, valid link.
        if valid_owner(&owner) && valid_repo(&repo) && !skip.contains(&owner.as_str()) {
            return Some((owner, repo));
        }
        from += i + "github.com/".len();
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_403_is_a_rate_limit_only_when_github_says_so() {
        // No requests left: a rate limit; the connect hint only without a token.
        let anon = super::forbidden_text(Some("0"), false, false);
        assert!(anon.contains("rate limit") && anon.contains("without a connected account"));
        let connected = super::forbidden_text(Some("0"), false, true);
        assert!(connected.contains("rate limit") && !connected.contains("connected account"));
        assert!(super::forbidden_text(None, true, true).contains("rate limit"));
        // Requests left: GitHub refused the repo, which isn't a rate limit.
        let refused = super::forbidden_text(Some("4999"), false, true);
        assert!(!refused.to_lowercase().contains("rate limit"));
        assert_eq!(
            crate::error::classify(&refused),
            crate::error::code::UNKNOWN
        );
    }

    use super::*;

    #[test]
    fn a_pasted_token_is_one_clean_line_or_refused() {
        let tok = "ghp_0123456789abcdefABCDEF0123456789abcd";
        assert_eq!(clean_pat(&format!("  {tok}\n")).unwrap(), tok);
        assert_eq!(clean_pat(&format!("\u{feff}{tok}\u{200b}")).unwrap(), tok);
        for bad in [
            String::new(),
            " \u{200b} ".to_string(),
            format!("{}\n{}", &tok[..20], &tok[20..]),
            format!("{}\u{200b}{}", &tok[..20], &tok[20..]),
            format!("{} {}", &tok[..20], &tok[20..]),
            format!("{tok}é"),
            "x".repeat(256),
        ] {
            let err = clean_pat(&bad).unwrap_err();
            assert!(
                !err.contains(&tok[4..12]),
                "the error never echoes the token: {err}"
            );
        }
    }

    #[test]
    fn a_malformed_auth_header_never_reaches_the_error_text() {
        // ureq refuses the header before connecting and quotes the whole header line in its
        // error, token included; gh_err must not pass that on.
        let secret = "ghp_SECRETsecret0123456789";
        let err = crate::net::api()
            .get("https://api.github.com/user")
            .set("Authorization", &format!("Bearer {secret}\u{200b}"))
            .call()
            .unwrap_err();
        assert!(
            err.to_string().contains(secret),
            "ureq's own text does carry it"
        );
        let text = gh_err(err);
        assert!(!text.contains("SECRET"), "{text}");
    }

    #[test]
    fn finds_repo() {
        let t = r#"<description>Docs at https://github.com/Stephan-S/FS25_AutoDrive/wiki cool</description>"#;
        assert_eq!(
            find_repo_in_text(t),
            Some(("Stephan-S".into(), "FS25_AutoDrive".into()))
        );
        assert_eq!(
            find_repo_in_text("visit github.com/loki79uk/FS25_UniversalAutoload.git"),
            Some(("loki79uk".into(), "FS25_UniversalAutoload".into()))
        );
        assert_eq!(find_repo_in_text("no links here"), None);
        assert_eq!(
            find_repo_in_text("github.com/sponsors/foo then github.com/real/repo"),
            Some(("real".into(), "repo".into()))
        );
    }

    #[test]
    fn token_only_attaches_to_github_hosts() {
        assert!(is_github_host(
            "https://github.com/o/r/releases/download/v1/a.zip"
        ));
        assert!(is_github_host("https://api.github.com/repos/o/r"));
        assert!(is_github_host("https://objects.githubusercontent.com/x"));
        assert!(is_github_host("https://codeload.github.com/o/r/zip"));
        // Spoofing attempts must NOT be treated as GitHub.
        assert!(!is_github_host("https://github.com.evil.com/steal"));
        assert!(!is_github_host("https://evil.com/github.com/a.zip"));
        assert!(!is_github_host("https://raw.githubusercontent.example/x"));
        assert!(!is_github_host("https://staticdelivery.example.com/a.png"));
        assert!(!is_github_host("http://github.com@evil.com/x"));
        // Plaintext HTTP must never qualify — a bearer token can't be sent in the clear.
        assert!(!is_github_host(
            "http://github.com/o/r/releases/download/v1/a.zip"
        ));
        assert!(!is_github_host("http://api.github.com/repos/o/r"));
    }

    #[test]
    fn owner_repo_and_gist_validation() {
        assert!(valid_owner("HLLMR") && valid_owner("a-b-1"));
        for bad in ["", "a/b", "a b", "a?", "..", &"x".repeat(40)] {
            assert!(!valid_owner(bad), "{bad}");
        }
        assert!(valid_repo("silo") && valid_repo("FS25_Mod.v2") && valid_repo("a-b_c"));
        for bad in [
            "",
            ".",
            "..",
            "r/collaborators/x",
            "r?x",
            "r#x",
            &"x".repeat(101),
        ] {
            assert!(!valid_repo(bad), "{bad}");
        }
        assert!(valid_gist_id("aa5a315d61ae9438b18d"));
        assert!(!valid_gist_id("../../user/repos") && !valid_gist_id("xyz"));
        assert!(check_repo("HLLMR", "Silo").is_ok());
        assert!(check_repo("HLLMR", "r/collaborators/x").is_err());
        assert!(check_gist_id("aa5a315d61ae9438b18d").is_ok());
        assert!(check_gist_id("../../user/repos").is_err());
    }

    #[test]
    fn api_url_refuses_dot_empty_and_control_segments() {
        // The url crate drops tab/LF/CR after its own dot check, so ".\t." would turn into a
        // real `..` and climb out of the intended endpoint.
        let evil = [
            "repos",
            "o",
            "r",
            "contents",
            ".\t.",
            "..\t",
            "collaborators",
            "evil",
        ];
        assert!(api_url(&evil).is_err());
        assert!(api_url(&["repos", "o", "r", "contents", "..", "x"]).is_err());
        assert!(api_url(&["repos", "o", "r", "contents", "."]).is_err());
        assert!(api_url(&["repos", "o", "", "x"]).is_err());
        assert!(api_url(&["gists", "a\nb"]).is_err());
        assert!(contents_url("o", "r", "a/../../x").is_err());
        assert!(contents_url("o", "r", ".\t./x").is_err());
        let err = api_url(&["repos", ".."]).unwrap_err();
        assert_eq!(err, "That GitHub address isn't valid.");
    }

    #[test]
    fn repo_guess_skips_invalid_names_and_keeps_scanning() {
        assert_eq!(
            find_repo_in_text("see github.com/user_name/x then github.com/real-owner/repo"),
            Some(("real-owner".into(), "repo".into()))
        );
        assert_eq!(find_repo_in_text("only github.com/user_name/x here"), None);
    }

    #[test]
    fn api_urls_escape_every_segment() {
        let u = api_url(&["repos", "o", "r", "releases", "latest"]).unwrap();
        assert_eq!(
            u.as_str(),
            "https://api.github.com/repos/o/r/releases/latest"
        );
        let evil = api_url(&["repos", "o", "r/collaborators/x?", "subscription"]).unwrap();
        assert_eq!(evil.host_str(), Some("api.github.com"));
        assert!(evil.path().contains("r%2Fcollaborators%2Fx%3F"));
        assert_eq!(
            contents_url("o", "r", "dir/silo-collection.json")
                .unwrap()
                .as_str(),
            "https://api.github.com/repos/o/r/contents/dir/silo-collection.json"
        );
    }

    #[test]
    fn github_host_check_parses_the_real_host() {
        assert!(is_github_host(
            "https://github.com/o/r/releases/download/v1/x.zip"
        ));
        assert!(is_github_host("https://objects.githubusercontent.com/x"));
        assert!(!is_github_host("http://github.com/x"));
        assert!(!is_github_host(
            "https://169.254.169.254\\.githubusercontent.com/x"
        ));
        assert!(!is_github_host("https://github.com.evil.example/x"));
        assert!(!is_github_host("https://evil.example/@github.com/x"));
    }

    #[test]
    fn update_downloads_only_come_from_github() {
        let err = check_update_url("https://evil.example/x.zip").unwrap_err();
        assert!(err.contains("GitHub"), "{err}");
        let err = check_update_url("https://127.0.0.1/x.zip").unwrap_err();
        assert!(!err.is_empty());
        assert!(check_update_url("http://github.com/o/r/releases/download/v1/x.zip").is_err());
        assert!(check_update_url("https://github.com/o/r/releases/download/v1/x.zip").is_ok());
    }

    #[test]
    fn version_compare() {
        assert!(is_newer("v1.2.0", "1.1.0"));
        assert!(is_newer("2.0.0.0", "1.9.9.9"));
        assert!(!is_newer("1.0.0.0", "1.0.0.0"));
        assert!(!is_newer("1.0", "1.0.0.1"));
        assert!(is_newer("v8.1.0.3", "8.1.0.2"));
    }

    // ── Update identity guard ──
    fn zip_with(path: &std::path::Path, files: &[(&str, &str)]) {
        use std::io::Write;
        let f = std::fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        for (name, body) in files {
            zw.start_file(*name, opts).unwrap();
            zw.write_all(body.as_bytes()).unwrap();
        }
        zw.finish().unwrap();
    }
    fn md(author: &str, title: &str) -> String {
        format!(
            "<modDesc descVersion=\"100\"><author>{author}</author><title><en>{title}</en></title></modDesc>"
        )
    }
    fn md_ut(ut: &str) -> String {
        format!(
            "<modDesc descVersion=\"100\"><author>X</author><title><en>Y</en></title><uniqueType>{ut}</uniqueType></modDesc>"
        )
    }

    #[test]
    fn update_identity_guard() {
        let dir = std::env::temp_dir().join(format!("silo_upd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let cur = dir.join("FS25_Cool.zip");
        zip_with(&cur, &[("modDesc.xml", &md("Alice", "Cool Mod"))]);

        // Same mod, newer build → accepted.
        let ok = dir.join("new_ok.zip");
        zip_with(&ok, &[("modDesc.xml", &md("Alice", "Cool Mod"))]);
        assert!(verify_update_identity(&ok, &cur).is_ok());

        // Valid zip, WRONG mod (different author/title) → refused.
        let wrong = dir.join("new_wrong.zip");
        zip_with(&wrong, &[("modDesc.xml", &md("Bob", "Other Mod"))]);
        assert!(verify_update_identity(&wrong, &cur).is_err());

        // Valid zip that isn't a mod at all (no modDesc.xml) → refused.
        let notmod = dir.join("new_notmod.zip");
        zip_with(&notmod, &[("readme.txt", "just a source archive")]);
        assert!(verify_update_identity(&notmod, &cur).is_err());

        // Fresh install (no current file): a real mod passes the structural guard, a non-mod fails.
        let missing = dir.join("does_not_exist.zip");
        assert!(verify_update_identity(&ok, &missing).is_ok());
        assert!(verify_update_identity(&notmod, &missing).is_err());

        // uniqueType is the strong identity: match accepted, mismatch refused.
        let cur_ut = dir.join("cur_ut.zip");
        zip_with(&cur_ut, &[("modDesc.xml", &md_ut("ALICE_COOL"))]);
        let ut_same = dir.join("ut_same.zip");
        zip_with(&ut_same, &[("modDesc.xml", &md_ut("ALICE_COOL"))]);
        let ut_diff = dir.join("ut_diff.zip");
        zip_with(&ut_diff, &[("modDesc.xml", &md_ut("BOB_OTHER"))]);
        assert!(verify_update_identity(&ut_same, &cur_ut).is_ok());
        assert!(verify_update_identity(&ut_diff, &cur_ut).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
