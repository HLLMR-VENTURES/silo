//! Client for SiloAPI — our canonical FS mod database (https://silo-api.hllmr.com).
//! This is the "Browse" tab's backend: search the catalog, read a mod's cross-source
//! pointers, and install a browsed mod's .zip straight into the library.
//!
//! All network stays in Rust (the webview CSP blocks HTTP). In debug builds only, the base
//! URL is overridable via the `siloapi_base` app_setting so a developer can point it at their
//! own SiloAPI; release builds always use `DEFAULT_BASE` (see `effective_siloapi_base`).

use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

pub const DEFAULT_BASE: &str = "https://silo-api.hllmr.com";

/// A catalog row as returned by `GET /mods`. Fields mirror the API (camelCase).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowseMod {
    pub id: String,
    #[serde(default)]
    pub tech_name: Option<String>,
    pub title: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    // Full mod body + changelog (see SiloAPI docs/ENRICHMENT.md), separate from the
    // short `description` summary. None until the server ingests them.
    #[serde(default)]
    pub description_full: Option<String>,
    #[serde(default)]
    pub changelog: Option<String>,
    #[serde(default)]
    pub image_url: Option<String>,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub trust_score: Option<i64>,
    // Provenance (SiloAPI docs/PROVENANCE-CANONICALIZATION.md). Present when the latest
    // version has been hashed (GitHub-source versions in P1). `verified` = a canonical hash
    // exists to compare against; the client still does the byte-level match itself.
    #[serde(default)]
    pub verified: Option<bool>,
    /// The author's claim through SiloAPI's author program (A7). None when unclaimed or on an
    /// API that predates claims; Browse shows "Verified author" only when it's present.
    #[serde(default, deserialize_with = "lenient_author")]
    pub verified_author: Option<VerifiedAuthor>,
    #[serde(default)]
    pub archive_sha256: Option<String>,
    #[serde(default)]
    pub manifest_hash: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
    // Popularity signals (see SiloAPI docs/ENRICHMENT.md). All optional: they stay None
    // until the server-side backfill lands, and the UI hides any badge that is None.
    #[serde(default)]
    pub downloads: Option<i64>,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub rating_count: Option<i64>,
    #[serde(default)]
    pub popularity: Option<f64>,
    /// Every place this mod can be got from, best-first. Drives the per-source buttons.
    #[serde(default)]
    pub sources: Vec<ModSource>,
    /// Where to send the user when nothing is directly installable.
    #[serde(default)]
    pub page_url: Option<String>,
    /// Semantic facet tags (brand/theme/region/realism/era), highest-confidence first.
    /// Populated on the detail endpoint; empty on list results.
    #[serde(default)]
    pub tags: Vec<Tag>,
    /// Real-world production-year range for identifiable machines (drives `availableBy`).
    #[serde(default)]
    pub year_from: Option<i64>,
    #[serde(default)]
    pub year_to: Option<i64>,
}

/// Who claimed a catalog mod (`verifiedAuthor: {login, avatarUrl, profileUrl}`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedAuthor {
    pub login: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
    #[serde(default)]
    pub profile_url: Option<String>,
}

/// A claim that doesn't parse (no login, wrong shape) counts as no claim, so one odd row can
/// never fail a whole catalog page.
fn lenient_author<'de, D>(d: D) -> Result<Option<VerifiedAuthor>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let v = Option::<serde_json::Value>::deserialize(d)?;
    Ok(
        v.and_then(|v| serde_json::from_value::<VerifiedAuthor>(v).ok())
            .filter(|a| !a.login.trim().is_empty()),
    )
}

/// One semantic facet tag on a mod (see SiloAPI docs/TAGGING.md). `confidence` lets the client
/// threshold ("don't cry wolf"); `source` is the provenance (heuristic:* or llm:*).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub namespace: String,
    pub value: String,
    #[serde(default)]
    pub confidence: Option<f64>,
    #[serde(default)]
    pub source: Option<String>,
}

/// The `/facets` response: available filter values per namespace, with mod counts.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct FacetValue {
    pub value: String,
    pub count: i64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Facets {
    pub facets: std::collections::HashMap<String, Vec<FacetValue>>,
}

