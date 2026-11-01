//! Catalog links for zips Silo installed under another name before #388 (#402).
//!
//! Since #388 a Browse install records which catalog entry a zip came from when the file isn't
//! named after the entry's techName (`catalogalias`). A zip installed that way earlier has no
//! record, so its drawer said "Not in Silo's catalog", update checks skipped it and Verify
//! integrity said Unverified. This looks such zips up by their bytes and records the same link
//! an install would have.
//!
//! A zip is looked at when its file name is its techName (as the joins require), the catalog
//! has no entry with that techName (`POST /mods/lookup`), and its bytes have no link yet. Silo
//! hashes it and asks the catalog which entries have a build with exactly those bytes
//! (`POST /mods/by-hash`: one request for a run's files, answered from one query on the
//! server, silo-api#39). It records the link when exactly one entry has, that entry's build of
//! those bytes downloads as a file of the zip's own name (a Browse install saves the file under
//! its download's name), and the entry isn't named after the zip. Otherwise it records nothing.
//! The record is an install's (`catalogalias::record`), made under the library lock.
//!
//! When the catalog has no `by-hash` route yet (a SiloAPI from before it), nothing is decided;
//! that's logged once per session, and the files are looked at once it has one.
//!
//! When it runs: it sends some mod names to the catalog, so only with the user's leave to
//! check for updates, and never for a mod left out of update checks: after a library scan when
//! the startup update check is on, and beside an update check of the library. Off the UI
//! thread, in the background, one run at a time (a run asked for while one is going is
//! skipped; a later one takes its files). The window hears which mods were linked
//! (`catalog:linked`), and the drawer and the Updates list ask again.
//!
//! Each version of a file (its path, size and modified time, as the scan cache keys it) is
//! decided once: then it isn't hashed or looked up again until the file changes. A failed or
//! offline lookup, a busy library, or a file that can't be read or changes meanwhile decides
//! nothing, so that file is tried again next time, behind files not tried yet; a hash already
//! taken is kept for it. A run stops at its `Limits` (files hashed, time); the rest wait for the
//! next run. Nothing here is shown to the user as an error; it's only logged.

use crate::catalogalias::{self, Link};
use crate::db;
use crate::siloapi::{self, HashMatch};
use rusqlite::Connection;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The most files one run hashes and looks up.
pub const MAX_HASHED: usize = 25;

/// How much one run may do: files hashed, and time (checked before each file is hashed, so a
/// run ends within about one file's hashing plus its one lookup).
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_hashed: usize,
    pub time: std::time::Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_hashed: MAX_HASHED,
            time: std::time::Duration::from_secs(60),
        }
    }
}

/// A zip from a library scan.
#[derive(Debug, Clone)]
pub struct LibraryZip {
    pub path: PathBuf,
    /// The techName the library knows it by (its file name without `.zip`).
    pub tech_name: String,
    /// The size and modified time the scan saw.
    pub size: u64,
    pub mtime_ms: u64,
}

/// What the backfill asks the catalog.
pub trait Catalog {
    /// Which of `names` are an entry's techName, exactly.
    fn known_tech_names(&self, names: &[String]) -> Result<HashSet<String>, String>;
    /// Every catalog build with one of these whole-zip SHA-256s (lowercase), by hash, every hash
    /// asked present; `Ok(None)` when the catalog can't answer this (no such route).
    fn by_hash(&self, hashes: &[String])
        -> Result<Option<HashMap<String, Vec<HashMatch>>>, String>;
}

/// The catalog at `base`.
pub struct SiloApi {
    pub base: String,
}

impl Catalog for SiloApi {
    fn known_tech_names(&self, names: &[String]) -> Result<HashSet<String>, String> {
        Ok(siloapi::lookup(&self.base, names)?
            .into_iter()
            .filter_map(|r| r.tech_name)
            .collect())
    }

    fn by_hash(
        &self,
        hashes: &[String],
    ) -> Result<Option<HashMap<String, Vec<HashMatch>>>, String> {
        siloapi::by_hash(&self.base, hashes)
    }
}

/// What the backfill made of one zip.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// Linked to this catalog entry.
    Linked(String),
    /// The catalog has an entry with its techName.
    InCatalog,
    /// Its bytes already have a link.
    AlreadyLinked,
    /// No entry has a build with its bytes.
    NoMatch,
    /// This many entries have a build with its bytes, so it can't be told which it came from.
    Several(usize),
    /// The one entry with its bytes downloads them under another file name, or is named after
    /// the zip: not what a Browse install of this file was.
    OtherName,
    /// Nothing was decided (why); it's looked at again next time.
    Retry(String),
}

