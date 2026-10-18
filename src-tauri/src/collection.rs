//! Collections — a portable, shareable *list* of mods (never the ZIPs themselves).
//!
//! A collection is a superset of a loadout (`LoadoutFile`): where a loadout is just
//! a name + a set of tech-names for local use, a collection carries enough per-mod
//! detail (source, canonical provenance hash, version) that a recipient can resolve,
//! fetch, and verify each mod through Silo's existing catalog → download → verify
//! pipeline. Phase 1 shares it through the user's own GitHub as a secret gist; this
//! module is only the on-the-wire format + the small pure helpers around it (schema
//! validation, gist-URL parsing). The export/import commands live elsewhere.
//!
//! Two hash spaces coexist deliberately (see `mpsync` and `provenance`): a collection
//! carries the canonical `manifestHash` (cross-source, order-independent) as its trust
//! field, so a recipient can fetch a *verified* build from any source — not only
//! byte-match the curator's exact zip. mpsync's whole-zip MD5 stays for the separate
//! host↔joiner multiplayer diff.

use serde::{Deserialize, Serialize};

/// The current wire schema. Bumped on a breaking format change; readers accept any
/// document under the `silo.collection/` family (forward-lenient — unknown-but-parseable
/// minor versions still load, since every field beyond the required three is optional).
pub const SCHEMA: &str = "silo.collection/1";
const SCHEMA_PREFIX: &str = "silo.collection/";

/// The canonical file name a collection is stored under inside a gist (or, later, a repo).
pub const FILE_NAME: &str = "silo-collection.json";

/// A shareable collection. Distinct from `LoadoutFile` (which stays the local,
/// bare-tech-name `.silo` interchange) — a collection down-imports to a loadout by
/// dropping everything but the tech-names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Collection {
    /// Format tag, e.g. `"silo.collection/1"`. Validated on read.
    pub schema: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Curator's GitHub login, stamped at export. (Distinct from any per-mod author.)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// RFC 3339, client-stamped at export.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// Optional savegame/map pin (for MP or a save-bound share).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub savegame: Option<SaveBinding>,
    pub mods: Vec<CollectionMod>,
}

/// One mod in a collection. Only `tech_name` is required; the rest is populated from
/// the catalog record + provenance at export time, and is what makes the import
/// fix-list *actionable* (resolve a source, verify against a hash) rather than a bare
/// name list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMod {
    pub tech_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// Preferred source: `"github" | "modhub"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Page/release URL — used for the open-page branch when a source can't be installed directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
    /// Canonical provenance hash (cross-source), when known — the trust field on import.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_hash: Option<String>,
    /// Whether a direct in-app install is possible; `false`/absent → the importer opens the page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installable: Option<bool>,
    /// SiloAPI mod id, to skip a lookup on resolve.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_id: Option<String>,
}

/// Optional savegame binding. Enough to re-associate without pinning a local path.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveBinding {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_title: Option<String>,
}

impl Collection {
    /// The tech-names in this collection, in order — the down-import to a plain loadout.
    pub fn tech_names(&self) -> Vec<String> {
        self.mods.iter().map(|m| m.tech_name.clone()).collect()
    }
}

/// Serialize a collection to the pretty JSON we write to a gist / file.
pub fn to_json(c: &Collection) -> Result<String, String> {
    serde_json::to_string_pretty(c).map_err(|e| e.to_string())
}

/// Parse + validate an untrusted collection document. Rejects anything that isn't a
/// Silo collection (wrong/missing schema or malformed JSON) with one user-facing message.
pub fn parse(json: &str) -> Result<Collection, String> {
    let c: Collection =
        serde_json::from_str(json).map_err(|_| "That isn't a valid Silo shared set".to_string())?;
    if !c.schema.starts_with(SCHEMA_PREFIX) {
        return Err("That isn't a valid Silo shared set".to_string());
    }
    Ok(c)
}

/// A `.silo` mod set file: a name and bare tech names (the format `export_set_file` writes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetFile {
    pub silo: u32,
    pub name: String,
    pub mods: Vec<String>,
}