/// GET /facets — the available facet values (brand/theme/region/realism/era) + counts, so the
/// UI can render filter chips. Best-effort: an older server without the route just yields none.
pub fn facets(base: &str) -> Result<Facets, String> {
    let url = format!("{}/facets", base.trim_end_matches('/'));
    get(&url)?.into_json().map_err(|e| e.to_string())
}

#[derive(Debug, Clone, Deserialize)]
struct BrowseResponse {
    mods: Vec<BrowseMod>,
    #[serde(default)]
    total: u64,
}

/// A page of catalog results plus how many match the filter overall, so the UI can
/// show "60 of 386" and know when it has reached the end.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowsePage {
    pub mods: Vec<BrowseMod>,
    pub total: u64,
}

/// One place a mod can be got from, as the API reports it. The API decides
/// `installable` (ModHub's CDN 403s hotlinked GETs), so the
/// client never re-derives that policy — it just renders a button per source.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModSource {
    pub source: String,
    pub source_url: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub installable: bool,
    /// Only present when `installable` — the API strips URLs it knows won't serve us.
    #[serde(default)]
    pub download_url: Option<String>,
    // Raw per-source popularity signals — a source only fills what it natively reports
    // (GitHub: downloads+stars, ModHub: rating+count).
    #[serde(default)]
    pub downloads: Option<i64>,
    #[serde(default)]
    pub rating: Option<f64>,
    #[serde(default)]
    pub stars: Option<i64>,
}

// The detail endpoint (`GET /mods/:id`) returns the same shape as a list row, so it
// deserializes straight into `BrowseMod` — see `detail()`. (A former `ModDetail` struct
// dropped the aggregate popularity fields + pageUrl and is gone.)

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub mods: u64,
    pub sources: u64,
}

/// A catalog category and how many mods carry it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryCount {
    pub category: String,
    pub count: u64,
}

#[derive(Debug, Deserialize)]
struct CategoriesResponse {
    #[serde(default)]
    categories: Vec<CategoryCount>,
}

/// A best-download pointer from the batch lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Download {
    pub source: String,
    pub url: String,
}

/// One row from `POST /mods/lookup`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LookupResult {
    pub tech_name: Option<String>,
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub latest_version: Option<String>,
    #[serde(default)]
    pub download: Option<Download>,
    /// The mod's page (ModHub first) for when nothing is directly installable. Absent on
    /// older servers.
    #[serde(default)]
    pub page_url: Option<String>,
    /// Semantic tags (silo-api#9) — same shape as `/mods/:id`. Empty on older servers.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

#[derive(Debug, Deserialize)]
struct LookupResponse {
    #[serde(default)]
    results: Vec<LookupResult>,
}

fn get(url: &str) -> Result<ureq::Response, String> {
    crate::net::api().get(url).call().map_err(|e| match e {
        ureq::Error::Status(code, _) => format!("SiloAPI returned {code}"),
        other => format!("Could not reach SiloAPI: {other}"),
    })
}

