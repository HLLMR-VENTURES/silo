//! Catalog links for zips whose name isn't the catalog's techName (#386).
//!
//! Silo joins a library mod to its catalog entry by techName, which for a zip is its file name
//! without `.zip`. A few catalog entries ship a file named otherwise (`FS25_ToDo` downloads as
//! `FS25_FieldToDoList.zip`). Silo never renames a mod zip (multiplayer needs the exact bytes and
//! the game names the mod after the file), so when Browse installs one of these it records which
//! entry the file came from: the file's name, its whole-file SHA-256, and the entry's id. The
//! library's own catalog joins (the drawer's catalog facts, update checks, Verify integrity, and
//! sharing a mod set) ask here first. Importing a shared set still goes by techName alone.
//!
//! The link belongs to the bytes Silo installed under that name: it is used only when the file
//! has its recorded name and hashes to what was recorded. A different build under the same name
//! (an update from elsewhere, a file swapped in by hand) has another hash and isn't linked, and a
//! renamed file isn't either. The recorded size only spares hashing a file that can't match.
//! When the record or the file can't be read, callers get an error, never "no link": falling
//! back to the entry with the file's name could be another mod's.

use crate::db::{self, CatalogAliasRow};
use rusqlite::Connection;
use std::path::Path;

/// The catalog entry a library zip came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub catalog_id: String,
    /// The entry's techName when the file was installed (None when the catalog had none).
    pub catalog_tech_name: Option<String>,
    /// The address the file was downloaded from.
    pub download_url: Option<String>,
}

impl From<&CatalogAliasRow> for Link {
    fn from(r: &CatalogAliasRow) -> Self {
        Link {
            catalog_id: r.catalog_id.clone(),
            catalog_tech_name: r.catalog_tech_name.clone(),
            download_url: r.download_url.clone(),
        }
    }
}

/// The techName FS25 gives a zip: its file name without `.zip` (any case of the extension).
/// None for anything that isn't a `.zip` file name.
pub fn zip_tech_name(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    let cut = name.len().checked_sub(4)?;
    if cut == 0 || !name.is_char_boundary(cut) || !name[cut..].eq_ignore_ascii_case(".zip") {
        return None;
    }
    Some(name[..cut].to_string())
}

/// Whether a download named `filename` from an entry whose techName is `catalog_tech_name`
/// needs an alias: its techName isn't exactly the catalog's (the catalog lookup is exact, case
/// included), or the catalog has none.
pub fn needs_alias(filename: &str, catalog_tech_name: Option<&str>) -> bool {
    let Some(stem) = zip_tech_name(Path::new(filename)) else {
        return false;
    };
    catalog_tech_name.map(str::trim) != Some(stem.as_str())
}

/// How to put the record for one name and hash back as it was before `record` or `forget`
/// changed it, when the file it was for then isn't placed after all.
#[derive(Debug)]
pub struct Undo {
    tech_name: String,
    sha256: String,
    before: Option<CatalogAliasRow>,
}

impl Undo {
    /// Put the record back as it was.
    pub fn apply(self, conn: &Connection) -> Result<(), String> {
        match &self.before {
            Some(row) => db::record_catalog_alias(conn, row),
            None => db::forget_catalog_alias(conn, &self.tech_name, &self.sha256),
        }
    }
}

/// The record for this name and hash now, to undo a change to it.
fn undo_for(conn: &Connection, tech_name: &str, sha256: &str) -> Result<Undo, String> {
    let before = db::catalog_aliases(conn, tech_name)?
        .into_iter()
        .find(|r| r.archive_sha256 == sha256);
    Ok(Undo {
        tech_name: tech_name.to_string(),
        sha256: sha256.to_string(),
        before,
    })
}