/// Read a shared mod set file: a collection document, or a `.silo` set file (which becomes a
/// collection with bare tech names, so it previews and installs through the same path).
pub fn parse_set_file(json: &str) -> Result<Collection, String> {
    const NOT_A_SET: &str = "That file isn't a Silo mod set";
    if let Ok(c) = parse(json) {
        return Ok(c);
    }
    let f: SetFile = serde_json::from_str(json).map_err(|_| NOT_A_SET.to_string())?;
    let name = f.name.trim();
    if name.is_empty() {
        return Err(NOT_A_SET.into());
    }
    let mut seen = std::collections::HashSet::new();
    let mods = f
        .mods
        .into_iter()
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty() && seen.insert(t.clone()))
        .map(|tech_name| CollectionMod {
            tech_name,
            version: None,
            source: None,
            source_url: None,
            manifest_hash: None,
            installable: None,
            catalog_id: None,
        })
        .collect();
    Ok(Collection {
        schema: SCHEMA.to_string(),
        name: name.to_string(),
        description: None,
        author: None,
        created_at: None,
        savegame: None,
        mods,
    })
}

/// What the catalog lookup said about one techName.
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogMatch {
    pub id: String,
    /// The source the catalog can download directly, if any.
    pub download_source: Option<String>,
}

/// One collection mod resolved for preview and apply alike.
#[derive(Debug, Clone, PartialEq)]
pub struct Target {
    pub tech_name: String,
    /// Catalog entry found by techName. A collection's own `catalog_id` is never used.
    pub catalog_id: Option<String>,
    /// Set when Silo can install it: the source apply downloads from.
    pub download_source: Option<String>,
    /// What the preview shows as the source (the download source, else the collection's hint).
    pub display_source: Option<String>,
}

/// Resolve every mod by its techName, exactly as the import preview shows it, so a shared file
/// can't preview one mod and install another.
pub fn resolve_targets(
    mods: &[CollectionMod],
    hits: &std::collections::HashMap<String, CatalogMatch>,
) -> Vec<Target> {
    mods.iter()
        .map(|m| {
            let hit = hits.get(&m.tech_name);
            let download_source = hit.and_then(|h| h.download_source.clone());
            Target {
                tech_name: m.tech_name.clone(),
                catalog_id: hit.map(|h| h.id.clone()),
                display_source: download_source.clone().or_else(|| m.source.clone()),
                download_source,
            }
        })
        .collect()
}