/// Search/list the catalog. `query`/`category` are optional filters.
// Many optional query filters (search / category / sort / tags / year / paging) that each map
// straight to a URL param; a params struct would only add indirection over this URL builder.
#[allow(clippy::too_many_arguments)]
pub fn browse(
    base: &str,
    query: Option<&str>,
    category: Option<&str>,
    sort: Option<&str>,
    tags: &[String],
    available_by: Option<u32>,
    limit: u32,
    offset: u32,
) -> Result<BrowsePage, String> {
    let mut url = format!(
        "{}/mods?limit={}&offset={}",
        base.trim_end_matches('/'),
        limit,
        offset
    );
    if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
        url.push_str(&format!("&q={}", urlencode(q.trim())));
    }
    if let Some(c) = category.filter(|s| !s.trim().is_empty()) {
        url.push_str(&format!("&category={}", urlencode(c.trim())));
    }
    // Facet tags are repeatable and ANDed server-side (?tag=brand:fendt&tag=region:europe).
    for t in tags.iter().filter(|s| !s.trim().is_empty()) {
        url.push_str(&format!("&tag={}", urlencode(t.trim())));
    }
    // Period-correct filter: keep only mods whose model existed by this year (year_from ≤ Y).
    if let Some(y) = available_by {
        url.push_str(&format!("&availableBy={y}"));
    }

    let fetch = |u: &str| -> Result<BrowsePage, String> {
        let resp = get(u)?;
        let parsed: BrowseResponse = resp.into_json().map_err(|e| e.to_string())?;
        Ok(BrowsePage {
            mods: parsed.mods,
            total: parsed.total,
        })
    };

    // A server predating a given sort value (e.g. `popular` before the enrichment
    // backfill ships) rejects it with an error body rather than a page. Rather than
    // surface a dead Browse, fall back to the server's default ordering — the new sort
    // lights up automatically once the API learns the value.
    match sort.filter(|s| !s.trim().is_empty()) {
        Some(s) => {
            let sorted = format!("{}&sort={}", url, urlencode(s.trim()));
            fetch(&sorted).or_else(|_| fetch(&url))
        }
        None => fetch(&url),
    }
}

/// SiloAPI's `POST /mods/lookup` accepts at most this many tech names per request.
pub const LOOKUP_BATCH: usize = 500;

/// Batch version lookup by tech name. Libraries bigger than [`LOOKUP_BATCH`] go out in several
/// requests (the API refuses larger bodies with 400); the answers are concatenated in order.
pub fn lookup(base: &str, tech_names: &[String]) -> Result<Vec<LookupResult>, String> {
    let mut out = Vec::new();
    for batch in lookup_batches(tech_names) {
        out.extend(lookup_one(base, batch)?);
    }
    Ok(out)
}

/// Split the names into request-sized batches (none when there are no names).
fn lookup_batches(tech_names: &[String]) -> std::slice::Chunks<'_, String> {
    tech_names.chunks(LOOKUP_BATCH)
}

fn lookup_one(base: &str, tech_names: &[String]) -> Result<Vec<LookupResult>, String> {
    let resp = crate::net::api()
        .post(&format!("{}/mods/lookup", base.trim_end_matches('/')))
        .set("Content-Type", "application/json")
        .send_json(ureq::json!({ "techNames": tech_names }))
        .map_err(|e| match e {
            ureq::Error::Status(code, _) => format!("SiloAPI lookup returned {code}"),
            other => format!("Could not reach SiloAPI: {other}"),
        })?;
    let parsed: LookupResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(parsed.results)
}

/// Catalog categories with counts (whole catalog, most-populated first).
pub fn categories(base: &str) -> Result<Vec<CategoryCount>, String> {
    let resp = get(&format!("{}/categories", base.trim_end_matches('/')))?;
    let parsed: CategoriesResponse = resp.into_json().map_err(|e| e.to_string())?;
    Ok(parsed.categories)
}

/// Catalog counts. The API returns counts as strings (bigint) — parse leniently.
pub fn stats(base: &str) -> Result<Stats, String> {
    let resp = get(&format!("{}/stats", base.trim_end_matches('/')))?;
    let v: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    Ok(Stats {
        mods: as_u64(&v["mods"]),
        sources: as_u64(&v["sources"]),
    })
}

/// One mod's full catalog record, including its cross-source pointers.
/// One mod's full catalog record. Returns `BrowseMod` (a superset of the old
/// `ModDetail`) so the drawer gets the aggregate popularity fields + pageUrl the list
/// rows carry — the detail endpoint sends them, and dropping them here is exactly what
/// left the drawer showing "rating not synced" while the grid card had it.
pub fn detail(base: &str, id: &str) -> Result<BrowseMod, String> {
    let resp = get(catalog_url(base, &["mods", id])?.as_str())?;
    resp.into_json().map_err(|e| e.to_string())
}