/// Record that the zip named `tech_name` (`.zip` added) whose bytes are `size` long and hash
/// to `sha256` came from the catalog entry `link`, replacing the record for the same name and
/// bytes. Called before the file is placed, so a placed file always has its record; the
/// returned `Undo` restores the previous record if the file then isn't placed. The record is
/// for those bytes under that name wherever they are (another mods folder too).
pub fn record(
    conn: &Connection,
    tech_name: &str,
    sha256: &str,
    size: u64,
    link: &Link,
) -> Result<Undo, String> {
    let sha256 = sha256.to_ascii_lowercase();
    let undo = undo_for(conn, tech_name, &sha256)?;
    db::record_catalog_alias(
        conn,
        &CatalogAliasRow {
            tech_name: tech_name.to_string(),
            archive_sha256: sha256,
            size: size as i64,
            catalog_id: link.catalog_id.clone(),
            catalog_tech_name: link
                .catalog_tech_name
                .as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
            download_url: link.download_url.clone(),
        },
    )?;
    Ok(undo)
}

/// The catalog entry Silo installed the zip at `path` from, when the library knows it as
/// `tech_name` (the file's own name must be that) and the file hashes to what Silo recorded.
/// `Ok(None)` when there's no record for its name, no file there, or its size or hash matches
/// none; a file with no record of its size is never hashed. An error when the record or the
/// file can't be read.
pub fn link_for_file(
    conn: &Connection,
    tech_name: &str,
    path: &Path,
) -> Result<Option<Link>, String> {
    link_for_file_with(conn, tech_name, path, crate::provenance::sha256_file)
}

/// `link_for_file`, with the hashing passed in (tests count the calls).
fn link_for_file_with(
    conn: &Connection,
    tech_name: &str,
    path: &Path,
    hash: impl Fn(&Path) -> Result<String, String>,
) -> Result<Option<Link>, String> {
    if zip_tech_name(path).as_deref() != Some(tech_name) {
        return Ok(None);
    }
    let rows = db::catalog_aliases(conn, tech_name)?;
    if rows.is_empty() {
        return Ok(None);
    }
    let meta = match std::fs::metadata(path) {
        Ok(m) if m.is_file() => m,
        Ok(_) => return Ok(None),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Silo couldn't read {}: {e}", path.display())),
    };
    let size = meta.len() as i64;
    if !rows.iter().any(|r| r.size == size) {
        return Ok(None);
    }
    let sha = hash(path)?.to_ascii_lowercase();
    Ok(rows
        .iter()
        .find(|r| r.size == size && r.archive_sha256 == sha)
        .map(Link::from))
}

/// The catalog entry Silo installed a zip from, given the file's path and its whole-file
/// SHA-256 already computed (Verify integrity and sharing a set hash the file anyway).
pub fn link_for_hash(
    conn: &Connection,
    tech_name: &str,
    path: &Path,
    sha256: &str,
) -> Result<Option<Link>, String> {
    if zip_tech_name(path).as_deref() != Some(tech_name) {
        return Ok(None);
    }
    let sha = sha256.to_ascii_lowercase();
    Ok(db::catalog_aliases(conn, tech_name)?
        .iter()
        .find(|r| r.archive_sha256 == sha)
        .map(Link::from))
}

/// Whether any alias is recorded for a zip named `tech_name`.
pub fn has_any(conn: &Connection, tech_name: &str) -> Result<bool, String> {
    Ok(!db::catalog_aliases(conn, tech_name)?.is_empty())
}

/// Drop the alias of the zip named `tech_name` with these bytes: Silo is installing those bytes
/// from an entry whose techName is that name, so they need none. The returned `Undo` puts it
/// back if the file then isn't placed.
pub fn forget(conn: &Connection, tech_name: &str, sha256: &str) -> Result<Undo, String> {
    let sha256 = sha256.to_ascii_lowercase();
    let undo = undo_for(conn, tech_name, &sha256)?;
    db::forget_catalog_alias(conn, tech_name, &sha256)?;
    Ok(undo)
}

