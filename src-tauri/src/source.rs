//! Where a mod came from: a web page the user saved for each mod (#196). Silo only stores the
//! address. Nothing in this module, or anywhere else in the app, ever requests it; the user's
//! browser opens it.

use crate::db::{self, RepoRow};
use crate::github;
use rusqlite::Connection;
use url::Url;

/// Longest address we keep. Real download pages are far shorter.
pub const MAX_URL_LEN: usize = 2048;

/// The one message for every address we refuse.
pub const BAD_URL: &str = "That doesn't look like a web page address. Paste the https:// link you downloaded the mod from.";

/// Check a pasted source page address and return it in normal form.
/// @param raw what the user pasted
/// @return the cleaned https address, or the `BAD_URL` message
pub fn validate_url(raw: &str) -> Result<String, String> {
    let bad = || BAD_URL.to_string();
    let s = raw.trim();
    if s.is_empty() || s.len() > MAX_URL_LEN {
        return Err(bad());
    }
    let url = Url::parse(s).map_err(|_| bad())?;
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return Err(bad());
    }
    // Refuses localhost, private and link-local addresses. It only parses; it never connects.
    let url = crate::net::validate_outbound_url(url.as_str(), false).map_err(|_| bad())?;
    if url.as_str().len() > MAX_URL_LEN {
        return Err(bad());
    }
    Ok(url.into())
}

/// The `owner/repo` a github.com address points at, if it is a repository page.
/// @param url an already validated address
pub fn github_repo(url: &str) -> Option<(String, String)> {
    let url = Url::parse(url).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if host != "github.com" && host != "www.github.com" {
        return None;
    }
    // Same rules (and reserved paths like /sponsors) as a repo link found in a modDesc.
    let (owner, repo) = github::find_repo_in_text(&format!("github.com{}", url.path()))?;
    (github::valid_owner(&owner) && github::valid_repo(&repo)).then_some((owner, repo))
}

/// Save (or remove, when `url` is empty or None) a mod's source page. When the page is a GitHub
/// repository and the mod has no repo linked yet, it also links that repo for update checks.
/// @return the repo link this created, if any
pub fn save(
    conn: &Connection,
    tech_name: &str,
    url: Option<&str>,
) -> Result<Option<RepoRow>, String> {
    let raw = url.map(str::trim).unwrap_or("");
    if raw.is_empty() {
        db::set_source(conn, tech_name, None)?;
        return Ok(None);
    }
    let clean = validate_url(raw)?;
    db::set_source(conn, tech_name, Some(&clean))?;

    let Some((owner, repo)) = github_repo(&clean) else {
        return Ok(None);
    };
    if db::has_repo(conn, tech_name) {
        return Ok(None);
    }
    db::set_repo(conn, tech_name, &owner, &repo)?;
    Ok(Some(RepoRow {
        tech_name: tech_name.to_string(),
        owner,
        repo,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validation_table() {
        let ok = [
            (
                "https://example.com/mods/tractor",
                "https://example.com/mods/tractor",
            ),
            (
                "  https://www.example.com/a?b=1  ",
                "https://www.example.com/a?b=1",
            ),
            ("https://example.com", "https://example.com/"),
            (
                "https://github.com/owner/repo",
                "https://github.com/owner/repo",
            ),
        ];
        for (raw, want) in ok {
            assert_eq!(validate_url(raw).as_deref(), Ok(want), "{raw}");
        }
        let too_long = format!("https://example.com/{}", "a".repeat(MAX_URL_LEN));
        let refused = [
            "",
            "   ",
            "http://example.com/mod",
            "https://localhost/mod",
            "https://127.0.0.1/mod",
            "https://10.0.0.5/mod",
            "https://192.168.1.2/mod",
            "https://169.254.1.1/mod",
            "javascript:alert(1)",
            "file:///c:/mods/a.zip",
            "ftp://example.com/a.zip",
            "not a url",
            "example.com/mod",
            "https://user:pass@example.com/mod",
            "https://user@example.com/mod",
            too_long.as_str(),
        ];
        for raw in refused {
            assert_eq!(validate_url(raw), Err(BAD_URL.to_string()), "{raw}");
        }
    }

    #[test]
    fn github_repo_only_for_github_repository_pages() {
        let r = |s: &str| github_repo(s).map(|(o, n)| format!("{o}/{n}"));
        assert_eq!(
            r("https://github.com/owner/repo").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(
            r("https://github.com/owner/repo/releases/tag/v1").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(
            r("https://www.github.com/owner/repo.git").as_deref(),
            Some("owner/repo")
        );
        assert_eq!(r("https://github.com/owner"), None);
        assert_eq!(r("https://github.com/sponsors/someone"), None);
        assert_eq!(r("https://example.com/github.com/owner/repo"), None);
        assert_eq!(r("https://notgithub.com/owner/repo"), None);
    }

    fn temp_conn(tag: &str) -> Connection {
        let base = std::env::temp_dir().join(format!("silo_source_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        db::open(&base.join("silo.db")).unwrap()
    }

    #[test]
    fn save_stores_the_cleaned_page_and_refuses_bad_ones() {
        let conn = temp_conn("save");
        assert_eq!(
            save(&conn, "FS25_A", Some(" https://example.com/a ")),
            Ok(None)
        );
        assert_eq!(db::load_sources(&conn)[0].url, "https://example.com/a");

        assert_eq!(
            save(&conn, "FS25_A", Some("http://example.com/a")),
            Err(BAD_URL.to_string())
        );
        assert_eq!(
            db::load_sources(&conn)[0].url,
            "https://example.com/a",
            "kept the old one"
        );

        assert_eq!(save(&conn, "FS25_A", None), Ok(None));
        assert!(db::load_sources(&conn).is_empty());
        assert_eq!(save(&conn, "FS25_A", Some("")), Ok(None));
    }

    #[test]
    fn a_github_page_links_the_repo_only_when_none_is_linked() {
        let conn = temp_conn("ghlink");
        let linked = save(
            &conn,
            "FS25_A",
            Some("https://github.com/Owner/Repo/releases"),
        )
        .unwrap();
        let linked = linked.expect("links the repo");
        assert_eq!(
            (linked.owner.as_str(), linked.repo.as_str()),
            ("Owner", "Repo")
        );
        assert_eq!(linked.tech_name, "FS25_A");
        let repos = db::load_repos(&conn);
        assert_eq!(repos.len(), 1);
        assert_eq!(
            (repos[0].owner.as_str(), repos[0].repo.as_str()),
            ("Owner", "Repo")
        );

        // A mod that already has a repo keeps it.
        db::set_repo(&conn, "FS25_B", "mine", "kept").unwrap();
        assert_eq!(
            save(&conn, "FS25_B", Some("https://github.com/other/thing")),
            Ok(None)
        );
        let b = db::load_repos(&conn)
            .into_iter()
            .find(|r| r.tech_name == "FS25_B")
            .unwrap();
        assert_eq!((b.owner.as_str(), b.repo.as_str()), ("mine", "kept"));

        // A non-GitHub page links nothing.
        assert_eq!(
            save(&conn, "FS25_C", Some("https://example.com/c")),
            Ok(None)
        );
        assert!(!db::has_repo(&conn, "FS25_C"));
    }
}