/// `{base}/<segments...>`, each segment percent-encoded on its own (a `/`, `?` or `#` in a
/// mod id stays inside that one segment). Refuses an empty, `.`/`..`, or control-character
/// segment (see `net::safe_path_segment`). Any path already in `base` is kept.
fn catalog_url(base: &str, segments: &[&str]) -> Result<url::Url, String> {
    if !segments.iter().all(|s| crate::net::safe_path_segment(s)) {
        return Err("That catalog address isn't valid.".to_string());
    }
    let mut u = url::Url::parse(base.trim_end_matches('/'))
        .map_err(|_| format!("Not a valid SiloAPI address: {base}"))?;
    u.path_segments_mut()
        .map_err(|_| format!("Not a valid SiloAPI address: {base}"))?
        .pop_if_empty()
        .extend(segments);
    Ok(u)
}

/// `{base}/mods/{id}/manifest?version=<v>`.
fn manifest_url(base: &str, id: &str, version: &str) -> Result<url::Url, String> {
    let mut u = catalog_url(base, &["mods", id, "manifest"])?;
    u.set_query(Some(&format!("version={}", urlencode(version))));
    Ok(u)
}

/// One entry in a canonical content manifest.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntryDto {
    pub path: String,
    pub sha256: String,
    #[serde(default)]
    pub size: Option<i64>,
}

/// The canonical manifest for one (mod, version) — `GET /mods/:id/manifest?version=<v>`.
/// The server does version-equivalence matching (ModHub `1.2.0` ↔ GitHub `v1.2.0`), so the
/// client sends its local modDesc version verbatim.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalManifest {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub archive_sha256: Option<String>,
    pub manifest_hash: String,
    #[serde(default)]
    pub entries: Vec<ManifestEntryDto>,
}

/// Fetch the canonical manifest for a specific version. `Ok(None)` when the server has no
/// hashed build for that version (404) — that's an "unverified", not an error.
pub fn manifest(base: &str, id: &str, version: &str) -> Result<Option<CanonicalManifest>, String> {
    let url = manifest_url(base, id, version)?;
    match crate::net::api().get(url.as_str()).call() {
        Ok(resp) => resp.into_json().map(Some).map_err(|e| e.to_string()),
        Err(ureq::Error::Status(404, _)) => Ok(None),
        Err(ureq::Error::Status(code, _)) => Err(format!("SiloAPI manifest returned {code}")),
        Err(other) => Err(format!("Could not reach SiloAPI: {other}")),
    }
}

/// Resolve a download into (url, filename). `want` picks a specific source (the button
/// the user clicked); without it, the API's best-ranked installable source wins.
///
/// The API decides what's installable and strips URLs it knows won't serve us, so a
/// source with no url here is one the user must fetch from its own site.
/// A resolved installable download plus the identity we can hold it to: `expected_sha256` is
/// the catalog's canonical whole-zip hash for the exact version being fetched, when it has one
/// (GitHub-source, provenance P1). `None` means "not hashed yet" (ModHub/unhashed) — we
/// can't prove identity, only that it's a valid archive.
pub struct ResolvedDownload {
    pub url: String,
    pub filename: String,
    pub expected_sha256: Option<String>,
}

pub fn resolve_download(
    base: &str,
    id: &str,
    want: Option<&str>,
) -> Result<ResolvedDownload, String> {
    let detail = detail(base, id)?;

    let usable = |s: &&ModSource| s.installable && s.download_url.is_some();
    let pick =
        match want {
            Some(name) => detail
                .sources
                .iter()
                .find(|s| s.source == name)
                .ok_or_else(|| format!("{name} doesn't list this mod"))
                .and_then(|s| {
                    if usable(&s) {
                        Ok(s)
                    } else {
                        Err(format!(
                            "{name} doesn't allow direct downloads — open its page instead"
                        ))
                    }
                })?,
            None => detail.sources.iter().find(usable).ok_or_else(|| {
                "No source allows a direct download — open the mod page".to_string()
            })?,
        };

    let url = pick
        .download_url
        .clone()
        .ok_or_else(|| "That source has no download URL".to_string())?;
    let filename = filename_from_url(&url)
        .ok_or_else(|| "Could not derive a .zip filename from the download URL".to_string())?;

    // Identity: the canonical whole-zip hash for the *version we're about to fetch*. Use the
    // /manifest endpoint (server does version-equivalence). A network/404 miss → None → the
    // download is still validated as an archive, just not identity-checked.
    let version = pick
        .version
        .clone()
        .or_else(|| detail.latest_version.clone())
        .unwrap_or_default();
    let expected_sha256 = manifest(base, id, &version)
        .ok()
        .flatten()
        .and_then(|m| m.archive_sha256);

    Ok(ResolvedDownload {
        url,
        filename,
        expected_sha256,
    })
}