/// The GitHub repository (`owner/repo`, lowercased) and file name of a release-asset address:
/// `https://github.com/<owner>/<repo>/releases/download/<tag>/<file>`. None for anything else.
pub fn release_asset(url: &str) -> Option<(String, String)> {
    let u = url::Url::parse(url).ok()?;
    let host = u.host_str()?.to_ascii_lowercase();
    if u.scheme() != "https" || (host != "github.com" && host != "www.github.com") {
        return None;
    }
    let segs: Vec<&str> = u.path_segments()?.collect();
    match segs.as_slice() {
        [owner, repo, "releases", "download", tag, file]
            if !owner.is_empty() && !repo.is_empty() && !tag.is_empty() && !file.is_empty() =>
        {
            Some((
                format!("{owner}/{repo}").to_ascii_lowercase(),
                file.to_string(),
            ))
        }
        _ => None,
    }
}

/// Whether an update downloaded from `asset_url` is another release of the build `link` was
/// installed from: a release asset of the same GitHub repository with the same file name. A
/// repository can publish several mods, so the repository alone isn't enough; FS25 knows a
/// mod by its file name.
pub fn another_release_of(link: &Link, asset_url: &str) -> bool {
    match (
        link.download_url.as_deref().and_then(release_asset),
        release_asset(asset_url),
    ) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

/// Whether the download at `url` is a file named exactly as `dest` (its last path segment,
/// without query or fragment).
pub fn url_names_file(url: &str, dest: &Path) -> bool {
    let Ok(u) = url::Url::parse(url) else {
        return false;
    };
    let last = u.path_segments().and_then(|mut s| s.next_back());
    match (last, dest.file_name().and_then(|n| n.to_str())) {
        (Some(a), Some(b)) => !a.is_empty() && a == b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!(
            "silo_catalogalias_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// A database at the current schema (the alias table comes from its migration).
    fn conn_in(dir: &Path) -> Connection {
        db::open(&dir.join("silo.db")).unwrap()
    }

    fn sha(bytes: &[u8]) -> String {
        use sha2::{Digest, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    const TODO_URL: &str =
        "https://github.com/knoellix/FS25_ToDo/releases/download/v0.1.0.17/FS25_FieldToDoList.zip";

    fn todo() -> Link {
        Link {
            catalog_id: "cat-todo".into(),
            catalog_tech_name: Some("FS25_ToDo".into()),
            download_url: Some(TODO_URL.into()),
        }
    }

    /// Write `bytes` as `<dir>/<tech>.zip` and record it as installed from `todo()`.
    fn installed(conn: &Connection, dir: &Path, tech: &str, bytes: &[u8]) -> std::path::PathBuf {
        let file = dir.join(format!("{tech}.zip"));
        std::fs::write(&file, bytes).unwrap();
        record(conn, tech, &sha(bytes), bytes.len() as u64, &todo()).unwrap();
        file
    }

    fn set_mtime(path: &Path, t: std::time::SystemTime) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(t).unwrap();
    }

    #[test]
    fn zip_tech_name_is_the_file_name_without_zip() {
        assert_eq!(
            zip_tech_name(Path::new("a/FS25_FieldToDoList.zip")).as_deref(),
            Some("FS25_FieldToDoList")
        );
        assert_eq!(
            zip_tech_name(Path::new("FS25_X.ZIP")).as_deref(),
            Some("FS25_X")
        );
        assert_eq!(zip_tech_name(Path::new("FS25_X")), None);
        assert_eq!(zip_tech_name(Path::new(".zip")), None);
        assert_eq!(zip_tech_name(Path::new("FS25_X.zip.part")), None);
    }

    #[test]
    fn an_alias_is_needed_only_when_the_name_differs_exactly() {
        assert!(needs_alias("FS25_FieldToDoList.zip", Some("FS25_ToDo")));
        assert!(!needs_alias("FS25_ToDo.zip", Some("FS25_ToDo")));
        // The catalog's lookup is exact, case included, so a case-only difference needs one.
        assert!(needs_alias("FS25_todo.zip", Some("FS25_ToDo")));
        // No techName in the catalog: the file can only be joined through the alias.
        assert!(needs_alias("FS25_X.zip", None));
        assert!(!needs_alias("notazip", Some("FS25_X")));
    }

    #[test]
    fn the_installed_file_is_linked_by_its_hash() {
        let dir = temp_dir("keeps");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        let link = link_for_file(&conn, "FS25_FieldToDoList", &file)
            .unwrap()
            .unwrap();
        assert_eq!(link, todo());
        // Copied elsewhere (a new modified time): the same bytes, still linked.
        set_mtime(&file, std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(todo())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_different_build_with_the_same_size_and_time_loses_the_link() {
        let dir = temp_dir("replaced");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        let when = std::fs::metadata(&file).unwrap().modified().unwrap();
        // Same length and, put back by hand, the same modified time: only the hash differs.
        std::fs::write(&file, b"build two").unwrap();
        set_mtime(&file, when);
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            None
        );
        assert_eq!(
            link_for_hash(&conn, "FS25_FieldToDoList", &file, &sha(b"build two")).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_whose_size_matches_no_record_is_not_hashed() {
        let dir = temp_dir("size");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        std::fs::write(&file, b"a longer build two").unwrap();
        let hashed = Cell::new(0);
        let count = |p: &Path| {
            hashed.set(hashed.get() + 1);
            crate::provenance::sha256_file(p)
        };
        assert_eq!(
            link_for_file_with(&conn, "FS25_FieldToDoList", &file, count).unwrap(),
            None
        );
        assert_eq!(hashed.get(), 0);
        // A mod without an alias is never hashed either.
        let plain = dir.join("FS25_Plain.zip");
        std::fs::write(&plain, b"x").unwrap();
        assert_eq!(
            link_for_file_with(&conn, "FS25_Plain", &plain, count).unwrap(),
            None
        );
        assert_eq!(hashed.get(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_link_needs_the_file_to_be_the_named_mod() {
        let dir = temp_dir("named");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        // Another mod's name, or the same bytes under another file name.
        assert_eq!(link_for_file(&conn, "FS25_Other", &file).unwrap(), None);
        let copy = dir.join("FS25_Copy.zip");
        std::fs::copy(&file, &copy).unwrap();
        assert_eq!(link_for_file(&conn, "FS25_Copy", &copy).unwrap(), None);
        assert_eq!(
            link_for_hash(&conn, "FS25_Copy", &copy, &sha(b"build one")).unwrap(),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn same_named_files_in_two_folders_are_each_judged_by_their_bytes() {
        let dir = temp_dir("folders");
        let conn = conn_in(&dir);
        let one = dir.join("one");
        let two = dir.join("two");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let a = installed(&conn, &one, "FS25_FieldToDoList", b"build one");
        // The other folder holds a different build of the same size and time, never installed.
        let b = two.join("FS25_FieldToDoList.zip");
        std::fs::write(&b, b"build two").unwrap();
        set_mtime(&b, std::fs::metadata(&a).unwrap().modified().unwrap());
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &a)
            .unwrap()
            .is_some());
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &b).unwrap(),
            None
        );
        // Installed there too: each build keeps its own record.
        let b = installed(&conn, &two, "FS25_FieldToDoList", b"build two!");
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &a)
            .unwrap()
            .is_some());
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &b)
            .unwrap()
            .is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_update_is_the_same_entry_only_as_the_same_file_from_the_same_repo() {
        assert_eq!(
            release_asset(TODO_URL),
            Some((
                "knoellix/fs25_todo".to_string(),
                "FS25_FieldToDoList.zip".to_string()
            ))
        );
        let later =
            "https://github.com/Knoellix/FS25_ToDo/releases/download/v0.2/FS25_FieldToDoList.zip";
        assert!(another_release_of(&todo(), later));
        // Another mod published from the same repository is not this entry's release.
        let sibling =
            "https://github.com/knoellix/FS25_ToDo/releases/download/v0.2/FS25_OtherMod.zip";
        assert!(!another_release_of(&todo(), sibling));
        let fork =
            "https://github.com/someone/FS25_ToDo/releases/download/v0.2/FS25_FieldToDoList.zip";
        assert!(!another_release_of(&todo(), fork));
        // Not a release asset, or not GitHub, or no address recorded: never the same.
        assert_eq!(
            release_asset("https://github.com/knoellix/FS25_ToDo/archive/main.zip"),
            None
        );
        assert_eq!(
            release_asset("http://github.com/k/r/releases/download/v1/x.zip"),
            None
        );
        assert_eq!(
            release_asset("https://github.com.evil.example/k/r/releases/download/v1/x.zip"),
            None
        );
        let no_url = Link {
            download_url: None,
            ..todo()
        };
        assert!(!another_release_of(&no_url, later));
    }

    #[test]
    fn an_install_that_isnt_placed_puts_the_record_back() {
        let dir = temp_dir("undo");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        // The same bytes from another entry, then not placed: the first entry's link is back.
        let other = Link {
            catalog_id: "cat-other".into(),
            ..todo()
        };
        let undo = record(&conn, "FS25_FieldToDoList", &sha(b"build one"), 9, &other).unwrap();
        let now = link_for_file(&conn, "FS25_FieldToDoList", &file)
            .unwrap()
            .unwrap();
        assert_eq!(now.catalog_id, "cat-other");
        undo.apply(&conn).unwrap();
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(todo())
        );
        // Forgotten, then not placed: back as well.
        let undo = forget(&conn, "FS25_FieldToDoList", &sha(b"build one")).unwrap();
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            None
        );
        undo.apply(&conn).unwrap();
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(todo())
        );
        // A new record with nothing before it is removed again.
        let undo = record(&conn, "FS25_New", &sha(b"n"), 1, &todo()).unwrap();
        assert!(has_any(&conn, "FS25_New").unwrap());
        undo.apply(&conn).unwrap();
        assert!(!has_any(&conn, "FS25_New").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_that_cant_be_hashed_is_an_error_not_no_link() {
        let dir = temp_dir("unreadable");
        let conn = conn_in(&dir);
        let file = installed(&conn, &dir, "FS25_FieldToDoList", b"build one");
        let locked = |_: &Path| -> Result<String, String> { Err("in use".into()) };
        assert!(link_for_file_with(&conn, "FS25_FieldToDoList", &file, locked).is_err());
        // Gone is not an error: there's simply no file to link.
        std::fs::remove_file(&file).unwrap();
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            None
        );
        // An unreadable record is an error too.
        conn.execute_batch("DROP TABLE catalog_alias").unwrap();
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &file).is_err());
        assert!(link_for_hash(&conn, "FS25_FieldToDoList", &file, "ab").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_download_names_the_file_only_by_its_exact_name() {
        let dest = Path::new("mods/FS25_FieldToDoList.zip");
        assert!(url_names_file(TODO_URL, dest));
        assert!(url_names_file(&format!("{TODO_URL}?raw=1#x"), dest));
        let renamed = "https://github.com/k/r/releases/download/v2/FS25_FieldToDoList2.zip";
        assert!(!url_names_file(renamed, dest));
        assert!(!url_names_file(
            "https://github.com/k/r/releases/download/v2/fs25_fieldtodolist.zip",
            dest
        ));
        assert!(!url_names_file("not a url", dest));
    }

    #[test]
    fn forgetting_an_alias_drops_only_those_bytes() {
        let dir = temp_dir("forget");
        let conn = conn_in(&dir);
        let one = dir.join("one");
        let two = dir.join("two");
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let a = installed(&conn, &one, "FS25_FieldToDoList", b"build one");
        let b = installed(&conn, &two, "FS25_FieldToDoList", b"build two!");
        forget(&conn, "FS25_FieldToDoList", &sha(b"build one")).unwrap();
        assert_eq!(
            link_for_file(&conn, "FS25_FieldToDoList", &a).unwrap(),
            None
        );
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &b)
            .unwrap()
            .is_some());
        assert!(has_any(&conn, "FS25_FieldToDoList").unwrap());
        assert!(!has_any(&conn, "FS25_Plain").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
