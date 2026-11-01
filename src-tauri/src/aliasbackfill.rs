//! Catalog links for zips Silo installed under another name before #388 (#402).
//!
//! Since #388 a Browse install records which catalog entry a zip came from when the file isn't
//! named after the entry's techName (`catalogalias`). A zip installed that way earlier has no
//! record, so its drawer said "Not in Silo's catalog", update checks skipped it and Verify
//! integrity said Unverified. After a library scan, this looks such zips up and records the same
//! link an install would have.
//!
//! SiloAPI has no "which entry lists these bytes" query, so the candidates come from what the
//! catalog already publishes: every entry with a GitHub source (`GET /mods?source=github`), the
//! only source Silo installs from directly and the one SiloAPI hashes. A zip is hashed only when
//! all of these hold:
//! - one of those entries downloads a file with the zip's name (any case), and none of them is
//!   named after the zip (a file named after its entry is joined by techName and needs no link);
//! - the catalog has no entry with the zip's techName (`POST /mods/lookup`);
//! - its file name is its techName (as the joins require).
//!
//! It is then linked when exactly one catalog entry lists its bytes: as the entry's latest build
//! (`archiveSha256` in the listing), or, for an entry that downloads a file with the zip's name,
//! as the build of the zip's own version (`GET /mods/:id/manifest?version=`, the comparison
//! Verify integrity makes). When no entry or several do, nothing is recorded. A zip whose bytes
//! already have a link is left alone. The record is an install's (`catalogalias::record`), made
//! under the library lock.
//!
//! When it runs: after every library scan, off the UI thread, one run at a time (a scan during a
//! run leaves its new files to the next one). Each version of a file (its path, size and modified
//! time, as the scan cache keys it) is looked at once: once decided, it isn't hashed or looked up
//! again until the file changes. A failed or offline lookup, a busy library, or a file that can't
//! be read or changes meanwhile decides nothing, so that file is tried again after the next scan.
//! A run hashes and looks up at most `MAX_HASHED` files; the rest wait for the next scan. Nothing
//! here is shown to the user; it's only logged.

use crate::catalogalias::{self, Link};
use crate::db;
use crate::siloapi;
use rusqlite::Connection;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// The most files one run hashes and looks up.
pub const MAX_HASHED: usize = 25;

/// The most listing pages read (100 entries each); a longer listing is a failed lookup.
const MAX_LIST_PAGES: u32 = 20;

/// A zip from a library scan.
#[derive(Debug, Clone)]
pub struct LibraryZip {
    pub path: PathBuf,
    /// The techName the library knows it by (its file name without `.zip`).
    pub tech_name: String,
    /// The size and modified time the scan saw.
    pub size: u64,
    pub mtime_ms: u64,
    /// The modDesc version, when it has one.
    pub version: Option<String>,
}

/// A catalog entry Silo can download from directly.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub tech_name: Option<String>,
    /// The whole-zip SHA-256 of its latest build, when the catalog has hashed it.
    pub archive_sha256: Option<String>,
    /// Its sources' download addresses.
    pub download_urls: Vec<String>,
}

impl From<&siloapi::BrowseMod> for Entry {
    fn from(m: &siloapi::BrowseMod) -> Self {
        Entry {
            id: m.id.clone(),
            tech_name: m.tech_name.clone(),
            archive_sha256: m.archive_sha256.clone(),
            download_urls: m
                .sources
                .iter()
                .filter_map(|s| s.download_url.clone())
                .collect(),
        }
    }
}

impl Entry {
    /// The address it downloads a file named `file` from (any case), if any.
    fn download_named(&self, file: &str) -> Option<&str> {
        self.download_urls
            .iter()
            .find(|u| {
                [
                    siloapi::filename_from_url(u),
                    crate::update::asset_file_name(u),
                ]
                .iter()
                .flatten()
                .any(|n| n.eq_ignore_ascii_case(file))
            })
            .map(String::as_str)
    }

    fn lists_latest(&self, sha256: &str) -> bool {
        self.archive_sha256
            .as_deref()
            .is_some_and(|h| h.eq_ignore_ascii_case(sha256))
    }
}

/// What the backfill asks the catalog.
pub trait Catalog {
    /// Every entry Silo can download from directly.
    fn entries(&self) -> Result<Vec<Entry>, String>;
    /// Which of `names` are an entry's techName, exactly.
    fn known_tech_names(&self, names: &[String]) -> Result<HashSet<String>, String>;
    /// The whole-zip SHA-256 the catalog has for entry `id`'s build of `version`; None when it
    /// has no hashed build of that version.
    fn archive_sha256(&self, id: &str, version: &str) -> Result<Option<String>, String>;
}