/// Last path segment if it looks like a .zip.
fn filename_from_url(url: &str) -> Option<String> {
    let clean = url.split(['?', '#']).next().unwrap_or(url);
    let name = clean.rsplit('/').next().unwrap_or("");
    if name.to_lowercase().ends_with(".zip") && name.len() > 4 {
        Some(name.to_string())
    } else {
        None
    }
}

fn as_u64(v: &serde_json::Value) -> u64 {
    v.as_u64()
        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        .unwrap_or(0)
}

/// Minimal percent-encoding for query values (spaces, &, etc.).
fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Download a resolved .zip to the temp file `part`, streaming so the caller can report
/// progress. Validates PK magic, caps at 500 MB, and checks the whole archive (central
/// directory, `modDesc.xml`, and the catalog's SHA-256 when known). On any failure `part` is
/// removed, so a failed download never leaves a half-written file; on success the caller
/// places `part` (it needs no library lock until then). `on_progress` is called with
/// (bytes_done, total_bytes) — total is None when the server omits Content-Length. Kept
/// Tauri-agnostic; the command layer turns it into events.
pub fn download_to_part<F: Fn(u64, Option<u64>)>(
    url: &str,
    part: &std::path::Path,
    expected_sha256: Option<&str>,
    on_progress: F,
) -> Result<(), String> {
    let result = fetch_and_validate(url, part, expected_sha256, &on_progress);
    if result.is_err() {
        let _ = std::fs::remove_file(part);
    }
    result
}

fn fetch_and_validate(
    url: &str,
    part: &std::path::Path,
    expected_sha256: Option<&str>,
    on_progress: &dyn Fn(u64, Option<u64>),
) -> Result<(), String> {
    const CAP: u64 = 500 * 1024 * 1024;
    // SSRF guard: the URL is catalog/frontend-influenced. Downloads must be public https —
    // never a private/loopback host (no dev localhost exception for a mod download).
    crate::net::validate_outbound_url(url, false)?;
    let resp = crate::net::download()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?;
    let total: Option<u64> = resp
        .header("Content-Length")
        .and_then(|s| s.parse().ok())
        .filter(|&t| t <= CAP);

    // Errors return early; `download_to_part` removes the part (the handle is closed by then).
    let mut file = std::fs::File::create(part).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();

    let mut buf = [0u8; 64 * 1024];
    let mut head: Vec<u8> = Vec::with_capacity(2);
    let mut done: u64 = 0;
    let mut last_emit: u64 = 0;
    on_progress(0, total);

    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        // Validate the zip's PK magic from the first two bytes.
        if head.len() < 2 {
            let need = (2 - head.len()).min(n);
            head.extend_from_slice(&buf[..need]);
            if head.len() == 2 && &head[..] != b"PK" {
                return Err("Downloaded file is not a valid .zip".to_string());
            }
        }
        done += n as u64;
        if done > CAP {
            return Err("Download exceeds the 500 MB safety cap".to_string());
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        // Throttle progress events to ~1 MB steps to avoid flooding the UI.
        if done - last_emit >= 1024 * 1024 {
            last_emit = done;
            on_progress(done, total);
        }
    }
    if head.len() < 2 {
        return Err("Downloaded file is not a valid .zip".to_string());
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);

    // Full-archive validation before we admit it to the library. The PK magic above only
    // proves the first two bytes; a truncated or garbage download can still start with "PK".
    // Require the central directory to parse AND a modDesc.xml to exist (every real FS mod
    // has one) — this is what stops "Silo installed a broken mod" reports.
    {
        let f = std::fs::File::open(part).map_err(|e| e.to_string())?;
        let mut ar = zip::ZipArchive::new(f)
            .map_err(|_| "Downloaded file isn't a valid .zip (corrupt or truncated)".to_string())?;
        ar.by_name("modDesc.xml")
            .map_err(|_| "Downloaded archive has no modDesc.xml — not an FS25 mod".to_string())?;
    }
    // Identity: a valid ZIP of the WRONG mod is still the wrong mod. If the catalog has a
    // canonical hash for this exact mod+version, the bytes must match it byte-for-byte —
    // this catches a mismatched/tampered asset before it enters the library.
    if let Some(expected) = expected_sha256 {
        let got = crate::provenance::sha256_file(part)?;
        if !got.eq_ignore_ascii_case(expected) {
            return Err(
                "Downloaded file doesn't match the catalog's known build for this mod \
                 (wrong or tampered file) — refusing it."
                    .to_string(),
            );
        }
    }
    on_progress(done, total);
    Ok(())
}

