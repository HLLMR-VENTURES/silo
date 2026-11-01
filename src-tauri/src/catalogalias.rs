//! Catalog links for zips whose name isn't the catalog's techName (#386).
//!
//! Silo joins a library mod to its catalog entry by techName, which for a zip is its file name
//! without `.zip`. A few catalog entries ship a file named otherwise (`FS25_ToDo` downloads as
//! `FS25_FieldToDoList.zip`). Silo never renames a mod zip (multiplayer needs the exact bytes and
//! the game names the mod after the file), so when Browse installs one of these it records which
//! entry the file came from: the file's name, its whole-file SHA-256, and the entry's id. Every
//! catalog join (the drawer's catalog facts, update checks, Verify integrity) asks here first.
//!
//! The link belongs to the bytes Silo installed. A different build under the same name (an
//! update from elsewhere, a file swapped in by hand) has another hash and isn't linked. Size and
//! modified time are only a shortcut for "still the file Silo recorded"; when either differs the
//! file is hashed again before the link is used.

use crate::db::{self, CatalogAliasRow};
use rusqlite::Connection;
use std::path::Path;

/// The catalog entry a library zip came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub catalog_id: String,
    /// The entry's techName when the file was installed (None when the catalog had none).
    pub catalog_tech_name: Option<String>,
}