/// The catalog at `base`.
pub struct SiloApi {
    pub base: String,
}

impl Catalog for SiloApi {
    fn entries(&self) -> Result<Vec<Entry>, String> {
        let mods = siloapi::list_source(&self.base, "github", MAX_LIST_PAGES)?;
        Ok(mods.iter().map(Entry::from).collect())
    }

    fn known_tech_names(&self, names: &[String]) -> Result<HashSet<String>, String> {
        Ok(siloapi::lookup(&self.base, names)?
            .into_iter()
            .filter_map(|r| r.tech_name)
            .collect())
    }

    fn archive_sha256(&self, id: &str, version: &str) -> Result<Option<String>, String> {
        Ok(siloapi::manifest(&self.base, id, version)?.and_then(|m| m.archive_sha256))
    }
}

/// What the backfill made of one zip.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Linked to this catalog entry.
    Linked(String),
    /// No entry downloads a file with its name.
    NoCandidate,
    /// An entry that downloads it is named after it: the techName join finds it.
    NamedAfterEntry,
    /// The catalog has an entry with its techName.
    InCatalog,
    /// Its bytes already have a link.
    AlreadyLinked,
    /// No entry lists its bytes.
    NoMatch,
    /// This many entries list its bytes, so it can't be told which it came from.
    Several(usize),
    /// Nothing was decided (why); it's looked at again after the next scan.
    Retry(String),
}

impl Outcome {
    /// Whether this settles the file's current version.
    fn decided(&self) -> bool {
        !matches!(self, Outcome::Retry(_))
    }
}

/// Look up the zips in `zips` that need it (see the module notes) and record the links found.
/// `hash` gives a file's whole-file SHA-256; `lock` takes the library lock for a record. Never
/// fails: what couldn't be done is logged and tried again after the next scan. Returns what it
/// made of each zip it looked at.
pub fn run<G>(
    conn: &Connection,
    zips: &[LibraryZip],
    catalog: &impl Catalog,
    hash: impl Fn(&Path) -> Result<String, String>,
    lock: impl Fn() -> Result<G, String>,
) -> Vec<(PathBuf, Outcome)> {
    let mut out = Vec::new();
    let mut pending = Vec::new();
    for z in zips {
        if catalogalias::zip_tech_name(&z.path).as_deref() != Some(z.tech_name.as_str()) {
            continue;
        }
        match db::alias_checked(conn, &path_key(z), z.size, z.mtime_ms) {
            Ok(true) => {}
            Ok(false) => pending.push(z),
            Err(e) => {
                log::warn!("[catalog] link backfill skipped: couldn't read what it checked: {e}");
                return out;
            }
        }
    }
    if pending.is_empty() {
        return out;
    }
    let entries = match catalog.entries() {
        Ok(e) => e,
        Err(e) => {
            log::info!("[catalog] link backfill skipped, trying after the next scan: {e}");
            retry_all(&pending, &e, &mut out);
            return out;
        }
    };

    // Which zips some entry downloads under their name; the rest are settled without hashing.
    let mut named: Vec<(&LibraryZip, Vec<&Entry>)> = Vec::new();
    for z in pending {
        let file = file_name(z);
        let by_name: Vec<&Entry> = entries
            .iter()
            .filter(|e| e.download_named(file).is_some())
            .collect();
        if by_name.is_empty() {
            settle(conn, z, Outcome::NoCandidate, &mut out);
        } else if by_name
            .iter()
            .any(|e| !catalogalias::needs_alias(file, e.tech_name.as_deref()))
        {
            settle(conn, z, Outcome::NamedAfterEntry, &mut out);
        } else {
            named.push((z, by_name));
        }
    }
    if named.is_empty() {
        return out;
    }
    if named.len() > MAX_HASHED {
        let later: Vec<&LibraryZip> = named
            .split_off(MAX_HASHED)
            .into_iter()
            .map(|(z, _)| z)
            .collect();
        retry_all(&later, "left for the next scan", &mut out);
    }

    let names: Vec<String> = named.iter().map(|(z, _)| z.tech_name.clone()).collect();
    let known = match catalog.known_tech_names(&names) {
        Ok(k) => k,
        Err(e) => {
            log::info!("[catalog] link backfill skipped, trying after the next scan: {e}");
            let zs: Vec<&LibraryZip> = named.iter().map(|(z, _)| *z).collect();
            retry_all(&zs, &e, &mut out);
            return out;
        }
    };
    for (z, by_name) in named {
        let outcome = if known.contains(&z.tech_name) {
            Outcome::InCatalog
        } else {
            check_bytes(conn, z, &by_name, &entries, catalog, &hash, &lock)
        };
        settle(conn, z, outcome, &mut out);
    }
    out
}