/// The only hosts we'll proxy an image from: GitHub's content CDN. Also an SSRF guard — a
/// hostile catalog `imageUrl` can't make us fetch an arbitrary internal URL. The host is
/// parsed with the WHATWG URL parser, so tricks like `https://169.254.169.254\.x.com/`
/// resolve to their real host and are refused. GIANTS' ModHub CDN is deliberately absent:
/// it refuses requests that don't come from its own site, and Silo doesn't work around
/// that — ModHub thumbnails come from SiloAPI's cache or fall back to a placeholder.
pub(crate) fn is_catalog_image_host(url: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    if parsed.scheme() != "https" {
        return false;
    }
    match parsed.host_str() {
        Some(host) => host
            .to_ascii_lowercase()
            .ends_with(".githubusercontent.com"),
        None => false,
    }
}

/// Fetch a catalog mod image in Rust and return it as a `data:` URL the webview can render
/// directly (the CSP keeps the webview from loading arbitrary image hosts). Sends Silo's
/// own User-Agent and no Referer. Host-allowlisted and size-capped.
pub fn fetch_image(url: &str) -> Result<String, String> {
    if !is_catalog_image_host(url) {
        return Err("not an allowed image host".to_string());
    }
    let resp = crate::net::api()
        .get(url)
        .call()
        .map_err(|e| e.to_string())?;
    let mime = resp
        .header("Content-Type")
        .filter(|m| m.starts_with("image/"))
        .unwrap_or("image/jpeg")
        .to_string();
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(16 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mod_ids_stay_in_one_path_segment() {
        let u = catalog_url("https://silo-api.hllmr.com", &["mods", "a/b?c#d"]).unwrap();
        assert_eq!(u.as_str(), "https://silo-api.hllmr.com/mods/a%2Fb%3Fc%23d");
        assert_eq!(u.host_str(), Some("silo-api.hllmr.com"));
        let u = catalog_url("https://h.example/api/", &["mods", "x", "manifest"]).unwrap();
        assert_eq!(u.as_str(), "https://h.example/api/mods/x/manifest");
        assert!(catalog_url("https://h.example", &["mods", ".\t."]).is_err());
        assert!(catalog_url("https://h.example", &["mods", ".."]).is_err());
        assert!(catalog_url("https://h.example", &["mods", ""]).is_err());
        let m = manifest_url("https://h.example", "a/b?x", "1.0 beta&y=1").unwrap();
        assert_eq!(
            m.as_str(),
            "https://h.example/mods/a%2Fb%3Fx/manifest?version=1.0%20beta%26y%3D1"
        );
    }

    #[test]
    fn verified_author_passes_through_when_claimed() {
        let base = r#"{"id":"m1","title":"Lizard","sources":[]}"#;
        let m: BrowseMod = serde_json::from_str(base).unwrap();
        assert!(
            m.verified_author.is_none(),
            "an API without claims still parses"
        );

        let claimed = r#"{"id":"m1","title":"Lizard","sources":[],
            "verifiedAuthor":{"login":"octocat","avatarUrl":null,"profileUrl":"https://x/authors/octocat/"}}"#;
        let m: BrowseMod = serde_json::from_str(claimed).unwrap();
        let a = m.verified_author.as_ref().unwrap();
        assert_eq!(a.login, "octocat");
        assert_eq!(a.avatar_url, None);
        // Re-serialized for the webview in camelCase, as Browse reads it.
        let out = serde_json::to_value(&m).unwrap();
        assert_eq!(out["verifiedAuthor"]["login"], "octocat");

        let unclaimed = r#"{"id":"m1","title":"Lizard","sources":[],"verifiedAuthor":null}"#;
        let m: BrowseMod = serde_json::from_str(unclaimed).unwrap();
        assert!(m.verified_author.is_none());

        // A malformed claim is no claim, never a failed page.
        for bad in [
            r#"{}"#,
            r#"{"login":null}"#,
            r#"{"login":"  "}"#,
            r#""octocat""#,
            r#"[1]"#,
        ] {
            let json =
                format!(r#"{{"id":"m1","title":"Lizard","sources":[],"verifiedAuthor":{bad}}}"#);
            let m: BrowseMod = serde_json::from_str(&json).unwrap();
            assert!(m.verified_author.is_none(), "{bad}");
        }
    }

    #[test]
    fn filename_parsing() {
        assert_eq!(
            filename_from_url(
                "https://cdn27.giants-software.com/modHub/storage/00360212/FS25_Schmiechtal.zip"
            ),
            Some("FS25_Schmiechtal.zip".to_string())
        );
        assert_eq!(
            filename_from_url("https://github.com/x/y/releases/download/v1/FS25_Foo.zip?raw=1"),
            Some("FS25_Foo.zip".to_string())
        );
        assert_eq!(filename_from_url("https://example.com/notazip"), None);
        assert_eq!(filename_from_url("https://example.com/.zip"), None);
    }

    #[test]
    fn image_proxy_only_allows_github_content() {
        assert!(is_catalog_image_host(
            "https://raw.githubusercontent.com/o/r/main/icon.png"
        ));
        assert!(is_catalog_image_host(
            "https://user-images.githubusercontent.com/1/2.png"
        ));
        // GIANTS' CDN refuses off-site requests; Silo no longer proxies it.
        assert!(!is_catalog_image_host(
            "https://cdn40.giants-software.com/modHub/storage/00303729/iconBig.jpg"
        ));
        // Plain http, look-alike hosts and the backslash trick are refused.
        assert!(!is_catalog_image_host(
            "http://raw.githubusercontent.com/o/r/main/icon.png"
        ));
        assert!(!is_catalog_image_host(
            "https://githubusercontent.com.evil.example/x.png"
        ));
        assert!(!is_catalog_image_host(
            "https://169.254.169.254\\.githubusercontent.com/latest/meta-data"
        ));
        assert!(!is_catalog_image_host("not a url"));
    }

    #[test]
    fn urlencoding() {
        assert_eq!(urlencode("auto load"), "auto%20load");
        assert_eq!(urlencode("a&b=c"), "a%26b%3Dc");
        assert_eq!(urlencode("FS25_Mod-1.0"), "FS25_Mod-1.0");
    }

    #[test]
    fn lenient_count_parsing() {
        assert_eq!(as_u64(&serde_json::json!("215")), 215);
        assert_eq!(as_u64(&serde_json::json!(199)), 199);
        assert_eq!(as_u64(&serde_json::json!(null)), 0);
    }
}

#[cfg(test)]
mod lookup_batch_tests {
    use super::*;

    fn names(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("FS25_mod{i}")).collect()
    }

    #[test]
    fn lookups_are_split_into_api_sized_batches() {
        assert_eq!(lookup_batches(&names(0)).count(), 0);
        assert_eq!(lookup_batches(&names(1)).count(), 1);
        assert_eq!(lookup_batches(&names(LOOKUP_BATCH)).count(), 1);
        let big = names(739);
        let sizes: Vec<usize> = lookup_batches(&big).map(|b| b.len()).collect();
        assert_eq!(sizes, vec![500, 239]);
        assert!(lookup_batches(&big).all(|b| b.len() <= LOOKUP_BATCH));
    }
}
