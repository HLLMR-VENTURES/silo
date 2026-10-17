//! Moving a mods folder's archive to a new location (#166). Each mod is copied, verified, its
//! links re-pointed, then its old copy deleted — so every mod boundary is a safe point (the mods
//! folder is complete and valid). Progress survives restarts through a marker in `app_setting`;
//! a move can pause (FS25 may be launched while paused), resume, or cancel (moves back).

use crate::db::{self, OrganizedRow};
use crate::organize::{self, Layout, LinkPolicy};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

/// Suffix of an in-flight copy. `scan` skips entries ending in it — keep the two in step.
const TMP_SUFFIX: &str = ".silo-move-tmp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MoveState {
    Running,
    Paused,
    Cancelling,
}

/// Persisted while a move is unfinished. `from` is where the archive lives now; `to` is where
/// it's going. Cancelling swaps the direction but keeps `from`/`to` as originally chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveMarker {
    pub from: PathBuf,
    pub to: PathBuf,
    pub state: MoveState,
    #[serde(default)]
    pub errors: Vec<String>,
    #[serde(default)]
    pub pause_reason: Option<String>,
}

/// `app_setting` key holding a mods folder's custom archive (absent ⇒ the default).
pub fn archive_setting_key(mods_key: &str) -> String {
    format!("archive_root:{mods_key}")
}

fn marker_key(mods_key: &str) -> String {
    format!("archive_move:{mods_key}")
}

/// The unfinished move for a mods folder, if any.
pub fn load_marker(conn: &Connection, mods_key: &str) -> Option<MoveMarker> {
    db::get_app_setting(conn, &marker_key(mods_key)).and_then(|j| serde_json::from_str(&j).ok())
}

/// Persist the move marker (written before the first byte moves).
pub fn save_marker(conn: &Connection, mods_key: &str, m: &MoveMarker) -> Result<(), String> {
    let j = serde_json::to_string(m).map_err(|e| e.to_string())?;
    db::set_app_setting(conn, &marker_key(mods_key), Some(&j))
}

fn clear_marker(conn: &Connection, mods_key: &str) -> Result<(), String> {
    db::set_app_setting(conn, &marker_key(mods_key), None)
}

// One move at a time. 0 = none, 1 = pause, 2 = cancel.
static REQUEST: AtomicU8 = AtomicU8::new(0);

/// Ask the running move to pause at the next mod boundary (an in-flight copy is abandoned).
pub fn request_pause() {
    REQUEST.store(1, Ordering::SeqCst);
}

/// Ask the running move to turn around and move everything back.
pub fn request_cancel() {
    REQUEST.store(2, Ordering::SeqCst);
}

fn take_request() -> u8 {
    REQUEST.swap(0, Ordering::SeqCst)
}

fn requested() -> bool {
    REQUEST.load(Ordering::SeqCst) != 0
}

/// Emitted as the move goes (`archive-move-progress`).
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    pub current: String,
    pub cancelling: bool,
}

/// Size of a mod on disk (file, or a folder recursively). Never follows links.
fn size_of(p: &Path) -> u64 {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::read_dir(p)
            .map(|rd| rd.flatten().map(|e| size_of(&e.path())).sum())
            .unwrap_or(0),
        Ok(m) => m.len(),
        Err(_) => 0,
    }
}

/// Copy a file in chunks, giving up (and deleting the partial) when `abort()` turns true.
fn copy_file_abortable(src: &Path, dst: &Path, abort: &dyn Fn() -> bool) -> std::io::Result<()> {
    let mut r = std::fs::File::open(src)?;
    let mut w = std::fs::File::create(dst)?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        if abort() {
            drop(w);
            let _ = std::fs::remove_file(dst);
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "move paused",
            ));
        }
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        w.write_all(&buf[..n])?;
    }
    w.sync_all()
}

fn copy_dir_abortable(src: &Path, dst: &Path, abort: &dyn Fn() -> bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir_abortable(&e.path(), &to, abort)?;
        } else {
            copy_file_abortable(&e.path(), &to, abort)?;
        }
    }
    Ok(())
}