/// Hash the zip and link it when exactly one entry lists its bytes.
fn check_bytes<G>(
    conn: &Connection,
    z: &LibraryZip,
    by_name: &[&Entry],
    entries: &[Entry],
    catalog: &impl Catalog,
    hash: &impl Fn(&Path) -> Result<String, String>,
    lock: &impl Fn() -> Result<G, String>,
) -> Outcome {
    if let Err(why) = unchanged(z) {
        return Outcome::Retry(why);
    }
    let sha = match hash(&z.path) {
        Ok(h) => h.to_ascii_lowercase(),
        Err(e) => return Outcome::Retry(format!("couldn't read it: {e}")),
    };
    // Hashed as the scan saw it, not mid-change.
    if let Err(why) = unchanged(z) {
        return Outcome::Retry(why);
    }
    match catalogalias::recorded_entry(conn, &z.tech_name, &sha) {
        Ok(Some(_)) => return Outcome::AlreadyLinked,
        Ok(None) => {}
        Err(e) => return Outcome::Retry(e),
    }
    let mut matched: Vec<&Entry> = Vec::new();
    // Everything the listing has on it already: entries whose latest build is these bytes.
    for entry in entries.iter().filter(|e| e.lists_latest(&sha)) {
        add_once(&mut matched, entry);
    }
    // An entry that downloads the file may list them as an older build: the zip's own version.
    if let Some(version) = z
        .version
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
    {
        for &entry in by_name {
            if entry.lists_latest(&sha) {
                continue;
            }
            match catalog.archive_sha256(&entry.id, version) {
                Ok(Some(h)) if h.eq_ignore_ascii_case(&sha) => add_once(&mut matched, entry),
                Ok(_) => {}
                Err(e) => return Outcome::Retry(e),
            }
        }
    }
    match matched.as_slice() {
        [] => Outcome::NoMatch,
        [one] if !catalogalias::needs_alias(file_name(z), one.tech_name.as_deref()) => {
            Outcome::NamedAfterEntry
        }
        [one] => link(conn, z, &sha, one, lock),
        several => Outcome::Several(several.len()),
    }
}

/// Add `entry` to `matched` unless an entry with its id is there already.
fn add_once<'a>(matched: &mut Vec<&'a Entry>, entry: &'a Entry) {
    if !matched.iter().any(|m| m.id == entry.id) {
        matched.push(entry);
    }
}

/// Record the link an install would have, under the library lock, unless the file changed or
/// its bytes got a link meanwhile.
fn link<G>(
    conn: &Connection,
    z: &LibraryZip,
    sha: &str,
    entry: &Entry,
    lock: &impl Fn() -> Result<G, String>,
) -> Outcome {
    let _write = match lock() {
        Ok(g) => g,
        Err(e) => return Outcome::Retry(e),
    };
    if let Err(why) = unchanged(z) {
        return Outcome::Retry(why);
    }
    match catalogalias::recorded_entry(conn, &z.tech_name, sha) {
        Ok(Some(_)) => return Outcome::AlreadyLinked,
        Ok(None) => {}
        Err(e) => return Outcome::Retry(e),
    }
    let link = Link {
        catalog_id: entry.id.clone(),
        catalog_tech_name: entry.tech_name.clone(),
        download_url: entry.download_named(file_name(z)).map(str::to_string),
    };
    match catalogalias::record(conn, &z.tech_name, sha, z.size, &link) {
        Ok(_) => Outcome::Linked(entry.id.clone()),
        Err(e) => Outcome::Retry(e),
    }
}