/// SHA-256 (hex) of the collection file's exact text plus what the catalog resolved for each
/// mod (in the file's order): apply refuses if either changed after the preview.
pub fn fingerprint(json: &str, targets: &[Target]) -> String {
    use sha2::{Digest, Sha256};
    // Every field is length-prefixed (and an absent one marked), so no two different inputs
    // can run together into the same bytes.
    fn field(h: &mut Sha256, s: Option<&str>) {
        match s {
            None => h.update([0u8]),
            Some(s) => {
                h.update([1u8]);
                h.update((s.len() as u64).to_le_bytes());
                h.update(s.as_bytes());
            }
        }
    }
    let mut h = Sha256::new();
    field(&mut h, Some(json));
    h.update((targets.len() as u64).to_le_bytes());
    for t in targets {
        field(&mut h, Some(&t.tech_name));
        field(&mut h, t.catalog_id.as_deref());
        field(&mut h, t.download_source.as_deref());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// A GitHub gist id is an opaque hex string (see `github::valid_gist_id`, the one rule).
fn is_gist_id(s: &str) -> bool {
    crate::github::valid_gist_id(s)
}

/// Pull a gist id out of whatever the user pasted: a bare id, a
/// `gist.github.com/{user}/{id}` or `/{id}` link, a `gist.githubusercontent.com` raw
/// URL, or an `api.github.com/gists/{id}` URL. Returns `None` if no gist id is present.
pub fn parse_gist_ref(input: &str) -> Option<String> {
    let s = input.trim();
    // Drop any query/fragment (e.g. `#file-silo-collection-json`) and trailing slash.
    let s = s
        .split(['#', '?'])
        .next()
        .unwrap_or(s)
        .trim_end_matches('/');

    // A bare id needs no host.
    if is_gist_id(s) {
        return Some(s.to_string());
    }

    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let mut it = rest.splitn(2, '/');
    let host = it.next()?.to_ascii_lowercase();
    let path = it.next().unwrap_or("");
    let segs: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    let candidate = match host.as_str() {
        // /gists/{id}
        "api.github.com" => segs
            .iter()
            .position(|&p| p == "gists")
            .and_then(|i| segs.get(i + 1).copied()),
        // /{id}  or  /{user}/{id}[/...]
        "gist.github.com" => match segs.len() {
            0 => None,
            1 => Some(segs[0]),
            _ => Some(segs[1]),
        },
        // /{user}/{id}/raw/...
        "gist.githubusercontent.com" => segs.get(1).copied(),
        _ => None,
    };

    candidate.filter(|c| is_gist_id(c)).map(|c| c.to_string())
}

/// Where a shared collection lives: a gist (P1, private/unlisted) or a public repo (P2).
#[derive(Debug, Clone, PartialEq)]
pub enum CollectionRef {
    Gist(String),
    Repo { owner: String, repo: String },
}

/// Parse whatever the user pasted into a collection reference — a gist link/id, or a
/// GitHub repo URL / bare `owner/repo`. Gist forms win first (they're unambiguous); a
/// repo is anything that resolves to a `github.com/{owner}/{repo}`.
pub fn parse_collection_ref(input: &str) -> Option<CollectionRef> {
    if let Some(r) = parse_page_ref(input) {
        return Some(r);
    }
    if let Some(id) = parse_gist_ref(input) {
        return Some(CollectionRef::Gist(id));
    }
    parse_repo_ref(input).map(|(owner, repo)| CollectionRef::Repo { owner, repo })
}

/// The handoff page link Silo hands out (`https://silo.hllmr.com/c/?g=<gist id>` or
/// `?r=<owner>/<repo>`, see `gist_page_url` / `repo_page_url`), as a friend would paste it.
fn parse_page_ref(input: &str) -> Option<CollectionRef> {
    let s = input.trim();
    let rest = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(s);
    let rest = rest.strip_prefix("www.").unwrap_or(rest);
    let (host, path) = rest.split_once('/')?;
    if !host.eq_ignore_ascii_case("silo.hllmr.com") {
        return None;
    }
    let path = path.split('#').next().unwrap_or(path);
    let (page, query) = path.split_once('?')?;
    if page.trim_end_matches('/') != "c" {
        return None;
    }
    for pair in query.split('&') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        let v = v.replace("%2F", "/").replace("%2f", "/");
        let v = v.as_str();
        match k {
            "g" if is_gist_id(v) => return Some(CollectionRef::Gist(v.to_string())),
            "r" => {
                let (owner, repo) = v.split_once('/')?;
                if is_gh_owner(owner) && is_gh_repo(repo) {
                    return Some(CollectionRef::Repo {
                        owner: owner.to_string(),
                        repo: repo.to_string(),
                    });
                }
                return None;
            }
            _ => {}
        }
    }
    None
}

/// GitHub usernames/orgs: alphanumeric + hyphen, ≤39 chars (no dots — that's how we
/// reject a non-github host masquerading as an owner in a bare `host/path`).
/// Delegates to `github::valid_owner`, the one rule.
fn is_gh_owner(s: &str) -> bool {
    crate::github::valid_owner(s)
}

/// GitHub repo names: alphanumeric + `-` `_` `.`, ≤100 chars, not `.`/`..`.
/// Delegates to `github::valid_repo`, the one rule.
fn is_gh_repo(s: &str) -> bool {
    crate::github::valid_repo(s)
}

/// Extract `(owner, repo)` from a `github.com/{owner}/{repo}` URL (with optional
/// `/tree/…`, `.git`, trailing slash, query/fragment) or a bare `owner/repo`. Rejects any
/// non-github host.
fn parse_repo_ref(input: &str) -> Option<(String, String)> {
    let s = input.trim();
    let s = s
        .split(['#', '?'])
        .next()
        .unwrap_or(s)
        .trim_end_matches('/');

    let rest = if let Some(r) = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
    {
        // Had a scheme → the host must be github.com (else reject via `?`).
        r.strip_prefix("www.")
            .unwrap_or(r)
            .strip_prefix("github.com/")?
    } else if let Some(r) = s
        .strip_prefix("github.com/")
        .or_else(|| s.strip_prefix("www.github.com/"))
    {
        r
    } else if s.contains("://") {
        return None; // some other scheme
    } else {
        s // bare owner/repo
    };

    let mut segs = rest.split('/').filter(|p| !p.is_empty());
    let owner = segs.next()?;
    let repo = segs.next()?.trim_end_matches(".git");
    (is_gh_owner(owner) && is_gh_repo(repo)).then(|| (owner.to_string(), repo.to_string()))
}

/// A GitHub repo name for a public collection: `silo-` + a slug of the collection name
/// (lowercase, runs of non-alphanumerics collapsed to a single hyphen). The prefix groups
/// a user's collections on their account and marks them for humans + eventual discovery.
pub fn repo_slug(name: &str) -> String {
    let mut slug = String::new();
    let mut dash = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            dash = false;
        } else if !slug.is_empty() && !dash {
            slug.push('-');
            dash = true;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "silo-collection".to_string()
    } else {
        format!("silo-{slug}")
    }
}