impl Outcome {
    /// Whether this settles the file's current version.
    fn decided(&self) -> bool {
        !matches!(self, Outcome::Retry(_))
    }
}

/// Set once it was logged that the catalog can't look up by hash yet (once a session).
static NO_ROUTE_LOGGED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Why files wait when the catalog can't look up by hash yet.
const NO_ROUTE: &str = "Silo's catalog can't look mods up by their files yet";

/// Why files wait when a run has done all it may.
const BUDGET_USED: &str = "this run has done all it may; the next one goes on";

/// A zip to look at, with what was noted about this version of it on an earlier try.
struct Pending<'a> {
    zip: &'a LibraryZip,
    noted: Option<db::AliasCheck>,
}

/// Look up the zips in `zips` that need it (see the module notes) and record the links found,
/// within the default `Limits`. `hash` gives a file's whole-file SHA-256; `lock` takes the
/// library lock for a record. Never fails: what couldn't be done is logged and tried again on a
/// later run. Returns what it made of each zip it looked at.
pub fn run<G>(
    conn: &Connection,
    zips: &[LibraryZip],
    catalog: &impl Catalog,
    hash: impl Fn(&Path) -> Result<String, String>,
    lock: impl Fn() -> Result<G, String>,
) -> Vec<(PathBuf, Outcome)> {
    run_with(conn, zips, catalog, hash, lock, Limits::default())
}

/// `run` within `limits`.
pub fn run_with<G>(
    conn: &Connection,
    zips: &[LibraryZip],
    catalog: &impl Catalog,
    hash: impl Fn(&Path) -> Result<String, String>,
    lock: impl Fn() -> Result<G, String>,
    limits: Limits,
) -> Vec<(PathBuf, Outcome)> {
    let until = std::time::Instant::now() + limits.time;
    let mut out = Vec::new();
    // Mods the user left out of update checks (#333) aren't asked about either.
    let skipped = match db::skip_update_names(conn) {
        Ok(s) => s,
        Err(e) => {
            log::warn!(
                "[catalog] link backfill skipped: couldn't read which mods to leave out: {e}"
            );
            return out;
        }
    };
    let mut pending = Vec::new();
    for z in zips {
        if catalogalias::zip_tech_name(&z.path).as_deref() != Some(z.tech_name.as_str())
            || skipped.contains(&z.tech_name)
        {
            continue;
        }
        let noted = match db::alias_check(conn, &path_key(z)) {
            Ok(n) => n.filter(|n| n.size == z.size && n.mtime_ms == z.mtime_ms),
            Err(e) => {
                log::warn!("[catalog] link backfill skipped: couldn't read what it checked: {e}");
                return out;
            }
        };
        if !noted.as_ref().is_some_and(|n| n.decided) {
            pending.push(Pending { zip: z, noted });
        }
    }
    if pending.is_empty() {
        return out;
    }
    // Files never tried first, then the ones that have waited longest, so a file that keeps
    // failing never holds up the rest.
    pending.sort_by_key(|p| p.noted.as_ref().map(|n| n.attempted_ms));

    // A zip the catalog knows by its own name is joined by that and needs no link.
    let names: Vec<String> = pending.iter().map(|p| p.zip.tech_name.clone()).collect();
    let known = match catalog.known_tech_names(&names) {
        Ok(k) => k,
        Err(e) => {
            log::info!("[catalog] link backfill skipped, trying next time: {e}");
            retry_all(pending.iter().map(|p| p.zip), &e, &mut out);
            return out;
        }
    };
    let mut asked: Vec<(&LibraryZip, String)> = Vec::new();
    // Files hashed (or tried) this run, read or not.
    let mut tried = 0;
    for p in &pending {
        if known.contains(&p.zip.tech_name) {
            settle(conn, p.zip, Outcome::InCatalog, None, &mut out);
        } else if tried >= limits.max_hashed || std::time::Instant::now() >= until {
            out.push((p.zip.path.clone(), Outcome::Retry(BUDGET_USED.to_string())));
        } else {
            tried += 1;
            match bytes_of(conn, p, &hash) {
                Ok(sha) => asked.push((p.zip, sha)),
                Err(why) => settle(conn, p.zip, Outcome::Retry(why), None, &mut out),
            }
        }
    }
    if asked.is_empty() {
        return out;
    }

    let hashes: Vec<String> = asked.iter().map(|(_, sha)| sha.clone()).collect();
    let found = match catalog.by_hash(&hashes) {
        Ok(Some(found)) => found,
        Ok(None) => {
            if !NO_ROUTE_LOGGED.swap(true, std::sync::atomic::Ordering::SeqCst) {
                log::info!(
                    "[catalog] {NO_ROUTE}; older installs get their catalog links once it can"
                );
            }
            retry_all(asked.iter().map(|(z, _)| *z), NO_ROUTE, &mut out);
            return out;
        }
        Err(e) => {
            log::info!("[catalog] link backfill skipped, trying next time: {e}");
            retry_all(asked.iter().map(|(z, _)| *z), &e, &mut out);
            return out;
        }
    };
    for (z, sha) in asked {
        let outcome = match found.get(&sha) {
            Some(matches) => judge(conn, z, &sha, matches, &lock),
            None => Outcome::Retry("Silo's catalog left this file out of its answer".to_string()),
        };
        settle(conn, z, outcome, Some(&sha), &mut out);
    }
    out
}