impl From<&CatalogAliasRow> for Link {
    fn from(r: &CatalogAliasRow) -> Self {
        Link {
            catalog_id: r.catalog_id.clone(),
            catalog_tech_name: r.catalog_tech_name.clone(),
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

/// Size and modified time (ms since the epoch) of the file `path` leads to. None when it can't
/// be read or isn't a file.
fn stats(path: &Path) -> Option<(i64, i64)> {
    let m = std::fs::metadata(path).ok()?;
    if !m.is_file() {
        return None;
    }
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((m.len() as i64, mtime))
}

/// Record that the zip at `file`, whose whole-file SHA-256 is `sha256`, was installed by Silo
/// from catalog entry `catalog_id`. Call right after placing it.
pub fn record(
    conn: &Connection,
    file: &Path,
    sha256: &str,
    catalog_id: &str,
    catalog_tech_name: Option<&str>,
) -> Result<(), String> {
    let tech_name = zip_tech_name(file).ok_or_else(|| format!("{} isn't a zip", file.display()))?;
    let (size, mtime_ms) =
        stats(file).ok_or_else(|| format!("couldn't read {}", file.display()))?;
    db::record_catalog_alias(
        conn,
        &CatalogAliasRow {
            tech_name,
            archive_sha256: sha256.to_ascii_lowercase(),
            size,
            mtime_ms,
            catalog_id: catalog_id.to_string(),
            catalog_tech_name: catalog_tech_name
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_string),
        },
    )
}

/// The catalog entry Silo installed the zip at `path` from, when the library knows it as
/// `tech_name` (the file's own name must be that) and it is still exactly the file Silo
/// installed. Hashes the file only when there is an alias for its name and its size or
/// modified time changed since it was recorded.
pub fn link_for_file(conn: &Connection, tech_name: &str, path: &Path) -> Option<Link> {
    link_for_file_with(conn, tech_name, path, crate::provenance::sha256_file)
}

/// `link_for_file`, with the hashing passed in (tests count the calls).
fn link_for_file_with(
    conn: &Connection,
    tech_name: &str,
    path: &Path,
    hash: impl Fn(&Path) -> Result<String, String>,
) -> Option<Link> {
    if zip_tech_name(path).as_deref() != Some(tech_name) {
        return None;
    }
    let rows = db::catalog_aliases(conn, tech_name);
    if rows.is_empty() {
        return None;
    }
    let (size, mtime_ms) = stats(path)?;
    if let Some(r) = rows
        .iter()
        .find(|r| r.size == size && r.mtime_ms == mtime_ms)
    {
        return Some(r.into());
    }
    let sha = hash(path).ok()?.to_ascii_lowercase();
    let r = rows.iter().find(|r| r.archive_sha256 == sha)?;
    // The same bytes with a new time (a copy, say): the next check needn't hash it again.
    if let Err(e) = db::touch_catalog_alias(conn, tech_name, &sha, size, mtime_ms) {
        log::warn!("[catalog] couldn't note {tech_name}'s new file time: {e}");
    }
    Some(r.into())
}

/// The catalog entry Silo installed a zip from, given the file's path and its whole-file
/// SHA-256 already computed (Verify integrity hashes the file anyway). Matches on the hash
/// only, never on size and time.
pub fn link_for_hash(
    conn: &Connection,
    tech_name: &str,
    path: &Path,
    sha256: &str,
) -> Option<Link> {
    if zip_tech_name(path).as_deref() != Some(tech_name) {
        return None;
    }
    let sha = sha256.to_ascii_lowercase();
    db::catalog_aliases(conn, tech_name)
        .iter()
        .find(|r| r.archive_sha256 == sha)
        .map(Link::from)
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

    fn set_mtime(path: &Path, secs_ago: u64) {
        let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
        f.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(secs_ago))
            .unwrap();
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
    fn the_installed_file_keeps_its_link_without_rehashing() {
        let dir = temp_dir("keeps");
        let conn = conn_in(&dir);
        let file = dir.join("FS25_FieldToDoList.zip");
        std::fs::write(&file, b"build one").unwrap();
        record(
            &conn,
            &file,
            &sha(b"build one"),
            "cat-todo",
            Some("FS25_ToDo"),
        )
        .unwrap();

        let hashed = Cell::new(0);
        let count = |p: &Path| {
            hashed.set(hashed.get() + 1);
            crate::provenance::sha256_file(p)
        };
        let link = link_for_file_with(&conn, "FS25_FieldToDoList", &file, count).unwrap();
        assert_eq!(link.catalog_id, "cat-todo");
        assert_eq!(link.catalog_tech_name.as_deref(), Some("FS25_ToDo"));
        assert_eq!(hashed.get(), 0, "same size and time: no hash");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_different_build_under_the_same_name_loses_the_link() {
        let dir = temp_dir("replaced");
        let conn = conn_in(&dir);
        let file = dir.join("FS25_FieldToDoList.zip");
        std::fs::write(&file, b"build one").unwrap();
        record(
            &conn,
            &file,
            &sha(b"build one"),
            "cat-todo",
            Some("FS25_ToDo"),
        )
        .unwrap();

        // Same length, so only the hash can tell them apart.
        std::fs::write(&file, b"build two").unwrap();
        set_mtime(&file, 3600);
        assert_eq!(link_for_file(&conn, "FS25_FieldToDoList", &file), None);
        assert_eq!(
            link_for_hash(&conn, "FS25_FieldToDoList", &file, &sha(b"build two")),
            None
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_same_bytes_with_a_new_time_keep_the_link_and_note_the_time() {
        let dir = temp_dir("touched");
        let conn = conn_in(&dir);
        let file = dir.join("FS25_FieldToDoList.zip");
        std::fs::write(&file, b"build one").unwrap();
        record(
            &conn,
            &file,
            &sha(b"build one"),
            "cat-todo",
            Some("FS25_ToDo"),
        )
        .unwrap();
        set_mtime(&file, 3600); // copied elsewhere and back, say

        let hashed = Cell::new(0);
        let count = |p: &Path| {
            hashed.set(hashed.get() + 1);
            crate::provenance::sha256_file(p)
        };
        assert!(link_for_file_with(&conn, "FS25_FieldToDoList", &file, count).is_some());
        assert_eq!(hashed.get(), 1);
        assert!(link_for_file_with(&conn, "FS25_FieldToDoList", &file, count).is_some());
        assert_eq!(hashed.get(), 1, "the new time was noted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_link_needs_the_file_to_be_the_named_mod() {
        let dir = temp_dir("named");
        let conn = conn_in(&dir);
        let file = dir.join("FS25_FieldToDoList.zip");
        std::fs::write(&file, b"build one").unwrap();
        record(
            &conn,
            &file,
            &sha(b"build one"),
            "cat-todo",
            Some("FS25_ToDo"),
        )
        .unwrap();

        // Another mod's name, or the same bytes under another file name.
        assert_eq!(link_for_file(&conn, "FS25_Other", &file), None);
        let copy = dir.join("FS25_Copy.zip");
        std::fs::copy(&file, &copy).unwrap();
        assert_eq!(link_for_file(&conn, "FS25_Copy", &copy), None);
        assert_eq!(
            link_for_hash(&conn, "FS25_Copy", &copy, &sha(b"build one")),
            None
        );
        // A mod without an alias is never hashed.
        let plain = dir.join("FS25_Plain.zip");
        std::fs::write(&plain, b"x").unwrap();
        let never = |_: &Path| -> Result<String, String> { panic!("hashed") };
        assert_eq!(link_for_file_with(&conn, "FS25_Plain", &plain, never), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn each_installed_build_keeps_its_own_link() {
        let dir = temp_dir("builds");
        let conn = conn_in(&dir);
        let a = dir.join("one").join("FS25_FieldToDoList.zip");
        let b = dir.join("two").join("FS25_FieldToDoList.zip");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::create_dir_all(b.parent().unwrap()).unwrap();
        std::fs::write(&a, b"build one").unwrap();
        std::fs::write(&b, b"build two!").unwrap();
        record(&conn, &a, &sha(b"build one"), "cat-todo", Some("FS25_ToDo")).unwrap();
        record(
            &conn,
            &b,
            &sha(b"build two!"),
            "cat-todo",
            Some("FS25_ToDo"),
        )
        .unwrap();
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &a).is_some());
        assert!(link_for_file(&conn, "FS25_FieldToDoList", &b).is_some());
        assert!(link_for_hash(&conn, "FS25_FieldToDoList", &a, &sha(b"build one")).is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