/// Note what was made of `z` (a decided outcome is kept for this version of the file) and log it.
fn settle(conn: &Connection, z: &LibraryZip, outcome: Outcome, out: &mut Vec<(PathBuf, Outcome)>) {
    let label = file_name(z);
    match &outcome {
        Outcome::Linked(id) => log::info!("[catalog] {label} linked to catalog entry {id}"),
        Outcome::Retry(why) => {
            log::info!("[catalog] {label}: no catalog link yet, trying after the next scan ({why})")
        }
        Outcome::Several(n) => {
            log::info!("[catalog] {label}: {n} catalog entries list these bytes; not linked")
        }
        other => log::debug!("[catalog] {label}: not linked ({other:?})"),
    }
    if outcome.decided() {
        if let Err(e) = db::mark_alias_checked(conn, &path_key(z), z.size, z.mtime_ms) {
            log::warn!("[catalog] {label}: couldn't note it was checked, so it will be again: {e}");
        }
    }
    out.push((z.path.clone(), outcome));
}

fn retry_all(zips: &[&LibraryZip], why: &str, out: &mut Vec<(PathBuf, Outcome)>) {
    out.extend(
        zips.iter()
            .map(|z| (z.path.clone(), Outcome::Retry(why.to_string()))),
    );
}

/// The file still has the size and modified time the scan saw.
fn unchanged(z: &LibraryZip) -> Result<(), String> {
    let meta = std::fs::metadata(&z.path).map_err(|e| format!("couldn't read it: {e}"))?;
    let mtime_ms = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    if meta.is_file() && meta.len() == z.size && mtime_ms == z.mtime_ms {
        Ok(())
    } else {
        Err("it changed since the scan".to_string())
    }
}

/// The key the scan cache uses for the file.
fn path_key(z: &LibraryZip) -> String {
    z.path.to_string_lossy().into_owned()
}