/// The zip's whole-file SHA-256 for the version the scan saw: the one noted on an earlier try,
/// or hashed now. Every try is noted (with the hash once taken, so a later failure doesn't hash
/// the file again), so a file that keeps failing waits behind files not tried yet.
fn bytes_of(
    conn: &Connection,
    p: &Pending,
    hash: &impl Fn(&Path) -> Result<String, String>,
) -> Result<String, String> {
    let z = p.zip;
    let noted = p.noted.as_ref().and_then(|n| n.sha256.clone());
    if let Err(why) = unchanged(z) {
        note(conn, z, noted.as_deref(), false);
        return Err(why);
    }
    if let Some(sha) = noted {
        note(conn, z, Some(&sha), false);
        return Ok(sha);
    }
    let taken = hash(&z.path).map(|h| h.to_ascii_lowercase());
    // Hashed as the scan saw it, not mid-change.
    if let Err(why) = unchanged(z) {
        note(conn, z, None, false);
        return Err(why);
    }
    note(conn, z, taken.as_ref().ok().map(String::as_str), false);
    taken.map_err(|e| format!("couldn't read it: {e}"))
}

/// What the catalog's builds with the bytes `sha` of the zip `z` make of it, linking it when
/// they show where it came from.
fn judge<G>(
    conn: &Connection,
    z: &LibraryZip,
    sha: &str,
    matches: &[HashMatch],
    lock: &impl Fn() -> Result<G, String>,
) -> Outcome {
    match catalogalias::recorded_entry(conn, &z.tech_name, sha) {
        Ok(Some(_)) => return Outcome::AlreadyLinked,
        Ok(None) => {}
        Err(e) => return Outcome::Retry(e),
    }
    let mut ids: Vec<&str> = matches.iter().map(|m| m.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    let id = match ids.as_slice() {
        [] => return Outcome::NoMatch,
        [one] => *one,
        several => return Outcome::Several(several.len()),
    };
    let file = file_name(z);
    // The entry's build of these bytes that downloads as this file, as a Browse install saved it.
    let Some(build) = matches
        .iter()
        .find(|m| m.file_name.as_deref() == Some(file))
    else {
        return Outcome::OtherName;
    };
    if !catalogalias::needs_alias(file, build.tech_name.as_deref()) {
        return Outcome::OtherName;
    }
    link(conn, z, sha, id, build, lock)
}

/// Record the link an install would have, under the library lock, unless the file changed or
/// its bytes got a link meanwhile.
fn link<G>(
    conn: &Connection,
    z: &LibraryZip,
    sha: &str,
    id: &str,
    build: &HashMatch,
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
        catalog_id: id.to_string(),
        catalog_tech_name: build.tech_name.clone(),
        download_url: build.download_url.clone(),
    };
    match catalogalias::record(conn, &z.tech_name, sha, z.size, &link) {
        Ok(_) => Outcome::Linked(id.to_string()),
        Err(e) => Outcome::Retry(e),
    }
}