/// The public handoff page. Recipients open `silo.hllmr.com/c/…`, which fetches the list and
/// hands off to the Silo app (or offers a download) — the human-friendly link a sharer sends,
/// instead of a raw gist/repo URL that dead-ends on a wall of JSON.
pub const PAGE_BASE: &str = "https://silo.hllmr.com/c/";

/// Handoff-page URL for a gist-backed collection (`?g=<id>`).
pub fn gist_page_url(id: &str) -> String {
    format!("{PAGE_BASE}?g={id}")
}

/// Handoff-page URL for a repo-backed collection (`?r=<owner>/<repo>`).
pub fn repo_page_url(owner: &str, repo: &str) -> String {
    format!("{PAGE_BASE}?r={owner}/{repo}")
}

/// A human-readable README for a shared collection (repo or gist): what it is, how to open
/// it in Silo, and the mod list with source links. Lists mods only — no files redistributed.
///
/// `page_url` is the `silo.hllmr.com/c/…` handoff page (an https link GitHub renders as
/// clickable — a `silo://` link would be stripped by GitHub's markdown sanitizer). `source_url`
/// is the raw gist/repo link, for pasting straight into Silo's importer.
pub fn readme(c: &Collection, page_url: &str, source_url: &str) -> String {
    let mut s = format!("# {}\n\n", c.name);
    if let Some(d) = c.description.as_deref().filter(|d| !d.trim().is_empty()) {
        s.push_str(d);
        s.push_str("\n\n");
    }
    s.push_str(
        "A Farming Simulator 25 mod set, shareable with [Silo](https://silo.hllmr.com).\n\n",
    );
    if let Some(a) = c.author.as_deref().filter(|a| !a.trim().is_empty()) {
        s.push_str(&format!("Curated by **{a}**.\n\n"));
    }
    s.push_str("## Open in Silo\n\n");
    s.push_str(&format!(
        "**[▶ Open this mod set in Silo]({page_url})**\n\n"
    ));
    s.push_str("That page hands off to the Silo desktop app. Install Silo first if you don't have it. You can also open **Mod sets → Open a shared set** in Silo and paste:\n\n");
    s.push_str(&format!("```\n{source_url}\n```\n\n"));
    s.push_str(&format!("## Mods ({})\n\n", c.mods.len()));
    for m in &c.mods {
        let ver = m
            .version
            .as_deref()
            .map(|v| format!(" `{v}`"))
            .unwrap_or_default();
        let src = match (m.source.as_deref(), m.source_url.as_deref()) {
            (Some(src), Some(url)) => format!(" — [{src}]({url})"),
            (Some(src), None) => format!(" — {src}"),
            _ => String::new(),
        };
        s.push_str(&format!("- **{}**{ver}{src}\n", m.tech_name));
    }
    s.push_str(
        "\n_Generated by Silo. This is a list of mods and where to get them. No mod files are redistributed._\n",
    );
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cm(tech: &str, catalog_id: Option<&str>, source: Option<&str>) -> CollectionMod {
        CollectionMod {
            tech_name: tech.into(),
            version: None,
            source: source.map(Into::into),
            source_url: None,
            manifest_hash: None,
            installable: None,
            catalog_id: catalog_id.map(Into::into),
        }
    }

    fn hit(id: &str, download_source: Option<&str>) -> CatalogMatch {
        CatalogMatch {
            id: id.into(),
            download_source: download_source.map(Into::into),
        }
    }

    #[test]
    fn the_tech_name_lookup_decides_what_installs() {
        let mods = vec![
            cm("FS25_A", Some("cat-B"), None), // carried id disagrees with the lookup
            cm("FS25_C", Some("cat-C"), None), // agrees
            cm("FS25_D", None, Some("modhub")), // catalogued, no direct download
            cm("FS25_X", Some("cat-evil"), None), // unknown to the catalog
        ];
        let hits = std::collections::HashMap::from([
            ("FS25_A".to_string(), hit("cat-A", Some("github"))),
            ("FS25_C".to_string(), hit("cat-C", Some("github"))),
            ("FS25_D".to_string(), hit("cat-D", None)),
        ]);
        let t = resolve_targets(&mods, &hits);
        assert_eq!(
            t[0].catalog_id.as_deref(),
            Some("cat-A"),
            "lookup wins over the carried id"
        );
        assert_eq!(t[0].download_source.as_deref(), Some("github"));
        assert_eq!(t[1].catalog_id.as_deref(), Some("cat-C"));
        assert!(t[2].catalog_id.is_some() && t[2].download_source.is_none());
        assert_eq!(t[2].display_source.as_deref(), Some("modhub"));
        assert_eq!(t[3].catalog_id, None, "a carried id alone never installs");
        assert_eq!(t[3].download_source, None);
    }

    #[test]
    fn the_fingerprint_follows_the_exact_text() {
        assert_eq!(fingerprint("{\"a\":1}", &[]), fingerprint("{\"a\":1}", &[]));
        assert_ne!(fingerprint("{\"a\":1}", &[]), fingerprint("{\"a\":2}", &[]));
        assert_eq!(fingerprint("x", &[]).len(), 64);
    }

    #[test]
    fn the_fingerprint_follows_what_the_catalog_resolved() {
        let mods = vec![cm("FS25_A", None, None), cm("FS25_B", None, None)];
        let resolve = |a: CatalogMatch| {
            let hits = std::collections::HashMap::from([
                ("FS25_A".to_string(), a),
                ("FS25_B".to_string(), hit("cat-B", None)),
            ]);
            resolve_targets(&mods, &hits)
        };
        let base = resolve(hit("cat-A", Some("github")));
        let json = "{\"a\":1}";
        assert_eq!(fingerprint(json, &base), fingerprint(json, &base.clone()));
        assert_ne!(
            fingerprint(json, &base),
            fingerprint(json, &resolve(hit("cat-OTHER", Some("github")))),
            "another catalog entry"
        );
        assert_ne!(
            fingerprint(json, &base),
            fingerprint(json, &resolve(hit("cat-A", None))),
            "no longer directly downloadable"
        );
        assert_ne!(
            fingerprint(json, &base),
            fingerprint(json, &resolve_targets(&mods, &Default::default())),
            "not in the catalog any more"
        );
        assert_ne!(fingerprint(json, &base), fingerprint(json, &[]));
    }

    fn sample() -> Collection {
        Collection {
            schema: SCHEMA.to_string(),
            name: "My Server Pack".to_string(),
            description: Some("Weekend co-op set".to_string()),
            author: Some("hllmr".to_string()),
            created_at: Some("2026-08-01T00:00:00Z".to_string()),
            savegame: Some(SaveBinding {
                name: "Co-op Farm".to_string(),
                map_title: Some("Riverbend".to_string()),
            }),
            mods: vec![
                CollectionMod {
                    tech_name: "FS25_Foo".to_string(),
                    version: Some("1.2.0".to_string()),
                    source: Some("github".to_string()),
                    source_url: Some("https://github.com/x/FS25_Foo".to_string()),
                    manifest_hash: Some("a3944f99".to_string()),
                    installable: Some(true),
                    catalog_id: Some("42".to_string()),
                },
                CollectionMod {
                    tech_name: "FS25_Bar".to_string(),
                    version: None,
                    source: Some("modhub".to_string()),
                    source_url: Some("https://www.farming-simulator.com/mod.php?id=1".to_string()),
                    manifest_hash: None,
                    installable: Some(false),
                    catalog_id: None,
                },
            ],
        }
    }

    #[test]
    fn round_trips_through_json() {
        let c = sample();
        let json = to_json(&c).unwrap();
        let back = parse(&json).unwrap();
        assert_eq!(c, back);
    }

    #[test]
    fn omits_absent_optional_fields() {
        // A minimal doc (only the required three) must serialize without null noise
        // and parse back with the optionals as None.
        let c = Collection {
            schema: SCHEMA.to_string(),
            name: "Bare".to_string(),
            description: None,
            author: None,
            created_at: None,
            savegame: None,
            mods: vec![CollectionMod {
                tech_name: "FS25_Only".to_string(),
                version: None,
                source: None,
                source_url: None,
                manifest_hash: None,
                installable: None,
                catalog_id: None,
            }],
        };
        let json = to_json(&c).unwrap();
        assert!(
            !json.contains("null"),
            "optional fields should be omitted, not null"
        );
        assert!(!json.contains("description"));
        assert_eq!(parse(&json).unwrap(), c);
    }

    #[test]
    fn down_imports_to_tech_names() {
        assert_eq!(sample().tech_names(), vec!["FS25_Foo", "FS25_Bar"]);
    }

    #[test]
    fn the_share_page_link_opens_its_gist_or_repo() {
        let id = "0123456789abcdef0123456789abcdef";
        assert_eq!(
            parse_collection_ref(&gist_page_url(id)),
            Some(CollectionRef::Gist(id.to_string()))
        );
        assert_eq!(
            parse_collection_ref(&repo_page_url("hllmr", "silo-my-pack")),
            Some(CollectionRef::Repo {
                owner: "hllmr".into(),
                repo: "silo-my-pack".into()
            })
        );
        assert_eq!(
            parse_collection_ref(&format!("  silo.hllmr.com/c?g={id}#x ")),
            Some(CollectionRef::Gist(id.to_string()))
        );
        // An escaped slash, and a stray pair without "=", still open.
        assert_eq!(
            parse_collection_ref("https://silo.hllmr.com/c/?utm&r=hllmr%2Fsilo-my-pack"),
            Some(CollectionRef::Repo {
                owner: "hllmr".into(),
                repo: "silo-my-pack".into()
            })
        );
        for bad in [
            "https://silo.hllmr.com/c/?g=not-an-id".to_string(),
            "https://silo.hllmr.com/c/?r=../etc".to_string(),
            format!("https://evil.example/c/?g={id}"),
            format!("https://silo.hllmr.com/x/?g={id}"),
            "https://silo.hllmr.com/c/".to_string(),
        ] {
            assert_eq!(parse_page_ref(&bad), None, "{bad}");
        }
    }

    #[test]
    fn a_silo_set_file_reads_as_a_collection_of_bare_names() {
        let c = parse_set_file(
            r#"{ "silo": 1, "name": " Weekend ", "mods": ["FS25_A", " FS25_B ", "FS25_A", ""] }"#,
        )
        .unwrap();
        assert_eq!(c.schema, SCHEMA);
        assert_eq!(c.name, "Weekend");
        assert_eq!(c.tech_names(), ["FS25_A", "FS25_B"]);
        assert!(c.mods.iter().all(|m| m.manifest_hash.is_none()));
    }

    #[test]
    fn a_collection_document_reads_as_itself() {
        let json = r#"{ "schema": "silo.collection/1", "name": "x", "mods": [{ "techName": "FS25_A", "version": "1.0" }] }"#;
        assert_eq!(parse_set_file(json).unwrap(), parse(json).unwrap());
    }

    #[test]
    fn anything_else_is_not_a_mod_set() {
        for bad in [
            "not json",
            r#"{ "silo": 1, "name": "  ", "mods": [] }"#,
            r#"{ "schema": "silo.loadout/1", "name": "x", "mods": [] }"#,
            r#"{ "name": "x", "mods": ["FS25_A"] }"#,
        ] {
            assert_eq!(
                parse_set_file(bad).unwrap_err(),
                "That file isn't a Silo mod set",
                "{bad}"
            );
        }
    }

    #[test]
    fn rejects_non_collection_json() {
        // A loadout file (no schema field) is not a collection.
        let loadout = r#"{ "silo": 1, "name": "x", "mods": ["FS25_A"] }"#;
        assert!(parse(loadout).is_err());
        // Wrong family.
        let wrong = r#"{ "schema": "silo.loadout/1", "name": "x", "mods": [] }"#;
        assert!(parse(wrong).is_err());
        // Garbage.
        assert!(parse("not json").is_err());
    }

    #[test]
    fn accepts_forward_minor_schema() {
        // A future minor under the same family still loads (forward-lenient).
        let doc = r#"{ "schema": "silo.collection/1.1", "name": "x", "mods": [] }"#;
        assert!(parse(doc).is_ok());
    }

    #[test]
    fn parses_gist_ref_from_every_shape() {
        let id = "aa11bb22cc33dd44ee55ff6677889900";
        // bare id
        assert_eq!(parse_gist_ref(id).as_deref(), Some(id));
        // canonical user/id
        assert_eq!(
            parse_gist_ref(&format!("https://gist.github.com/hllmr/{id}")).as_deref(),
            Some(id)
        );
        // id-only URL
        assert_eq!(
            parse_gist_ref(&format!("https://gist.github.com/{id}")).as_deref(),
            Some(id)
        );
        // with fragment + trailing slash
        assert_eq!(
            parse_gist_ref(&format!(
                "https://gist.github.com/hllmr/{id}/#file-silo-collection-json"
            ))
            .as_deref(),
            Some(id)
        );
        // api URL
        assert_eq!(
            parse_gist_ref(&format!("https://api.github.com/gists/{id}")).as_deref(),
            Some(id)
        );
        // raw usercontent URL
        assert_eq!(
            parse_gist_ref(&format!(
                "https://gist.githubusercontent.com/hllmr/{id}/raw/abc/silo-collection.json"
            ))
            .as_deref(),
            Some(id)
        );
    }

    #[test]
    fn rejects_non_gist_input() {
        assert_eq!(parse_gist_ref("https://gist.github.com/hllmr"), None); // user page, no id
        assert_eq!(parse_gist_ref("https://github.com/HLLMR/silo"), None); // not a gist
        assert_eq!(parse_gist_ref("hello world"), None);
        assert_eq!(parse_gist_ref("short123"), None); // too short to be an id
    }

    #[test]
    fn collection_ref_resolves_gists_and_repos() {
        let id = "aa11bb22cc33dd44ee55ff6677889900";
        // Gist forms → Gist.
        assert_eq!(
            parse_collection_ref(id),
            Some(CollectionRef::Gist(id.to_string()))
        );
        assert_eq!(
            parse_collection_ref(&format!("https://gist.github.com/hllmr/{id}")),
            Some(CollectionRef::Gist(id.to_string()))
        );
        // Repo forms → Repo.
        let repo = CollectionRef::Repo {
            owner: "HLLMR".into(),
            repo: "my-pack".into(),
        };
        assert_eq!(
            parse_collection_ref("https://github.com/HLLMR/my-pack"),
            Some(repo.clone())
        );
        assert_eq!(
            parse_collection_ref("https://github.com/HLLMR/my-pack/tree/main"),
            Some(repo.clone())
        );
        assert_eq!(
            parse_collection_ref("github.com/HLLMR/my-pack.git"),
            Some(repo.clone())
        );
        assert_eq!(parse_collection_ref("HLLMR/my-pack"), Some(repo));
    }

    #[test]
    fn collection_ref_rejects_junk_and_other_hosts() {
        assert_eq!(parse_collection_ref("hello world"), None);
        assert_eq!(parse_collection_ref("https://gitlab.com/foo/bar"), None); // not github
        assert_eq!(parse_collection_ref("foo.com/bar"), None); // bare host, not owner/repo
        assert_eq!(parse_collection_ref("https://github.com/HLLMR"), None); // no repo segment
    }

    #[test]
    fn repo_slug_is_prefixed_and_clean() {
        assert_eq!(repo_slug("Weekend Co-op Pack!"), "silo-weekend-co-op-pack");
        assert_eq!(
            repo_slug("  1980s American Farm  "),
            "silo-1980s-american-farm"
        );
        assert_eq!(repo_slug("***"), "silo-collection"); // nothing usable → default
    }

    #[test]
    fn readme_has_the_essentials() {
        let md = readme(
            &sample(),
            "https://silo.hllmr.com/c/?r=hllmr/silo-my-server-pack",
            "https://github.com/hllmr/silo-my-server-pack",
        );
        assert!(md.contains("# My Server Pack"));
        assert!(md.contains("Open in Silo"));
        // The "Open in Silo" button links the https handoff page (GitHub renders it),
        // never a silo:// link (GitHub's sanitizer strips custom schemes).
        assert!(md.contains("(https://silo.hllmr.com/c/?r=hllmr/silo-my-server-pack)"));
        assert!(!md.contains("silo://"));
        // The raw source URL is offered for pasting into Silo's importer.
        assert!(md.contains("https://github.com/hllmr/silo-my-server-pack"));
        assert!(md.contains("FS25_Foo"));
        assert!(md.contains("No mod files are redistributed"));
    }
}