/// Same content? Zips: same size, then SHA-256. Folders: same file count and total size,
/// recursively.
fn verify(src: &Path, dst: &Path, kind: &str) -> bool {
    if kind == "dir" {
        fn tally(p: &Path) -> (usize, u64) {
            std::fs::read_dir(p)
                .map(|rd| {
                    rd.flatten().fold((0, 0), |(n, s), e| {
                        if e.path().is_dir() {
                            let (a, b) = tally(&e.path());
                            (n + a, s + b)
                        } else {
                            (n + 1, s + e.metadata().map(|m| m.len()).unwrap_or(0))
                        }
                    })
                })
                .unwrap_or((0, 0))
        }
        dst.is_dir() && tally(src) == tally(dst)
    } else {
        match (std::fs::metadata(src), std::fs::metadata(dst)) {
            (Ok(a), Ok(b)) if a.is_file() && b.is_file() && a.len() == b.len() => {}
            _ => return false,
        }
        matches!(
            (crate::provenance::sha256_file(src), crate::provenance::sha256_file(dst)),
            (Ok(a), Ok(b)) if a == b
        )
    }
}

fn remove_any(p: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(_) => Ok(()),
    }
}

/// If the mod is active and its mods-folder entry is a symlink into `src`, point it at `dst`.
/// Hardlinked/copied zips and marker-copied folders are standalone and need nothing. If the new
/// link can't be made, the old one is put back (best effort) so the mod stays active.
fn repoint(
    row: &OrganizedRow,
    mods: &Path,
    src: &Path,
    dst: &Path,
    policy: &mut LinkPolicy,
) -> Result<(), String> {
    if !row.active {
        return Ok(());
    }
    let link = mods.join(&row.file_name);
    let Ok(m) = link.symlink_metadata() else {
        return Ok(());
    };
    if !m.file_type().is_symlink() {
        return Ok(());
    }
    let target = std::fs::read_link(&link).map_err(|e| e.to_string())?;
    let points_at_src =
        target == src || std::fs::canonicalize(&link).ok() == std::fs::canonicalize(src).ok();
    if !points_at_src && link.exists() {
        return Ok(()); // a link to something else — not ours to change
    }
    // A dangling dir link can't report itself as a dir on Windows, so `remove_link` may pick
    // `remove_file`; `remove_dir` then removes the link itself (never a real folder: it's a link).
    organize::remove_link(&link)
        .or_else(|_| std::fs::remove_dir(&link))
        .map_err(|e| format!("unlink: {e}"))?;
    if let Err(e) = organize::make_link(dst, &link, &row.kind, policy) {
        if src.exists() {
            let _ = organize::make_link(src, &link, &row.kind, policy);
        }
        return Err(format!("relink: {e}"));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Moved,
    AlreadyDone,
}

/// Move one archived mod from `from` to `to` (archive folders). Idempotent: safe to re-run after
/// a crash, pause, or failure. Never deletes the source until the destination is verified and
/// the mod's link (if any) points at it. `policy` is built for the destination layout
/// (`Layout { mods, archive: to }`). A different file already at the destination is never
/// deleted or overwritten — the mod fails and stays where it is.
pub(crate) fn move_one(
    row: &OrganizedRow,
    mods: &Path,
    from: &Path,
    to: &Path,
    policy: &mut LinkPolicy,
    abort: &dyn Fn() -> bool,
) -> Result<Step, String> {
    let src = organize::archive_path(from, &row.category, &row.file_name);
    let dst = organize::archive_path(to, &row.category, &row.file_name);
    let tmp = dst.with_file_name(format!("{}{TMP_SUFFIX}", row.file_name));
    let (s, d) = (
        src.symlink_metadata().is_ok(),
        dst.symlink_metadata().is_ok(),
    );
    if !s && d {
        repoint(row, mods, &src, &dst, policy)?;
        return Ok(Step::AlreadyDone);
    }
    if !s && !d {
        return Err("the archived copy is missing from both locations".into());
    }
    if d {
        // Our own copies are placed by an atomic rename after a full, synced copy, so a
        // destination that doesn't match is something else (e.g. another version the user keeps
        // there). Never delete it.
        if !verify(&src, &dst, &row.kind) {
            return Err(format!(
                "a different copy is already at {} — move or rename it, then retry",
                dst.display()
            ));
        }
    } else {
        if let Some(p) = dst.parent() {
            std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
        }
        let _ = remove_any(&tmp);
        let copied = if row.kind == "dir" {
            copy_dir_abortable(&src, &tmp, abort)
        } else {
            copy_file_abortable(&src, &tmp, abort)
        };
        if let Err(e) = copied {
            let _ = remove_any(&tmp);
            return Err(e.to_string());
        }
        std::fs::rename(&tmp, &dst).map_err(|e| {
            let _ = remove_any(&tmp);
            format!("place: {e}")
        })?;
        if !verify(&src, &dst, &row.kind) {
            let _ = remove_any(&dst);
            return Err("the copy didn't match the original".into());
        }
    }
    repoint(row, mods, &src, &dst, policy)?;
    remove_any(&src).map_err(|e| format!("remove old copy: {e}"))?;
    Ok(Step::Moved)
}

/// Delete stale temp copies left by a crash, under each category folder of `archive`.
fn clean_temps(archive: &Path) {
    if let Ok(cats) = std::fs::read_dir(archive) {
        for c in cats.flatten() {
            if let Ok(es) = std::fs::read_dir(c.path()) {
                for e in es.flatten() {
                    if e.file_name().to_string_lossy().ends_with(TMP_SUFFIX) {
                        let _ = remove_any(&e.path());
                    }
                }
            }
        }
    }
}

pub struct RunCtx<'a> {
    pub conn: &'a Connection,
    pub mods: &'a Path,
    pub mods_key: &'a str,
    pub game_running: &'a dyn Fn() -> bool,
    pub progress: &'a mut dyn FnMut(&Progress),
}