/// Log what was made of `z`, and keep a decided outcome (with the hash, when taken) for this
/// version of the file.
fn settle(
    conn: &Connection,
    z: &LibraryZip,
    outcome: Outcome,
    sha: Option<&str>,
    out: &mut Vec<(PathBuf, Outcome)>,
) {
    let label = file_name(z);
    match &outcome {
        Outcome::Linked(id) => log::info!("[catalog] {label} linked to catalog entry {id}"),
        Outcome::Retry(why) => {
            log::info!("[catalog] {label}: no catalog link yet, trying next time ({why})")
        }
        Outcome::Several(n) => {
            log::info!("[catalog] {label}: {n} catalog entries have these bytes; not linked")
        }
        other => log::debug!("[catalog] {label}: not linked ({other:?})"),
    }
    if outcome.decided() {
        note(conn, z, sha, true);
    }
    out.push((z.path.clone(), outcome));
}

/// Save what was noted about this version of `z`, now. A failure only means it's looked at again.
fn note(conn: &Connection, z: &LibraryZip, sha: Option<&str>, decided: bool) {
    let check = db::AliasCheck {
        size: z.size,
        mtime_ms: z.mtime_ms,
        sha256: sha.map(str::to_string),
        decided,
        attempted_ms: now_ms(),
    };
    if let Err(e) = db::note_alias_check(conn, &path_key(z), &check) {
        log::warn!(
            "[catalog] {}: couldn't note it was checked, so it will be again: {e}",
            file_name(z)
        );
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn retry_all<'a>(
    zips: impl Iterator<Item = &'a LibraryZip>,
    why: &str,
    out: &mut Vec<(PathBuf, Outcome)>,
) {
    out.extend(zips.map(|z| (z.path.clone(), Outcome::Retry(why.to_string()))));
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

    /// `FS25_ToDo`'s build that downloads as `FS25_FieldToDoList.zip`.
    fn todo_build() -> HashMatch {
        HashMatch {
            id: "cat-todo".into(),
            tech_name: Some("FS25_ToDo".into()),
            version: "v0.1.0.17".into(),
            source: "github".into(),
            file_name: Some("FS25_FieldToDoList.zip".into()),
            download_url: Some(TODO_URL.into()),
        }
    }

    /// A fake catalog: the builds by hash and the techNames it knows. It counts what it's asked
    /// and can be told to fail, or to have no by-hash route.
    #[derive(Default)]
    struct Fake {
        builds: HashMap<String, Vec<HashMatch>>,
        known: HashSet<String>,
        fail_known: bool,
        fail_by_hash: bool,
        no_route: bool,
        asked_known: Cell<usize>,
        asked_by_hash: RefCell<Vec<Vec<String>>>,
    }

    impl Fake {
        fn with(bytes: &[u8], builds: Vec<HashMatch>) -> Self {
            Fake::default().and(bytes, builds)
        }
        fn and(mut self, bytes: &[u8], builds: Vec<HashMatch>) -> Self {
            self.builds.insert(sha(bytes), builds);
            self
        }
        fn asked(&self) -> usize {
            self.asked_known.get() + self.asked_by_hash.borrow().len()
        }
    }

    impl Catalog for Fake {
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
        fn by_hash(
            &self,
            hashes: &[String],
        ) -> Result<Option<HashMap<String, Vec<HashMatch>>>, String> {
            self.asked_by_hash.borrow_mut().push(hashes.to_vec());
            if self.fail_by_hash {
                return Err("Silo can't reach its catalog right now".into());
            }
            if self.no_route {
                return Ok(None);
            }
            Ok(Some(
                hashes
                    .iter()
                    .map(|h| (h.clone(), self.builds.get(h).cloned().unwrap_or_default()))
                    .collect(),
            ))
        }
    }

    /// The library zip at `path` as a scan sees it now.
    fn scanned(path: &Path) -> LibraryZip {
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

    /// Whether this version of the file is settled.
    fn settled(conn: &Connection, z: &LibraryZip) -> bool {
        db::alias_check(conn, &path_key(z))
            .unwrap()
            .is_some_and(|n| n.decided && n.size == z.size && n.mtime_ms == z.mtime_ms)
    }

    #[test]
    fn one_entry_with_the_bytes_links_the_zip_for_the_drawer_update_and_verify_joins() {
        let (dir, conn) = setup("one");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        // The same entry may have the bytes under two builds (re-tagged): still one entry.
        let retagged = HashMatch {
            version: "v0.1.0.17b".into(),
            ..todo_build()
        };
        let fake = Fake::with(b"build one", vec![todo_build(), retagged]);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file)], &fake, &hashed);
        assert_eq!(out, [(file.clone(), Outcome::Linked("cat-todo".into()))]);
        assert_eq!(hashed.get(), 1);
        assert_eq!(*fake.asked_by_hash.borrow(), [vec![sha(b"build one")]]);
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
        assert_eq!(rows[0].size, 9);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_entry_or_several_with_the_bytes_record_nothing() {
        let (dir, conn) = setup("none_several");
        let none = write_zip(&dir, "FS25_FieldToDoList.zip", b"edited by hand");
        let both = write_zip(&dir, "FS25_StatsExtended_update.zip", b"shared build");
        let fork = HashMatch {
            id: "cat-fork".into(),
            tech_name: Some("FS25_StatsFork".into()),
            file_name: Some("FS25_StatsExtended_update.zip".into()),
            ..todo_build()
        };
        let stats = HashMatch {
            id: "cat-stats".into(),
            tech_name: Some("FS25_statsExtended".into()),
            ..fork.clone()
        };
        let fake = Fake::with(b"shared build", vec![stats, fork]);
        let zips = [scanned(&none), scanned(&both)];
        let out = backfill(&conn, &zips, &fake, &Cell::new(0));
        assert_eq!(out[0].1, Outcome::NoMatch);
        assert_eq!(out[1].1, Outcome::Several(2));
        // Both looked up in one request.
        assert_eq!(fake.asked_by_hash.borrow().len(), 1);
        assert_eq!(fake.asked_by_hash.borrow()[0].len(), 2);
        assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        assert!(!catalogalias::has_any(&conn, "FS25_StatsExtended_update").unwrap());
        assert!(zips.iter().all(|z| settled(&conn, z)));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_one_entry_must_have_shipped_the_bytes_under_the_zips_name() {
        let (dir, conn) = setup("other_name");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        // The entry's build of these bytes downloads as another file, or as nothing known.
        let elsewhere = HashMatch {
            file_name: Some("FS25_ToDo_v17.zip".into()),
            ..todo_build()
        };
        let unnamed = HashMatch {
            file_name: None,
            ..todo_build()
        };
        for builds in [vec![elsewhere], vec![unnamed]] {
            let fake = Fake::with(b"build one", builds);
            let out = backfill(&conn, &[scanned(&file)], &fake, &Cell::new(0));
            assert_eq!(out[0].1, Outcome::OtherName);
            assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
            conn.execute("DELETE FROM alias_check", []).unwrap();
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_file_named_after_its_entry_or_known_by_its_tech_name_is_skipped() {
        let (dir, conn) = setup("named");
        let file = write_zip(&dir, "FS25_ToDo.zip", b"build one");
        let mut fake = Fake::with(b"build one", vec![todo_build()]);
        fake.known.insert("FS25_ToDo".into());
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::InCatalog);
        assert_eq!(hashed.get(), 0, "not hashed");
        assert!(fake.asked_by_hash.borrow().is_empty());
        // Named after its entry, though the catalog's exact lookup missed it: the entry's build
        // of its bytes carries the zip's own name, so it needs no alias.
        let file = write_zip(&dir, "fs25_todo.zip", b"build two");
        let own = HashMatch {
            tech_name: Some("fs25_todo".into()),
            file_name: Some("fs25_todo.zip".into()),
            ..todo_build()
        };
        let fake = Fake::with(b"build two", vec![own]);
        let out = backfill(&conn, &[scanned(&file)], &fake, &Cell::new(0));
        assert_eq!(out[0].1, Outcome::OtherName);
        assert!(!catalogalias::has_any(&conn, "fs25_todo").unwrap());
        // A zip the library knows by another name isn't looked at at all.
        let renamed = LibraryZip {
            tech_name: "FS25_Else".into(),
            ..scanned(&file)
        };
        let fake = Fake::default();
        assert!(backfill(&conn, &[renamed], &fake, &Cell::new(0)).is_empty());
        assert_eq!(fake.asked(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unchanged_file_is_not_hashed_or_looked_up_again() {
        let (dir, conn) = setup("unchanged");
        let linked = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let unmatched = write_zip(&dir, "FS25_Unlisted.zip", b"not listed");
        let known = write_zip(&dir, "FS25_Known.zip", b"known");
        let mut fake = Fake::with(b"build one", vec![todo_build()]);
        fake.known.insert("FS25_Known".into());
        let zips = [scanned(&linked), scanned(&unmatched), scanned(&known)];
        let hashed = Cell::new(0);
        assert_eq!(backfill(&conn, &zips, &fake, &hashed).len(), 3);
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
        let fake = Fake::with(b"build one", vec![todo_build()]);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &[scanned(&file)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::NoMatch);
        // The user puts the catalog's build back: another size, another time.
        std::fs::write(&file, b"build one").unwrap();
        let out = backfill(&conn, &[scanned(&file)], &fake, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        assert_eq!(hashed.get(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_failed_lookup_records_nothing_and_retries_without_hashing_again() {
        let (dir, conn) = setup("offline");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let zips = [scanned(&file)];
        let hashed = Cell::new(0);
        let no_names = Fake {
            fail_known: true,
            ..Fake::with(b"build one", vec![todo_build()])
        };
        let no_hashes = Fake {
            fail_by_hash: true,
            ..Fake::with(b"build one", vec![todo_build()])
        };
        for fake in [&no_names, &no_hashes, &no_hashes] {
            let out = backfill(&conn, &zips, fake, &hashed);
            assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
            assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
            assert!(!settled(&conn, &zips[0]));
        }
        assert_eq!(hashed.get(), 1, "hashed once, then the noted hash is used");
        // Back online: the same file version is linked, still without hashing it again.
        let fake = Fake::with(b"build one", vec![todo_build()]);
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        assert_eq!(hashed.get(), 1);
        // A new build of the file is hashed afresh, never judged by the old hash.
        std::fs::write(&file, b"build three!").unwrap();
        backfill(&conn, &[scanned(&file)], &no_hashes, &hashed);
        assert_eq!(hashed.get(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_catalog_without_the_route_decides_nothing_until_it_has_it() {
        let (dir, conn) = setup("no_route");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let zips = [scanned(&file)];
        let old = Fake {
            no_route: true,
            ..Fake::with(b"build one", vec![todo_build()])
        };
        let hashed = Cell::new(0);
        for _ in 0..2 {
            let out = backfill(&conn, &zips, &old, &hashed);
            assert_eq!(out, [(file.clone(), Outcome::Retry(NO_ROUTE.to_string()))]);
            assert!(!settled(&conn, &zips[0]));
            assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        }
        assert!(NO_ROUTE_LOGGED.load(std::sync::atomic::Ordering::SeqCst));
        assert_eq!(hashed.get(), 1);
        // Once the catalog has it, the file is linked.
        let new = Fake::with(b"build one", vec![todo_build()]);
        let out = backfill(&conn, &zips, &new, &hashed);
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_mod_left_out_of_update_checks_is_never_asked_about() {
        let (dir, conn) = setup("skip");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        db::set_curation(
            &conn,
            &db::CurationRow {
                tech_name: "FS25_FieldToDoList".into(),
                favorite: false,
                hidden: false,
                broken: false,
                rating: 0,
                note: None,
                skip_updates: true,
            },
        )
        .unwrap();
        let fake = Fake::with(b"build one", vec![todo_build()]);
        let hashed = Cell::new(0);
        assert!(backfill(&conn, &[scanned(&file)], &fake, &hashed).is_empty());
        assert_eq!((fake.asked(), hashed.get()), (0, 0));
        // When that can't be read, nothing is asked at all.
        conn.execute_batch("DROP TABLE curation").unwrap();
        assert!(backfill(&conn, &[scanned(&file)], &fake, &hashed).is_empty());
        assert_eq!(fake.asked(), 0);
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
        let fake = Fake::with(b"build one", vec![todo_build()]);
        let out = backfill(&conn, &[scanned(&file)], &fake, &Cell::new(0));
        assert_eq!(out[0].1, Outcome::AlreadyLinked);
        assert_eq!(
            catalogalias::link_for_file(&conn, "FS25_FieldToDoList", &file).unwrap(),
            Some(installed)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_busy_library_or_a_changed_or_unreadable_file_waits_for_the_next_run() {
        let (dir, conn) = setup("busy");
        let file = write_zip(&dir, "FS25_FieldToDoList.zip", b"build one");
        let zip = scanned(&file);
        let fake = Fake::with(b"build one", vec![todo_build()]);
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
        assert!(!catalogalias::has_any(&conn, "FS25_FieldToDoList").unwrap());
        let out = run(&conn, std::slice::from_ref(&zip), &fake, hash, || Ok(()));
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        // A file that can't be read (the game has it open, say) waits too.
        let other = dir.join("other");
        std::fs::create_dir_all(&other).unwrap();
        let file = write_zip(&other, "FS25_FieldToDoList.zip", b"build one!");
        let fake = Fake::with(b"build one!", vec![todo_build()]);
        let zip = scanned(&file);
        let unreadable = |_: &Path| -> Result<String, String> { Err("in use".into()) };
        let out = run(&conn, std::slice::from_ref(&zip), &fake, unreadable, || {
            Ok(())
        });
        assert!(matches!(out[0].1, Outcome::Retry(_)), "{out:?}");
        assert!(fake.asked_by_hash.borrow().is_empty());
        let out = run(&conn, &[zip], &fake, hash, || Ok(()));
        assert_eq!(out[0].1, Outcome::Linked("cat-todo".into()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `n` zips, each the build of its own entry under its own name, and the catalog of them.
    fn many(dir: &Path, n: usize) -> (Vec<LibraryZip>, Fake) {
        let mut fake = Fake::default();
        let mut zips = Vec::new();
        for i in 0..n {
            let name = format!("FS25_File{i:02}.zip");
            let bytes = format!("bytes {i}");
            zips.push(scanned(&write_zip(dir, &name, bytes.as_bytes())));
            fake = fake.and(
                bytes.as_bytes(),
                vec![HashMatch {
                    id: format!("cat-{i}"),
                    tech_name: Some(format!("FS25_Entry{i}")),
                    file_name: Some(name),
                    ..todo_build()
                }],
            );
        }
        (zips, fake)
    }

    #[test]
    fn one_run_hashes_a_bounded_number_and_asks_once() {
        let (dir, conn) = setup("bounded");
        let (zips, fake) = many(&dir, MAX_HASHED + 2);
        let hashed = Cell::new(0);
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(hashed.get(), MAX_HASHED);
        assert_eq!(
            fake.asked_by_hash.borrow().len(),
            1,
            "one request for the run"
        );
        assert_eq!(fake.asked_by_hash.borrow()[0].len(), MAX_HASHED);
        let linked = out
            .iter()
            .filter(|(_, o)| matches!(o, Outcome::Linked(_)))
            .count();
        assert_eq!(linked, MAX_HASHED);
        // The rest go on in the next run.
        let out = backfill(&conn, &zips, &fake, &hashed);
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|(_, o)| matches!(o, Outcome::Linked(_))));
        // No time: nothing hashed or looked up by hash, nothing settled.
        let (dir2, conn2) = setup("bounded_time");
        let (zips2, fake2) = many(&dir2, 2);
        let none = Limits {
            time: std::time::Duration::ZERO,
            ..Limits::default()
        };
        let hash = crate::provenance::sha256_file;
        let out = run_with(&conn2, &zips2, &fake2, hash, || Ok(()), none);
        assert!(
            out.iter().all(|(_, o)| matches!(o, Outcome::Retry(_))),
            "{out:?}"
        );
        assert!(fake2.asked_by_hash.borrow().is_empty());
        assert!(!zips2.iter().any(|z| settled(&conn2, z)));
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::remove_dir_all(&dir2);
    }

    #[test]
    fn files_that_keep_failing_dont_hold_up_the_rest() {
        let (dir, conn) = setup("starve");
        let (mut zips, fake) = many(&dir, MAX_HASHED + 1);
        // The first ones can't be read, or the scan couldn't read their size or time.
        for z in zips.iter_mut().take(MAX_HASHED).skip(MAX_HASHED / 2) {
            z.size = 0;
            z.mtime_ms = 0;
        }
        let last = zips[MAX_HASHED].path.clone();
        let locked = |p: &Path| -> Result<String, String> {
            if p == last.as_path() {
                crate::provenance::sha256_file(p)
            } else {
                Err("in use".into())
            }
        };
        let out = run(&conn, &zips, &fake, locked, || Ok(()));
        assert!(
            out.iter().all(|(_, o)| matches!(o, Outcome::Retry(_))),
            "{out:?}"
        );
        // The next run tries the file it didn't get to first.
        let out = run(&conn, &zips, &fake, locked, || Ok(()));
        let got = out.iter().find(|(p, _)| *p == last).map(|(_, o)| o.clone());
        assert_eq!(got, Some(Outcome::Linked(format!("cat-{MAX_HASHED}"))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