fn file_name(z: &LibraryZip) -> &str {
    z.path.file_name().and_then(|n| n.to_str()).unwrap_or("")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::HashMap;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "silo_aliasbackfill_{tag}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
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

    /// `FS25_ToDo`, downloading `FS25_FieldToDoList.zip`, with `latest` as its latest build.
    fn todo(latest: Option<&[u8]>) -> Entry {
        Entry {
            id: "cat-todo".into(),
            tech_name: Some("FS25_ToDo".into()),
            archive_sha256: latest.map(sha),
            download_urls: vec![TODO_URL.into()],
        }
    }

    /// A fake catalog that counts what it's asked and can be told to fail.
    #[derive(Default)]
    struct Fake {
        entries: Vec<Entry>,
        known: HashSet<String>,
        /// (id, version) to the hash listed for that build.
        builds: HashMap<(String, String), String>,
        fail_entries: bool,
        fail_known: bool,
        fail_builds: bool,
        asked_entries: Cell<usize>,
        asked_known: Cell<usize>,
        asked_builds: RefCell<Vec<(String, String)>>,
    }

    impl Fake {
        fn with(entries: Vec<Entry>) -> Self {
            Fake {
                entries,
                ..Fake::default()
            }
        }
        fn build(mut self, id: &str, version: &str, bytes: &[u8]) -> Self {
            self.builds
                .insert((id.into(), version.into()), sha(bytes).to_uppercase());
            self
        }
        fn asked(&self) -> usize {
            self.asked_entries.get() + self.asked_known.get() + self.asked_builds.borrow().len()
        }
    }

    impl Catalog for Fake {
        fn entries(&self) -> Result<Vec<Entry>, String> {
            self.asked_entries.set(self.asked_entries.get() + 1);
            if self.fail_entries {
                return Err("Silo can't reach its catalog right now".into());
            }
            Ok(self.entries.clone())
        }
        fn known_tech_names(&self, names: &[String]) -> Result<HashSet<String>, String> {
            self.asked_known.set(self.asked_known.get() + 1);
            if self.fail_known {
                return Err("offline".into());
            }
            Ok(names
                .iter()
                .filter(|n| self.known.contains(*n))
                .cloned()
                .collect())
        }
        fn archive_sha256(&self, id: &str, version: &str) -> Result<Option<String>, String> {
            self.asked_builds
                .borrow_mut()
                .push((id.into(), version.into()));
            if self.fail_builds {
                return Err("offline".into());
            }
            Ok(self.builds.get(&(id.into(), version.into())).cloned())
        }
    }

    /// The library zip at `path` as a scan sees it now.
    fn scanned(path: &Path, version: Option<&str>) -> LibraryZip {
        let meta = std::fs::metadata(path).unwrap();
        LibraryZip {
            path: path.to_path_buf(),
            tech_name: catalogalias::zip_tech_name(path).unwrap(),
            size: meta.len(),
            mtime_ms: meta
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            version: version.map(str::to_string),
        }
    }

    fn write_zip(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    /// Run the backfill with real hashing (counted) and a free library lock.
    fn backfill(
        conn: &Connection,
        zips: &[LibraryZip],
        catalog: &Fake,
        hashed: &Cell<usize>,
    ) -> Vec<(PathBuf, Outcome)> {
        let hash = |p: &Path| {
            hashed.set(hashed.get() + 1);
            crate::provenance::sha256_file(p)
        };
        run(conn, zips, catalog, hash, || Ok(()))
    }

    fn setup(tag: &str) -> (PathBuf, Connection) {
        let dir = temp_dir(tag);
        let conn = db::open(&dir.join("silo.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn one_entry_listing_the_bytes_links_the_zip_for_the_drawer_and_update_joins() {
        let (dir, conn) = setup("one");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file, Some("0.1.0.17"))], &fake, &hashed);
        assert_eq!(out, [(file.clone(), Outcome::Linked("cat-todo".into()))]);
        assert_eq!(hashed.get(), 1);
        // The drawer, update checks and sharing join through `link_for_file`; Verify integrity
        // through `link_for_hash`. Both find the entry, with the row an install would write.
        let want = Link {
            catalog_id: "cat-todo".into(),
            catalog_tech_name: Some("FS25_ToDo".into()),
            download_url: Some(TODO_URL.into()),
        };
        assert_eq!(
            catalogalias::link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(want.clone())
        );
        assert_eq!(
            catalogalias::link_for_hash(&conn, "FS25_FieldToDoList", &file, &sha(b"build one"))
                .unwrap(),
            Some(want)
        );
        let rows = db::catalog_aliases(&conn, "FS25_FieldToDoList").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].size, 9);
        // The latest build was on the listing: no manifest was asked for.
        assert!(fake.asked_builds.borrow().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_older_build_is_found_by_its_own_version() {
        let (dir, conn) = setup("older");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let fake =
            Fake::with(vec![todo(Some(b"build two"))]).build("cat-todo", "0.1.0.16", b"build one");
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file, Some(" 0.1.0.16 "))], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        assert_eq!(
            *fake.asked_builds.borrow(),
            [("cat-todo".to_string(), "0.1.0.16".to_string())]
        );
        assert!(
            catalogalias::link_for_file(&conn, "FS25_FieldToDoList", &file)
                .unwrap()
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_entry_listing_the_bytes_records_nothing() {
        let (dir, conn) = setup("none");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"edited by hand");
        let fake =
            Fake::with(vec![todo(Some(b"build one"))]).build("cat-todo", "0.1.0.17", b"build one");
        let out = backfill(
            &conn,
            &[scanned(&file, Some("0.1.0.17"))],
            &fake,
            &Cell::new(0),
        );
        assert_eq!(out[0].1, Outcome::NoMatch);
        assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        // No version to ask for an older build with: only the latest is compared.
        let file2 = write_zip(&dir, "FS25_ProductionOverflow.zip", b"x");
        let overflow = Entry {
            id: "cat-apm".into(),
            tech_name: Some("FS25_AnimalProductsManager".into()),
            archive_sha256: None,
            download_urls: vec![
                "https://github.com/m/r/releases/download/v1/FS25_ProductionOverflow.zip".into(),
            ],
        };
        let fake = Fake::with(vec![overflow]);
        let out = backfill(&conn, &[scanned(&file2, None)], &fake, &Cell::new(0));
        assert_eq!(out[0].1, Outcome::NoMatch);
        assert!(fake.asked_builds.borrow().is_empty());
        assert!(!catalogalias::has_any(&conn, "FS25_ProductionOverflow").unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_entries_listing_the_bytes_record_nothing() {
        let (dir, conn) = setup("several");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        // A fork downloading the same file lists the same build, by version; or an unrelated
        // entry has the same bytes as its latest build.
        let fork = Entry {
            id: "cat-fork".into(),
            tech_name: Some("FS25_ToDoFork".into()),
            archive_sha256: None,
            download_urls: vec![
                "https://github.com/someone/FS25_ToDo/releases/download/v1/FS25_FieldToDoList.zip"
                    .into(),
            ],
        };
        let other = Entry {
            id: "cat-other".into(),
            tech_name: Some("FS25_Other".into()),
            archive_sha256: Some(sha(b"build one")),
            download_urls: vec![],
        };
        for entries in [
            vec![todo(Some(b"build one")), fork.clone()],
            vec![todo(Some(b"build one")), other],
        ] {
            let fake = Fake::with(entries).build("cat-fork", "1.0", b"build one");
            let out = backfill(&conn, &[scanned(&file, Some("1.0"))], &fake, &Cell::new(0));
            assert_eq!(out[0].1, Outcome::Several(2));
            assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
            conn.execute("DELETE FROM alias_check", []).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unchanged_file_is_not_hashed_or_looked_up_again() {
        let (dir, conn) = setup("unchanged");
        let linked = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let unmatched = write_zip(&dir, "FS25_StatsExtended_update.zip", b"not listed");
        let plain = write_zip(&dir, "FS25_Plain.zip", b"plain");
        let stats = Entry {
            id: "cat-stats".into(),
            tech_name: Some("FS25_statsExtended".into()),
            archive_sha256: Some(sha(b"listed")),
            download_urls: vec![
                "https://github.com/e/r/releases/download/1/FS25_StatsExtended_update.zip".into(),
            ],
        };
        let fake = Fake::with(vec![todo(Some(b"build one")), stats]);
        let zips = [
            scanned(&linked, Some("1")),
            scanned(&unmatched, Some("1")),
            scanned(&plain, Some("1")),
        ];
        let hashed = Cell::new(0);
        let first = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(first.len(), 3);
        assert_eq!(hashed.get(), 2);
        let asked = fake.asked();
        let again = backfill(&conn, &zips, &fake, &hashed);
        assert!(again.is_empty(), "{again:?}");
        assert_eq!(hashed.get(), 2, "nothing hashed again");
        assert_eq!(fake.asked(), asked, "nothing looked up again");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_changed_file_is_checked_again() {
        let (dir, conn) = setup("changed");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"a build nobody lists");
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file, None)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::NoMatch);
        // The user puts the catalog's build back: another size, another time.
        std::fs::write(&file, b"build one").unwrap();
        let out = backfill(&conn, &[scanned(&file, None)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        assert_eq!(hashed.get(), 2);
        assert!(
            catalogalias::link_for_file(&conn, "FS25_FieldToDoList", &file)
                .unwrap()
                .is_some()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_lookup_records_nothing_and_is_tried_after_the_next_scan() {
        let (dir, conn) = setup("offline");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let zips = [scanned(&file, Some("0.1.0.16"))];
        let hashed = Cell::new(0);
        // The listing, the techName lookup, or the manifest fails: no record, nothing noted.
        let offline = [
            Fake {
                fail_entries: true,
                ..Fake::with(vec![todo(Some(b"build one"))])
            },
            Fake {
                fail_known: true,
                ..Fake::with(vec![todo(Some(b"build one"))])
            },
            Fake {
                fail_builds: true,
                ..Fake::with(vec![todo(Some(b"build two"))])
            },
        ];
        for fake in &offline {
            let out = backfill(&conn, &zips, fake, &hashed);
            assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
            assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
            let z = &zips[0];
            assert!(!db::alias_checked(&conn, &path_key(z), z.size, z.mtime_ms).unwrap());
        }
        // Back online: the same file version is looked at and linked.
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_named_after_its_entry_or_known_by_its_tech_name_is_skipped_unhashed() {
        let (dir, conn) = setup("named");
        // The entry's file is named after it: the techName join finds it already.
        let own = write_zip(&dir, "FS25_ToDo.zip", b"build one");
        let entry = Entry {
            download_urls: vec![
                "https://github.com/knoellix/FS25_ToDo/releases/download/v1/FS25_ToDo.zip".into(),
            ],
            ..todo(Some(b"build one"))
        };
        let fake = Fake::with(vec![entry]);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&own, None)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::NamedAfterEntry);
        // Another entry has the zip's techName: it's joined by that, so no link.
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let mut fake = Fake::with(vec![todo(Some(b"build one"))]);
        fake.known.insert("FS25_FieldToDoList".into());
        let out = backfill(&conn, &[scanned(&file, None)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::InCatalog);
        // No entry downloads a file of its name: settled without a hash or a lookup by name.
        let other = write_zip(&dir, "FS25_Unlisted.zip", b"build one");
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let out = backfill(&conn, &[scanned(&other, None)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::NoCandidate);
        assert_eq!(fake.asked_known.get(), 0);
        assert_eq!(hashed.get(), 0, "none of them was hashed");
        assert!(!catalogalias::has_any(&conn, "FS25_ToDo").unwrap());
        assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        assert!(!catalogalias::has_any(&conn, "FS25_Unlisted").unwrap());
        // A zip the library knows by another name isn't looked at at all.
        let renamed = LibraryZip {
            tech_name: "FS25_Else".into(),
            ..scanned(&file, None)
        };
        conn.execute("DELETE FROM alias_check", []).unwrap();
        assert!(backfill(&conn, &[renamed], &fake, &hashed).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn bytes_already_linked_keep_their_link() {
        let (dir, conn) = setup("already");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let installed = Link {
            catalog_id: "cat-installed".into(),
            catalog_tech_name: Some("FS25_ToDo".into()),
            download_url: None,
        };
        catalogalias::record(
            &conn,
            "FS25_FieldToDoList",
            &sha(b"build one"),
            9,
            &installed,
        )
        .unwrap();
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let out = backfill(&conn, &[scanned(&file, None)], &fake, &Cell::new(0));
        assert_eq!(out[0].1, Outcome::AlreadyLinked);
        assert_eq!(
            catalogalias::link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(installed)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_busy_library_or_a_file_changed_since_the_scan_waits_for_the_next_scan() {
        let (dir, conn) = setup("busy");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let zip = scanned(&file, None);
        let fake = Fake::with(vec![todo(Some(b"build one"))]);
        let hash = crate::provenance::sha256_file;
        let busy = || -> Result<(), String> { Err("Silo is busy".into()) };
        let out = run(&conn, std::slice::from_ref(&zip), &fake, hash, busy);
        assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
        let stale = LibraryZip {
            size: zip.size + 1,
            ..zip.clone()
        };
        let out = run(&conn, &[stale], &fake, hash, || Ok(()));
        assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
        let unreadable = |_: &Path| -> Result<String, String> { Err("in use".into()) };
        let out = run(&conn, std::slice::from_ref(&zip), &fake, unreadable, || {
            Ok(())
        });
        assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
        assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        // Each is looked at again, and linked once nothing is in the way.
        let out = run(&conn, &[zip], &fake, hash, || Ok(()));
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn one_run_hashes_at_most_a_bounded_number_of_files() {
        let (dir, conn) = setup("bounded");
        let n = MAX_HASHED + 2;
        let mut entries = Vec::new();
        let mut zips = Vec::new();
        for i in 0..n {
            let name = format!("FS25_File{i}.zip");
            let file = write_zip(&dir, &name, format!("bytes {i}").as_bytes());
            zips.push(scanned(&file, None));
            entries.push(Entry {
                id: format!("cat-{i}"),
                tech_name: Some(format!("FS25_Entry{i}")),
                archive_sha256: Some(sha(format!("bytes {i}").as_bytes())),
                download_urls: vec![format!(
                    "https://github.com/o/r/releases/download/v1/{name}"
                )],
            });
        }
        let fake = Fake::with(entries);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(hashed.get(), MAX_HASHED);
        let linked = out
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Linked(_)))
            .count();
        assert_eq!(linked, MAX_HASHED);
        // The rest are linked after the next scan.
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, o)| matches!(o, Outcome::Linked(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_entry_converts_from_a_catalog_row() {
        let m: siloapi::BrowseMod = serde_json::from_value(serde_json::json!({
            "id": "cat-todo", "techName": "FS25_ToDo", "title": "FS25_ToDo",
            "archiveSha256": "ABC",
            "sources": [
                {"source": "github", "sourceUrl": "https://github.com/k/r", "installable": true,
                 "downloadUrl": TODO_URL},
                {"source": "modhub", "sourceUrl": "https://www.farming-simulator.com/x",
                 "installable": false}
            ]
        }))
        .unwrap();
        let e = Entry::from(&m);
        assert_eq!(
            e,
            Entry {
                id: "cat-todo".into(),
                tech_name: Some("FS25_ToDo".into()),
                archive_sha256: Some("ABC".into()),
                download_urls: vec![TODO_URL.into()],
            }
        );
        assert_eq!(e.download_named("fs25_fieldtodolist.ZIP"), Some(TODO_URL));
        assert_eq!(e.download_named("FS25_ToDo.zip"), None);
        assert!(e.lists_latest("abc"));
    }
}