/// Pause the move: persist and hand the marker back.
fn pause(ctx: &RunCtx, mut marker: MoveMarker) -> Result<Option<MoveMarker>, String> {
    marker.state = MoveState::Paused;
    save_marker(ctx.conn, ctx.mods_key, &marker)?;
    Ok(Some(marker))
}

/// Run (or resume / cancel) the move described by `marker` until it finishes or pauses.
/// Returns the final marker (`None` when the move is finished and cleared).
pub fn run(ctx: &mut RunCtx, mut marker: MoveMarker) -> Result<Option<MoveMarker>, String> {
    if (ctx.game_running)() {
        return Err(
            "Close Farming Simulator 25 first — Silo won't move mods while the game is using them."
                .into(),
        );
    }
    let _ = take_request();
    if marker.state == MoveState::Paused {
        marker.state = MoveState::Running;
    }
    marker.pause_reason = None;
    marker.errors.clear();
    save_marker(ctx.conn, ctx.mods_key, &marker)?;

    'restart: loop {
        let (from, to) = match marker.state {
            MoveState::Cancelling => (marker.to.clone(), marker.from.clone()),
            _ => (marker.from.clone(), marker.to.clone()),
        };
        clean_temps(&to);
        // Links are made into the destination archive; the policy probes (lazily, at most once)
        // only if a hardlink actually fails.
        let layout = Layout {
            mods: ctx.mods.to_path_buf(),
            archive: to.clone(),
        };
        let mut policy = LinkPolicy::new(&layout);
        let mut rows = db::load_organized(ctx.conn, ctx.mods_key);
        rows.sort_by(|a, b| a.tech_name.cmp(&b.tech_name));
        let sizes: Vec<u64> = rows
            .iter()
            .map(|r| {
                let s = organize::archive_path(&from, &r.category, &r.file_name);
                let d = organize::archive_path(&to, &r.category, &r.file_name);
                size_of(if s.exists() { &s } else { &d })
            })
            .collect();
        let mut p = Progress {
            total: rows.len(),
            bytes_total: sizes.iter().sum(),
            cancelling: marker.state == MoveState::Cancelling,
            ..Default::default()
        };
        for (row, size) in rows.iter().zip(sizes) {
            if (ctx.game_running)() {
                marker.pause_reason = Some("Farming Simulator 25 started".into());
                return pause(ctx, marker);
            }
            match take_request() {
                1 => return pause(ctx, marker),
                2 if marker.state != MoveState::Cancelling => {
                    marker.state = MoveState::Cancelling;
                    save_marker(ctx.conn, ctx.mods_key, &marker)?;
                    continue 'restart;
                }
                _ => {}
            }
            p.current = row.tech_name.clone();
            (ctx.progress)(&p);
            match move_one(row, ctx.mods, &from, &to, &mut policy, &requested) {
                Ok(_) => {}
                Err(_) if requested() => {
                    // Interrupted mid-copy by pause/cancel: handled at the next boundary.
                    continue;
                }
                Err(e) => marker.errors.push(format!("{}: {e}", row.tech_name)),
            }
            p.done += 1;
            p.bytes_done += size;
            (ctx.progress)(&p);
        }
        // A request that arrived during the last mod.
        match take_request() {
            1 => return pause(ctx, marker),
            2 if marker.state != MoveState::Cancelling => {
                marker.state = MoveState::Cancelling;
                save_marker(ctx.conn, ctx.mods_key, &marker)?;
                continue 'restart;
            }
            _ => {}
        }
        if !marker.errors.is_empty() {
            return pause(ctx, marker);
        }
        // Finished: switch the setting (only on a completed forward move), tidy the folder we
        // left, clear the marker.
        let default = Layout::default_for(ctx.mods).archive;
        if marker.state != MoveState::Cancelling {
            let value = if crate::modsroot::same_folder(&marker.to, &default) {
                None
            } else {
                Some(marker.to.to_string_lossy().into_owned())
            };
            db::set_app_setting(
                ctx.conn,
                &archive_setting_key(ctx.mods_key),
                value.as_deref(),
            )?;
        }
        let left = Layout {
            mods: ctx.mods.to_path_buf(),
            archive: from.clone(),
        };
        if left.archive_is_default() {
            // Silo's own folder: remove every empty directory, and the folder itself if empty.
            let _ = organize::cleanup_empty_archive(&from, true);
        } else {
            // A folder the user chose: only the now-empty category folders Silo filed into.
            let categories: HashSet<String> = rows.iter().map(|r| r.category.clone()).collect();
            organize::remove_empty_category_dirs(&from, &categories);
        }
        clear_marker(ctx.conn, ctx.mods_key)?;
        return Ok(None);
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preflight {
    pub mod_count: usize,
    pub bytes: u64,
    pub free_bytes: Option<u64>,
    pub same_drive: bool,
    pub symlinks_available: bool,
    pub loose_mods: usize,
    pub problems: Vec<String>,
}

/// Path sanity for a new archive location: anything `Layout::new` rejects, plus "already
/// there", "inside the mods folder", and "another folder's archive". Pure apart from `same`
/// (inject `modsroot::same_folder`) and `Layout::new`'s canonical comparison.
pub fn path_problems(
    current: &Path,
    new: &Path,
    mods: &Path,
    other_archives: &[PathBuf],
    same: impl Fn(&Path, &Path) -> bool,
) -> Vec<String> {
    let mut v = Vec::new();
    if same(current, new) {
        v.push("That's already where your library archive is.".into());
    }
    let default = Layout::default_for(mods).archive;
    let inside = new.starts_with(mods) && !same(new, &default);
    if inside {
        v.push("Pick a folder outside your mods folder (or the default archive).".into());
    }
    if let Err(e) = Layout::new(mods.to_path_buf(), new.to_path_buf()) {
        // Don't repeat the "inside the mods folder" problem in other words.
        if !inside {
            v.push(e);
        }
    }
    if other_archives.iter().any(|a| same(a, new)) {
        v.push("Another mods folder already uses that archive.".into());
    }
    v
}

/// Space problem, if the target drive can't hold the archive plus a 1 GB margin.
pub fn space_problem(bytes: u64, free: Option<u64>) -> Option<String> {
    const MARGIN: u64 = 1 << 30;
    match free {
        Some(f) if f < bytes + MARGIN => Some(format!(
            "Not enough free space: needs {:.1} GB, {:.1} GB free.",
            (bytes + MARGIN) as f64 / 1e9,
            f as f64 / 1e9
        )),
        _ => None,
    }
}

/// Loose mods (zips / mod folders) at the top level of an archive folder — candidates to adopt.
pub fn loose_mods(archive: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(archive)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.extension()
                        .map(|x| x.eq_ignore_ascii_case("zip"))
                        .unwrap_or(false)
                        || p.join("modDesc.xml").is_file()
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fx {
        base: PathBuf,
        mods: PathBuf,
        old: PathBuf,
        new: PathBuf,
        conn: Connection,
        key: String,
    }

    fn fx(tag: &str) -> Fx {
        let base = std::env::temp_dir().join(format!("silo_reloc_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let old = mods.join("archive");
        let new = base.join("hdd");
        std::fs::create_dir_all(old.join("Tools")).unwrap();
        std::fs::create_dir_all(&new).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&mods);
        Fx {
            base,
            mods,
            old,
            new,
            conn,
            key,
        }
    }

    /// The destination layout a move into `f.new` links against.
    fn dest(f: &Fx) -> Layout {
        Layout {
            mods: f.mods.clone(),
            archive: f.new.clone(),
        }
    }

    fn add_zip(f: &Fx, tech: &str, active: bool) -> OrganizedRow {
        let file = format!("{tech}.zip");
        std::fs::write(
            f.old.join("Tools").join(&file),
            format!("PK\x03\x04 {tech}"),
        )
        .unwrap();
        let row = OrganizedRow {
            tech_name: tech.into(),
            file_name: file,
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
            active,
        };
        db::upsert_organized(&f.conn, &f.key, &row).unwrap();
        row
    }

    fn marker(f: &Fx) -> MoveMarker {
        MoveMarker {
            from: f.old.clone(),
            to: f.new.clone(),
            state: MoveState::Running,
            errors: vec![],
            pause_reason: None,
        }
    }

    fn run_with(
        f: &Fx,
        m: MoveMarker,
        game: &dyn Fn() -> bool,
    ) -> Result<Option<MoveMarker>, String> {
        let mut prog = |_: &Progress| {};
        let mut ctx = RunCtx {
            conn: &f.conn,
            mods: &f.mods,
            mods_key: &f.key,
            game_running: game,
            progress: &mut prog,
        };
        run(&mut ctx, m)
    }

    #[test]
    fn move_one_handles_every_state() {
        let f = fx("states");
        let row = add_zip(&f, "FS25_A", false);
        let layout = dest(&f);
        let mut pol = LinkPolicy::new(&layout);
        let never = || false;
        let (old_a, new_a) = (
            f.old.join("Tools/FS25_A.zip"),
            f.new.join("Tools/FS25_A.zip"),
        );
        // old only → moved
        assert_eq!(
            move_one(&row, &f.mods, &f.old, &f.new, &mut pol, &never).unwrap(),
            Step::Moved
        );
        assert!(new_a.exists() && !old_a.exists());
        // new only → already done
        assert_eq!(
            move_one(&row, &f.mods, &f.old, &f.new, &mut pol, &never).unwrap(),
            Step::AlreadyDone
        );
        // both, identical (crash after placing, before deleting) → old copy removed
        std::fs::write(&old_a, b"PK\x03\x04 FS25_A").unwrap();
        assert_eq!(
            move_one(&row, &f.mods, &f.old, &f.new, &mut pol, &never).unwrap(),
            Step::Moved
        );
        assert!(!old_a.exists());
        assert_eq!(std::fs::read(&new_a).unwrap(), b"PK\x03\x04 FS25_A");
        // both, different → refused; neither file is touched
        std::fs::write(&old_a, b"PK\x03\x04 FS25_A").unwrap();
        std::fs::write(&new_a, b"someone else's build").unwrap();
        assert!(move_one(&row, &f.mods, &f.old, &f.new, &mut pol, &never).is_err());
        assert_eq!(std::fs::read(&old_a).unwrap(), b"PK\x03\x04 FS25_A");
        assert_eq!(std::fs::read(&new_a).unwrap(), b"someone else's build");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn an_aborted_copy_leaves_no_temp_and_keeps_the_original() {
        let f = fx("abort");
        let row = add_zip(&f, "FS25_A", false);
        let layout = dest(&f);
        let mut pol = LinkPolicy::new(&layout);
        assert!(move_one(&row, &f.mods, &f.old, &f.new, &mut pol, &|| true).is_err());
        assert!(f.old.join("Tools/FS25_A.zip").exists());
        assert!(!f.new.join("Tools/FS25_A.zip").exists());
        assert!(!f.new.join("Tools/FS25_A.zip.silo-move-tmp").exists());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn a_full_move_switches_the_setting_and_clears_the_marker() {
        let f = fx("full");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        let out = run_with(&f, marker(&f), &|| false).unwrap();
        assert!(out.is_none());
        assert!(f.new.join("Tools/FS25_A.zip").exists() && f.new.join("Tools/FS25_B.zip").exists());
        assert_eq!(
            db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).as_deref(),
            Some(f.new.to_string_lossy().as_ref())
        );
        assert!(load_marker(&f.conn, &f.key).is_none());
        assert!(!f.old.exists(), "the emptied default archive is removed");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn moving_back_to_the_default_clears_the_setting_and_leaves_the_users_folder() {
        let f = fx("back");
        add_zip(&f, "FS25_A", false);
        assert!(run_with(&f, marker(&f), &|| false).unwrap().is_none());
        std::fs::write(f.new.join("notes.txt"), b"mine").unwrap();
        // Spelled differently (trailing separator) — still the default, compared canonically.
        let back = MoveMarker {
            from: f.new.clone(),
            to: PathBuf::from(format!("{}{}", f.old.display(), std::path::MAIN_SEPARATOR)),
            state: MoveState::Running,
            errors: vec![],
            pause_reason: None,
        };
        std::fs::create_dir_all(&f.old).unwrap();
        assert!(run_with(&f, back, &|| false).unwrap().is_none());
        assert!(f.old.join("Tools/FS25_A.zip").exists());
        assert!(db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).is_none());
        assert!(
            !f.new.join("Tools").exists(),
            "our emptied category folder is removed"
        );
        assert!(
            f.new.join("notes.txt").exists(),
            "the user's own file stays"
        );
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn refuses_to_start_while_the_game_runs() {
        let f = fx("game");
        add_zip(&f, "FS25_A", false);
        assert!(run_with(&f, marker(&f), &|| true).is_err());
        assert!(f.old.join("Tools/FS25_A.zip").exists(), "nothing moved");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn pauses_when_the_game_starts_and_resume_finishes() {
        let f = fx("autopause");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        // The game "starts" after the first mod.
        let calls = std::cell::Cell::new(0);
        let game = || {
            calls.set(calls.get() + 1);
            calls.get() > 2 // 1: start check, 2: before FS25_A, 3: before FS25_B
        };
        let paused = run_with(&f, marker(&f), &game).unwrap().unwrap();
        assert_eq!(paused.state, MoveState::Paused);
        assert!(paused.pause_reason.is_some());
        assert!(f.new.join("Tools/FS25_A.zip").exists());
        assert!(f.old.join("Tools/FS25_B.zip").exists(), "B not moved yet");
        assert_eq!(load_marker(&f.conn, &f.key), Some(paused.clone()));
        // Resume with the game closed.
        assert!(run_with(&f, paused, &|| false).unwrap().is_none());
        assert!(f.new.join("Tools/FS25_B.zip").exists());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn cancel_moves_everything_back() {
        let f = fx("cancel");
        add_zip(&f, "FS25_A", false);
        add_zip(&f, "FS25_B", false);
        // Move A by hand to simulate a half-done move, then cancel.
        let rows = db::load_organized(&f.conn, &f.key);
        let layout = dest(&f);
        move_one(
            &rows[0],
            &f.mods,
            &f.old,
            &f.new,
            &mut LinkPolicy::new(&layout),
            &|| false,
        )
        .unwrap();
        let mut m = marker(&f);
        m.state = MoveState::Cancelling;
        assert!(run_with(&f, m, &|| false).unwrap().is_none());
        assert!(f.old.join("Tools/FS25_A.zip").exists() && f.old.join("Tools/FS25_B.zip").exists());
        assert!(!f.new.join("Tools/FS25_A.zip").exists());
        assert!(
            db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).is_none(),
            "setting unchanged"
        );
        assert!(
            f.new.is_dir(),
            "the user's chosen folder itself is never removed"
        );
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn a_failed_mod_keeps_its_original_and_blocks_the_switch() {
        let f = fx("fail");
        add_zip(&f, "FS25_A", false);
        let ghost = OrganizedRow {
            tech_name: "FS25_Ghost".into(),
            file_name: "FS25_Ghost.zip".into(),
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
            active: false,
        };
        db::upsert_organized(&f.conn, &f.key, &ghost).unwrap(); // no file anywhere
        let out = run_with(&f, marker(&f), &|| false).unwrap().unwrap();
        assert_eq!(out.state, MoveState::Paused);
        assert_eq!(out.errors.len(), 1);
        assert!(db::get_app_setting(&f.conn, &archive_setting_key(&f.key)).is_none());
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn an_active_hardlinked_zip_stays_readable_after_its_old_copy_is_removed() {
        let f = fx("hardlink");
        let row = add_zip(&f, "FS25_A", true);
        let flat = f.mods.join("FS25_A.zip");
        std::fs::hard_link(f.old.join("Tools/FS25_A.zip"), &flat).unwrap();
        let layout = dest(&f);
        move_one(
            &row,
            &f.mods,
            &f.old,
            &f.new,
            &mut LinkPolicy::new(&layout),
            &|| false,
        )
        .unwrap();
        assert_eq!(std::fs::read(&flat).unwrap(), b"PK\x03\x04 FS25_A");
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[cfg(unix)]
    #[test]
    fn an_active_symlinked_folder_mod_is_repointed() {
        let f = fx("dirlink");
        let src = f.old.join("Tools/FS25_D");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let row = OrganizedRow {
            tech_name: "FS25_D".into(),
            file_name: "FS25_D".into(),
            kind: "dir".into(),
            category: "Tools".into(),
            subcategory: None,
            active: true,
        };
        db::upsert_organized(&f.conn, &f.key, &row).unwrap();
        std::os::unix::fs::symlink(&src, f.mods.join("FS25_D")).unwrap();
        // Folder mods are always symlinked outside a Mac App Store container; the policy's
        // zip-symlink decision doesn't apply.
        let layout = dest(&f);
        move_one(
            &row,
            &f.mods,
            &f.old,
            &f.new,
            &mut LinkPolicy::new(&layout),
            &|| false,
        )
        .unwrap();
        assert!(f
            .mods
            .join("FS25_D")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            std::fs::canonicalize(f.mods.join("FS25_D")).unwrap(),
            std::fs::canonicalize(f.new.join("Tools/FS25_D")).unwrap()
        );
        let _ = std::fs::remove_dir_all(&f.base);
    }

    #[test]
    fn path_and_space_checks() {
        let same = |a: &Path, b: &Path| a == b;
        // Absolute on every OS (Layout::new requires it); never created.
        let base = std::env::temp_dir().join(format!("silo_reloc_paths_{}", std::process::id()));
        let mods = base.join("m");
        let current = mods.join("archive");
        let hdd = base.join("hdd").join("a");
        // already there
        assert!(!path_problems(&current, &current, &mods, &[], same).is_empty());
        // inside the mods folder
        assert!(!path_problems(&current, &mods.join("sub"), &mods, &[], same).is_empty());
        // another folder's archive
        assert!(!path_problems(&current, &hdd, &mods, std::slice::from_ref(&hdd), same).is_empty());
        // Layout::new's rules: relative, the mods folder itself, a parent of the mods folder
        assert!(!path_problems(&current, Path::new("rel/a"), &mods, &[], same).is_empty());
        assert!(!path_problems(&current, &mods, &mods, &[], same).is_empty());
        assert!(!path_problems(&current, &base, &mods, &[], same).is_empty());
        // fine
        assert!(path_problems(&current, &hdd, &mods, &[], same).is_empty());
        assert!(space_problem(10 << 30, Some(5 << 30)).is_some());
        assert!(space_problem(1 << 30, Some(5 << 30)).is_none());
        assert!(space_problem(1 << 30, None).is_none());
    }

    #[test]
    fn finds_loose_mods_at_the_archive_top_level_only() {
        let f = fx("loose");
        std::fs::write(f.new.join("FS25_X.zip"), b"PK").unwrap();
        std::fs::create_dir_all(f.new.join("FS25_Y")).unwrap();
        std::fs::write(f.new.join("FS25_Y/modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::create_dir_all(f.new.join("Tools")).unwrap(); // a category folder, not a mod
        let mut found: Vec<_> = loose_mods(&f.new)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        found.sort();
        assert_eq!(found, vec!["FS25_X.zip", "FS25_Y"]);
        let _ = std::fs::remove_dir_all(&f.base);
    }
}
