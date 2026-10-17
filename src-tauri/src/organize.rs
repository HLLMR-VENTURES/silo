//! The organize / projection engine — the only code that writes to the game's mods
//! folder. Model (chosen by the user):
//!
//! * **At rest:** every mod is moved into `mods/archive/<Category>/` (subfolders the
//!   game ignores, since it only reads the flat root).
//! * **Active:** the chosen set is projected back into the flat root as **hardlinks**
//!   (zips) or dir links (unpacked mods) — same volume as the archive, so no admin /
//!   Developer Mode and no disk duplication. The game reads the flat root and loads
//!   exactly the active set.
//! * **Flatten:** everything moves back to a vanilla flat `mods/`; only *empty* archive
//!   directories are removed. Anything Silo doesn't recognize is left in place, never
//!   deleted — always one step from stock, and never a step that eats a file.
//!
//! Every move/link is recorded in the `organized` manifest so cleanup only ever
//! touches Silo-owned entries; nothing the user placed is deleted. Failures are
//! collected and reported, not panicked on.

use crate::db::{self, OrganizedRow};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

const ARCHIVE: &str = "archive";

/// Where a mods folder's pieces live. `mods` is the flat folder the game reads; `archive` holds
/// parked mods as `<archive>/<Category>/<file>`. Default: `<mods>/archive` (#166 makes it
/// configurable per mods folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout {
    pub mods: PathBuf,
    pub archive: PathBuf,
}

impl Layout {
    /// A validated layout — use this for anything built from user input or settings. Both
    /// folders must be absolute; the archive can't be the mods folder, can't contain it, and
    /// can't sit inside it unless it is the default `<mods>/archive` (the game would otherwise
    /// see parked mods, or Silo would file the mods folder into itself). Compared canonically.
    pub fn new(mods: PathBuf, archive: PathBuf) -> Result<Layout, String> {
        if !mods.is_absolute() {
            return Err(format!(
                "The mods folder must be an absolute path: {}",
                mods.display()
            ));
        }
        if !archive.is_absolute() {
            return Err(format!(
                "The library archive must be an absolute path: {}",
                archive.display()
            ));
        }
        let mods_key = crate::modsroot::canonical_key(&mods);
        let arch_key = crate::modsroot::canonical_key(&archive);
        if arch_key == mods_key {
            return Err("The library archive can't be the mods folder itself".into());
        }
        if Path::new(&mods_key).starts_with(&arch_key) {
            return Err("The library archive can't contain the mods folder".into());
        }
        let layout = Layout { mods, archive };
        if Path::new(&arch_key).starts_with(&mods_key) && !layout.archive_is_default() {
            return Err(format!(
                "The library archive can't be inside the mods folder (except the default {ARCHIVE}/ folder)"
            ));
        }
        Ok(layout)
    }

    pub fn default_for(mods: &Path) -> Layout {
        Layout {
            mods: mods.to_path_buf(),
            archive: mods.join(ARCHIVE),
        }
    }

    /// True when the archive is the default `<mods>/archive` (Silo may remove it when empty).
    /// Compared canonically, so a differently spelled path to the same folder still counts.
    pub fn archive_is_default(&self) -> bool {
        crate::modsroot::canonical_key(&self.archive)
            == crate::modsroot::canonical_key(&self.mods.join(ARCHIVE))
    }
}

/// Dropped inside a copy-fallback **directory** projection so we can later prove Silo
/// created it (a dir has no hardlink identity to compare). Hardlink/symlink projections
/// are identified structurally and need no marker.
const SILO_MARKER: &str = ".silo-projection";

/// A mod present in the flat root that could be organized.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInput {
    pub tech_name: String,
    /// Basename in the flat root: `FS25_Foo.zip` (zip) or `FS25_Foo` (dir).
    pub file_name: String,
    /// "zip" | "dir"
    pub kind: String,
    pub category: String,
    pub subcategory: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMove {
    pub tech_name: String,
    pub file_name: String,
    pub category: String,
    pub rel_from: String,
    pub rel_to: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub changed: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

/// Result of the link-repair pass — how many copy-projections were deduped into links
/// (hardlinks on the archive's volume, symlinks when the archive is on another drive).
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelinkReport {
    /// Copy-projections converted to hardlinks (same volume) or symlinks (other drive).
    pub upgraded: usize,
    /// Already a hardlink/symlink to the archive — nothing to do.
    pub already_linked: usize,
    /// Left untouched: no archived twin, cross-volume, or the user's own (different) file.
    pub skipped: usize,
    /// Bytes reclaimed by collapsing duplicate copies into links to the archived file.
    pub reclaimed_bytes: u64,
    /// The archive is on the mods folder's volume, but that filesystem rejects hardlinks (a
    /// cloud-sync provider — OneDrive / Google Drive / Proton Drive). Silo uses copies there;
    /// there's nothing to optimize.
    pub hardlinks_unsupported: bool,
    /// The archive is on another drive (no hardlinks possible) and symlinks aren't available
    /// (Windows without Developer Mode, a sandboxed Mac App Store install, or a filesystem that
    /// refuses them). Active mods stay copies; nothing was done.
    pub cross_drive_copies: bool,
    pub errors: Vec<String>,
}

/// Whether the flat root supports hardlinks. Cloud-sync providers (OneDrive, Google Drive,
/// Proton Drive) present `Documents` as on-demand placeholders and reject hardlinks with
/// `ERROR_CLOUD_FILE_INCOMPATIBLE_HARDLINKS` (os error 396); Silo falls back to copy-projections
/// there. Cheap probe: write a tiny temp file and try to hardlink it beside itself.
pub fn supports_hardlinks(root: &Path) -> bool {
    let a = root.join(".silo-hardlink-probe");
    let b = root.join(".silo-hardlink-probe.link");
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    if std::fs::write(&a, b"silo").is_err() {
        return false; // can't write a probe — don't attempt links we can't clean up
    }
    let ok = std::fs::hard_link(&a, &b).is_ok();
    let _ = std::fs::remove_file(&a);
    let _ = std::fs::remove_file(&b);
    ok
}

/// Windows-invalid path chars → underscore (spaces and `&` are fine in folder names). A
/// category that collapses to nothing once whitespace is trimmed and trailing dots/spaces are
/// dropped (as Windows does) — `""`, `"."`, `".."`, `". ."` … — would name the archive itself
/// or its parent, so it becomes `"_"`. Every other name is kept as-is, so existing archive
/// folders keep their names.
fn sanitize(category: &str) -> String {
    let s: String = category
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect();
    if s.trim().trim_end_matches(['.', ' ']).is_empty() {
        "_".into()
    } else {
        s
    }
}

pub(crate) fn archive_dir(archive: &Path, category: &str) -> PathBuf {
    archive.join(sanitize(category))
}

pub(crate) fn archive_path(archive: &Path, category: &str, file_name: &str) -> PathBuf {
    archive_dir(archive, category).join(file_name)
}

/// Dry run: which mods in `source` (the flat mods folder, or the archive's top level when
/// adopting) would move into `<archive>/<Category>/`.
pub fn plan_organize(source: &Path, mods: &[ModInput]) -> Vec<PlannedMove> {
    let mut plan = Vec::new();
    for m in mods {
        let from = source.join(&m.file_name);
        // Only plan mods that are actually sitting in the source folder right now.
        if !from.exists() {
            continue;
        }
        plan.push(PlannedMove {
            tech_name: m.tech_name.clone(),
            file_name: m.file_name.clone(),
            category: m.category.clone(),
            rel_from: m.file_name.clone(),
            rel_to: format!("{ARCHIVE}/{}/{}", sanitize(&m.category), m.file_name),
        });
    }
    plan
}

/// Move mods from `source` (the flat mods folder, or the archive's top level when adopting)
/// into `<archive>/<Category>/` and record the manifest. Mods already recorded (organized)
/// are skipped.
pub fn apply_organize(
    conn: &Connection,
    layout: &Layout,
    source: &Path,
    mods: &[ModInput],
) -> Report {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let mut rep = Report::default();
    let already: HashSet<String> = db::load_organized(conn, &key)
        .into_iter()
        .map(|r| r.tech_name)
        .collect();

    for m in mods {
        if already.contains(&m.tech_name) {
            rep.skipped += 1;
            continue;
        }
        // file_name drives every join below — it must be a plain basename, never a path.
        if let Err(e) = crate::paths::safe_file_name(&m.file_name) {
            rep.errors.push(format!("{}: {e}", m.tech_name));
            continue;
        }
        let from = source.join(&m.file_name);
        if !from.exists() {
            rep.skipped += 1;
            continue;
        }
        let dir = archive_dir(&layout.archive, &m.category);
        let to = dir.join(&m.file_name);
        // Adopting a folder mod named like its category would move it into itself.
        if to.starts_with(&from) {
            rep.errors.push(format!(
                "{}: {} would be filed inside itself — left it untouched",
                m.tech_name, m.file_name
            ));
            rep.skipped += 1;
            continue;
        }
        // Never overwrite: a rename replaces an existing destination, and whatever sits there
        // (another build, the user's own file) may be the only copy.
        if to.symlink_metadata().is_ok() {
            rep.errors.push(format!(
                "{}: {} already exists in the archive — left both untouched",
                m.tech_name, m.file_name
            ));
            rep.skipped += 1;
            continue;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            rep.errors
                .push(format!("{}: mkdir {}: {e}", m.tech_name, dir.display()));
            continue;
        }
        // Manifest-first: record the row BEFORE moving the file. If the DB write fails we
        // never move, so we can't leave a file orphaned in the archive (which flatten
        // would later be unable to tell from junk). If the move then fails, roll the row
        // back. This holds under normal errors; a HARD crash between the row write and the
        // move can still leave a row with no archived file and the original still in the
        // root. That window is made non-destructive on the read side: flatten/set_active
        // never remove a root entry unless the archived copy exists behind it, so the only
        // copy is never deleted. (A journalled, startup-reconciled operation log would close
        // the window entirely — tracked as a follow-up.)
        let row = OrganizedRow {
            tech_name: m.tech_name.clone(),
            file_name: m.file_name.clone(),
            kind: m.kind.clone(),
            category: m.category.clone(),
            subcategory: m.subcategory.clone(),
            active: false,
        };
        if let Err(e) = db::upsert_organized(conn, &key, &row) {
            rep.errors.push(format!("{}: manifest: {e}", m.tech_name));
            continue;
        }
        match move_path(&from, &to) {
            Ok(()) => rep.changed += 1,
            Err(e) => {
                // Roll back the row we just wrote — nothing was moved.
                let _ = db::delete_organized(conn, &key, &m.tech_name);
                rep.errors.push(format!("{}: move: {e}", m.tech_name));
            }
        }
    }
    rep
}

/// Reconcile the flat-root projection to exactly `active`: link the ones that
/// should be active, remove links for the ones that shouldn't.
pub fn set_active(conn: &Connection, layout: &Layout, active: &HashSet<String>) -> Report {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let mut rep = Report::default();
    let mut policy = LinkPolicy::new(layout);
    for row in db::load_organized(conn, &key) {
        let link = root.join(&row.file_name);
        let src = archive_path(&layout.archive, &row.category, &row.file_name);
        let want = active.contains(&row.tech_name);
        let linked = link.symlink_metadata().is_ok();

        if want && !linked {
            match make_link(&src, &link, &row.kind, &mut policy) {
                Ok(()) => {
                    // Record the projection's identity so a later deactivate can confirm it's
                    // ours (and skip the content hash). Zips only — dir projections are
                    // identified structurally (junction / marker), cheaply.
                    if row.kind == "zip" {
                        if let Some((size, mtime)) = file_stats(&link) {
                            db::record_projection(conn, &key, &row.file_name, size, mtime);
                        }
                    }
                    if let Err(e) = db::set_organized_active(conn, &key, &row.tech_name, true) {
                        rep.errors.push(format!("{}: manifest: {e}", row.tech_name));
                    }
                    rep.changed += 1;
                }
                Err(e) => rep.errors.push(format!("{}: link: {e}", row.tech_name)),
            }
        } else if !want && linked {
            // Two guards before removing anything from the root:
            //  1. The archived copy must exist behind it — a root entry with no archive is the
            //     only copy (e.g. a crash between manifest-write and move), never delete it.
            //  2. The entry must PROVABLY be Silo's projection. FAST path: its (size, mtime)
            //     still match what we recorded when we wrote it → our own untouched copy, no
            //     hash needed. Otherwise fall back to the careful structural/content check.
            //     If the user swapped in their own file at that name, neither matches — leave it.
            if !src.exists() {
                rep.errors.push(format!(
                    "{}: no archived copy behind the root file — kept it (won't delete the only copy)",
                    row.tech_name
                ));
                rep.skipped += 1;
            } else if !(matches_provenance(conn, &key, &link, &row.file_name)
                || is_silo_projection(&link, &src, &row.kind))
            {
                rep.errors.push(format!(
                    "{}: the file in the mods folder isn't the one Silo projected (did you replace it?) — left it untouched",
                    row.tech_name
                ));
                rep.skipped += 1;
            } else {
                match remove_link(&link) {
                    Ok(()) => {
                        db::forget_projection(conn, &key, &row.file_name);
                        if let Err(e) = db::set_organized_active(conn, &key, &row.tech_name, false)
                        {
                            rep.errors.push(format!("{}: manifest: {e}", row.tech_name));
                        }
                        rep.changed += 1;
                    }
                    Err(e) => rep.errors.push(format!("{}: unlink: {e}", row.tech_name)),
                }
            }
        } else {
            rep.skipped += 1;
        }
    }
    rep
}

/// A plain file's `(size, mtime_ms)`, or `None` for a symlink/junction (identified structurally,
/// not by provenance) or an unreadable path.
fn file_stats(path: &Path) -> Option<(u64, i64)> {
    let m = std::fs::symlink_metadata(path).ok()?;
    if m.file_type().is_symlink() {
        return None;
    }
    let mtime = m
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as i64;
    Some((m.len(), mtime))
}

/// Fast, hash-free ownership check: the flat file is provably Silo's own untouched projection if
/// its current `(size, mtime)` still match what we recorded when we wrote it. A file the user
/// swapped in has a different mtime and won't match — so it's never wrongly treated as ours.
fn matches_provenance(conn: &Connection, key: &str, link: &Path, file_name: &str) -> bool {
    let Some((size, mtime)) = db::projection_stats(conn, key, file_name) else {
        return false;
    };
    matches!(file_stats(link), Some((s, m)) if s == size && m == mtime)
}

/// Repair copy-projections into hardlinks. A projection that was written as a byte-for-byte
/// **copy** (rather than a hardlink — e.g. the flat file pre-existed so `set_active` never
/// linked it) wastes the mod's bytes twice and, worse, forces every later activate/deactivate
/// to `sha256` the whole file to prove it's Silo's. Collapsing each such copy into a hardlink
/// to its archived twin reclaims the space and makes identity checks O(1) forever after.
///
/// When the archive is on another drive (no hardlinks possible), copies are upgraded to
/// symlinks instead, where the OS allows them.
///
/// Only ever touches a flat file that is **byte-identical** to the archive copy — never the
/// user's own replacement build. Crash-safe: hardlink (or symlink) to a temp sibling,
/// then atomically rename over the flat file (both the old copy and the new hardlink hold the
/// same bytes, so an interruption at any point leaves a valid file). `progress(done, total)`
/// is called once per active zip examined so a long pass can show a bar instead of a freeze.
pub fn relink_projections(
    conn: &Connection,
    layout: &Layout,
    progress: impl FnMut(usize, usize),
) -> RelinkReport {
    // Same volume → hardlinks, unless the folder is cloud-synced (every attempt would error
    // 396 — copies stay). Archive on another drive → symlinks, when the OS allows them.
    let mode = if same_volume(&layout.mods, &layout.archive) {
        if supports_hardlinks(&layout.mods) {
            RelinkMode::Hardlink
        } else {
            RelinkMode::SameVolumeNoHardlinks
        }
    } else if symlinks_usable(layout) {
        RelinkMode::Symlink
    } else {
        RelinkMode::CrossDriveNoSymlinks
    };
    relink_with(conn, layout, mode, progress)
}

/// How [`relink_projections`] may link — decided once up front, injectable for tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelinkMode {
    Hardlink,
    Symlink,
    /// Same volume, but hardlinks are rejected (cloud-synced folder).
    SameVolumeNoHardlinks,
    /// Archive on another drive and symlinks unavailable.
    CrossDriveNoSymlinks,
}

fn relink_with(
    conn: &Connection,
    layout: &Layout,
    mode: RelinkMode,
    mut progress: impl FnMut(usize, usize),
) -> RelinkReport {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let mut rep = RelinkReport::default();
    // Nothing can be linked: report the situation cleanly instead of one failure per mod.
    let hardlinks = match mode {
        RelinkMode::Hardlink => true,
        RelinkMode::Symlink => false,
        RelinkMode::SameVolumeNoHardlinks => {
            rep.hardlinks_unsupported = true;
            return rep;
        }
        RelinkMode::CrossDriveNoSymlinks => {
            rep.cross_drive_copies = true;
            return rep;
        }
    };
    // Only active zips can be projected as a plain file; dir projections are marker-identified
    // and cheap already, so they never need repair.
    let rows: Vec<OrganizedRow> = db::load_organized(conn, &key)
        .into_iter()
        .filter(|r| r.active && r.kind == "zip")
        .collect();
    let total = rows.len();
    for (i, row) in rows.into_iter().enumerate() {
        progress(i, total);
        let flat = root.join(&row.file_name);
        let src = archive_path(&layout.archive, &row.category, &row.file_name);
        let Ok(fmeta) = flat.symlink_metadata() else {
            rep.skipped += 1;
            continue;
        };
        if !src.exists() {
            rep.skipped += 1;
            continue;
        }
        // A symlink resolving to the archive is already a link (a symlink pass across drives).
        if fmeta.file_type().is_symlink() {
            if is_silo_projection(&flat, &src, "zip") {
                rep.already_linked += 1;
            } else {
                rep.skipped += 1;
            }
            continue;
        }
        // Already sharing the archive's file id → a hardlink, nothing to reclaim.
        if let (Ok(a), Ok(b)) = (
            same_file::Handle::from_path(&flat),
            same_file::Handle::from_path(&src),
        ) {
            if a == b {
                rep.already_linked += 1;
                continue;
            }
        }
        match same_bytes(&flat, &src) {
            Ok(true) => match relink_over(&src, &flat, hardlinks) {
                Ok(()) => {
                    rep.upgraded += 1;
                    rep.reclaimed_bytes += fmeta.len();
                    // The copy's recorded (size, mtime) no longer describes the flat entry. A
                    // hardlink gets its own fresh record (fast deactivate); a symlink is
                    // identified structurally and keeps none.
                    db::forget_projection(conn, &key, &row.file_name);
                    if hardlinks {
                        if let Some((size, mtime)) = file_stats(&flat) {
                            db::record_projection(conn, &key, &row.file_name, size, mtime);
                        }
                    }
                }
                Err(e) => rep.errors.push(format!("{}: relink: {e}", row.tech_name)),
            },
            // Distinct bytes → the user swapped in their own build; leave it untouched.
            Ok(false) => rep.skipped += 1,
            Err(e) => rep.errors.push(format!("{}: compare: {e}", row.tech_name)),
        }
    }
    progress(total, total);
    rep
}

/// Byte-equal? Cheap size gate first, then a content hash of each.
fn same_bytes(a: &Path, b: &Path) -> std::io::Result<bool> {
    let (ma, mb) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
    if ma.len() != mb.len() {
        return Ok(false);
    }
    Ok(
        match (
            crate::provenance::sha256_file(a),
            crate::provenance::sha256_file(b),
        ) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        },
    )
}

/// Atomically replace `flat` with a hardlink (`hard`) or symlink to `src`. Link a temp sibling
/// in the same directory (a failure leaves `flat` untouched), then rename over `flat`.
fn relink_over(src: &Path, flat: &Path, hard: bool) -> std::io::Result<()> {
    let tmp = flat.with_file_name(format!(
        "{}.silo-relink-tmp",
        flat.file_name().and_then(|n| n.to_str()).unwrap_or("mod")
    ));
    let _ = std::fs::remove_file(&tmp);
    if hard {
        std::fs::hard_link(src, &tmp)?;
    } else {
        symlink_file(src, &tmp)?;
    }
    match std::fs::rename(&tmp, flat) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Adopt the file the user swapped into the mods folder as the new canonical version: back
/// up the old archived copy, promote the flat file into the archive, and (if the mod is
/// active) re-project it so it's Silo-managed again. Reversible — the old copy is kept under
/// `backups/`.
pub fn adopt_foreign(conn: &Connection, layout: &Layout, file_name: &str) -> Result<(), String> {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    crate::paths::safe_file_name(file_name).map_err(|e| e.to_string())?;
    let row = db::load_organized(conn, &key)
        .into_iter()
        .find(|r| r.file_name == file_name)
        .ok_or_else(|| "Not a Silo-managed mod".to_string())?;
    let flat = root.join(file_name);
    let Ok(fmeta) = flat.symlink_metadata() else {
        return Err("No file in the mods folder to adopt".into());
    };
    // Promoting a link would file the link itself (not a mod) into the archive and back up the
    // real archived copy out of the way. Refuse before touching anything.
    if fmeta.file_type().is_symlink() {
        return Err("The mods-folder entry is a link, not a file — use Restore instead".into());
    }
    let archive = archive_path(&layout.archive, &row.category, file_name);
    // Back up the old archived copy before replacing it.
    if archive.exists() {
        let bak = backup_path(root, file_name)?;
        move_path(&archive, &bak).map_err(|e| format!("backup: {e}"))?;
    } else if let Some(parent) = archive.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // Promote the swapped-in file into the archive, then re-project it back into the root.
    move_path(&flat, &archive).map_err(|e| format!("promote: {e}"))?;
    if row.active {
        make_link(&archive, &flat, &row.kind, &mut LinkPolicy::new(layout))
            .map_err(|e| format!("re-project: {e}"))?;
    }
    Ok(())
}

/// Restore Silo's managed copy over a file the user swapped in: back up their file, then
/// re-project the archived original (if the mod is active). Reversible — their file is kept
/// under `backups/`.
pub fn restore_projection(
    conn: &Connection,
    layout: &Layout,
    file_name: &str,
) -> Result<(), String> {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    crate::paths::safe_file_name(file_name).map_err(|e| e.to_string())?;
    let row = db::load_organized(conn, &key)
        .into_iter()
        .find(|r| r.file_name == file_name)
        .ok_or_else(|| "Not a Silo-managed mod".to_string())?;
    let archive = archive_path(&layout.archive, &row.category, file_name);
    if !archive.exists() {
        return Err("No archived copy to restore from".into());
    }
    let flat = root.join(file_name);
    // Back up the user's file, then free the slot.
    if flat.symlink_metadata().is_ok() {
        let bak = backup_path(root, file_name)?;
        move_path(&flat, &bak).map_err(|e| format!("backup: {e}"))?;
    }
    if row.active {
        make_link(&archive, &flat, &row.kind, &mut LinkPolicy::new(layout))
            .map_err(|e| format!("re-project: {e}"))?;
    }
    Ok(())
}

/// A timestamped, non-clobbering location under `backups/` for a file we're about to replace.
fn backup_path(root: &Path, file_name: &str) -> Result<PathBuf, String> {
    let dir = root.join("backups");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Ok(dir.join(format!("{file_name}.{secs}.bak")))
}

/// Restore a vanilla flat `mods/`: remove every Silo link, move archived files back
/// to the root, remove only empty archive directories (never an unrecognized file), and
/// clear the manifest.
pub fn flatten(conn: &Connection, layout: &Layout) -> Report {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let mut rep = Report::default();
    let mut categories: HashSet<String> = HashSet::new();
    for row in db::load_organized(conn, &key) {
        categories.insert(row.category.clone());
        let link = root.join(&row.file_name);
        let src = archive_path(&layout.archive, &row.category, &row.file_name);

        if src.exists() {
            let occupied = link.symlink_metadata().is_ok();
            // If a file occupies the root slot but it isn't Silo's projection (the user swapped
            // their own build in), don't remove it and don't overwrite it with the archived
            // copy — leave both in place and report the anomaly for the user to resolve.
            if occupied && !is_silo_projection(&link, &src, &row.kind) {
                rep.errors.push(format!(
                    "{}: the mods-folder file isn't Silo's projection — left it and the archived copy untouched",
                    row.tech_name
                ));
                rep.skipped += 1;
                continue;
            }
            // Normal restore: drop our projection to free the slot, then move the archived
            // original back into the root.
            if occupied {
                if let Err(e) = remove_link(&link) {
                    rep.errors.push(format!("{}: unlink: {e}", row.tech_name));
                    continue;
                }
            }
            if let Err(e) = move_path(&src, &link) {
                rep.errors.push(format!("{}: restore: {e}", row.tech_name));
                continue;
            }
        } else if link.exists() {
            // A file sits in the root slot but there is NO archived copy behind it. That is
            // the original, not a projection (a projection always has an archive) — this is
            // exactly the state a crash between the manifest write and the move leaves. Never
            // delete it: keep the file, drop the stale manifest row.
            rep.errors.push(format!(
                "{}: no archived copy — kept the file in the mods root (not deleted)",
                row.tech_name
            ));
        }
        // else: neither archive nor root file — nothing to restore; just clear the row.
        if let Err(e) = db::delete_organized(conn, &key, &row.tech_name) {
            rep.errors.push(format!("{}: manifest: {e}", row.tech_name));
            continue;
        }
        rep.changed += 1;
    }
    // A user-chosen archive is the user's folder: remove only the category folders Silo filed
    // into (and only when empty), never recurse, and don't report their other content.
    if !layout.archive_is_default() {
        remove_empty_category_dirs(&layout.archive, &categories);
        return rep;
    }
    // The default archive is Silo's own. NEVER `remove_dir_all` it — that would destroy any
    // file we don't have a manifest row for (e.g. a mod whose manifest write failed on a
    // crash). Remove only provably-empty directories, and surface anything left untouched.
    match cleanup_empty_archive(&layout.archive, true) {
        Ok(leftover) => {
            for p in leftover {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned());
                rep.errors.push(format!(
                    "kept an unrecognized file in your library archive (NOT deleted): {}",
                    name.unwrap_or_else(|| p.display().to_string())
                ));
            }
        }
        Err(e) => rep.errors.push(format!("archive cleanup: {e}")),
    }
    rep
}

/// Remove each category folder Silo filed into under `archive`, but only if it is empty.
/// Never recurses, never touches anything else, and reports nothing — for an archive folder
/// the user chose, whose other content is theirs.
pub(crate) fn remove_empty_category_dirs(archive: &Path, categories: &HashSet<String>) {
    for cat in categories {
        // `remove_dir` refuses a non-empty folder, which is exactly the guard we want.
        let _ = std::fs::remove_dir(archive_dir(archive, cat));
    }
}

/// Remove empty directories under `archive` bottom-up. NEVER deletes a file or a
/// non-empty directory — any file the manifest didn't restore is left in place and
/// returned so the caller can tell the user. This is the guard against the flatten
/// step eating an untracked mod. `archive` itself is removed (when empty) only if
/// `remove_root` — a user-chosen archive folder is never removed.
pub(crate) fn cleanup_empty_archive(
    archive: &Path,
    remove_root: bool,
) -> std::io::Result<Vec<PathBuf>> {
    let mut leftover = Vec::new();
    if !archive.exists() {
        return Ok(leftover);
    }
    // Returns true if `dir` is empty once processing finishes.
    fn walk(dir: &Path, leftover: &mut Vec<PathBuf>) -> std::io::Result<bool> {
        let mut empty = true;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            // Use symlink semantics: never follow/recurse a link.
            if entry.file_type()?.is_dir() {
                if walk(&path, leftover)? {
                    let _ = std::fs::remove_dir(&path);
                } else {
                    empty = false;
                }
            } else {
                leftover.push(path);
                empty = false;
            }
        }
        Ok(empty)
    }
    if walk(archive, &mut leftover)? && remove_root {
        let _ = std::fs::remove_dir(archive);
    }
    Ok(leftover)
}

// ── filesystem primitives ──

/// A hidden temp sibling of `to`, used for the copy-and-swap fallback.
fn tmp_sibling(to: &Path) -> PathBuf {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "x".into());
    to.with_file_name(format!(".{name}.silo-tmp"))
}

/// Remove a path whatever it is (file/dir/link), treating "not found" as success.
fn remove_path(p: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(p) {
        Ok(m) if m.is_dir() => std::fs::remove_dir_all(p),
        Ok(_) => std::fs::remove_file(p),
        Err(_) => Ok(()),
    }
}

/// Move a file/dir, preferring an instant same-volume rename. On any rename failure
/// (typically a cross-volume move) fall back SAFELY: copy to a temporary sibling of the
/// destination, rename that into place, and only then remove the source — so a failed or
/// partial copy never leaves a half-written file at `to`, and `from` is never lost until
/// the destination is complete.
fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }

    let tmp = tmp_sibling(to);
    let _ = remove_path(&tmp); // clear any stale temp from a prior aborted run
    let copied = if from.is_dir() {
        copy_dir_all(from, &tmp)
    } else {
        std::fs::copy(from, &tmp).map(|_| ())
    };
    if let Err(e) = copied {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&tmp, to) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    if from.is_dir() {
        std::fs::remove_dir_all(from)
    } else {
        std::fs::remove_file(from)
    }
}

/// Project `src` into the flat root at `link`. Files → hardlink (no privilege, same volume, no
/// disk duplication); if that fails, a symlink only when the archive is on **another drive**
/// and symlinks work there (`policy`, decided lazily); otherwise a copy. A same-volume folder
/// that rejects hardlinks (cloud-synced) keeps copies. Dirs → OS symlink, falling back to copy;
/// inside a Mac App Store container always a marked copy (the sandboxed game can't follow a
/// link out). Never overwrites an existing entry at `link`, and never links to a missing `src`
/// (a dangling link would look active but load nothing, and cleanup couldn't prove it ours).
pub(crate) fn make_link(
    src: &Path,
    link: &Path,
    kind: &str,
    policy: &mut LinkPolicy,
) -> std::io::Result<()> {
    std::fs::metadata(src)?;
    if kind == "dir" {
        if policy.mods_in_container() {
            if link.symlink_metadata().is_ok() {
                // Copying into a folder that's already there would merge into it.
                return Err(std::io::ErrorKind::AlreadyExists.into());
            }
            return copy_dir_projection(src, link);
        }
        #[cfg(unix)]
        let symlink = |s: &Path, l: &Path| std::os::unix::fs::symlink(s, l);
        #[cfg(windows)]
        let symlink = |s: &Path, l: &Path| std::os::windows::fs::symlink_dir(s, l);
        link_dir_or_copy(src, link, symlink)
    } else {
        link_zip(
            src,
            link,
            || policy.allow_symlink(),
            |s, l| std::fs::hard_link(s, l),
            symlink_file,
        )
    }
}

/// Whether a zip projection may fall back to a symlink — decided lazily, at most once per
/// engine call, and only after a hardlink attempt actually failed, so the common same-volume
/// case never probes (or writes) anything.
pub(crate) struct LinkPolicy<'a> {
    layout: &'a Layout,
    cached: Option<bool>,
}

impl<'a> LinkPolicy<'a> {
    pub(crate) fn new(layout: &'a Layout) -> Self {
        LinkPolicy {
            layout,
            cached: None,
        }
    }

    /// Symlinks only for an archive on a different volume, outside a Mac App Store sandbox,
    /// where a probe link actually works.
    pub(crate) fn allow_symlink(&mut self) -> bool {
        let layout = self.layout;
        *self.cached.get_or_insert_with(|| {
            !same_volume(&layout.mods, &layout.archive) && symlinks_usable(layout)
        })
    }

    /// The mods folder is inside a Mac App Store sandbox, so the game can't follow any link
    /// out of it — folder mods must be projected as marked copies.
    pub(crate) fn mods_in_container(&self) -> bool {
        in_mac_app_container(&self.layout.mods)
    }
}

/// Symlinks from the mods folder into the archive can be created and the game will follow them.
fn symlinks_usable(layout: &Layout) -> bool {
    !in_mac_app_container(&layout.mods) && supports_symlinks(layout)
}

/// The Mac App Store FS25 is sandboxed: it can't follow a link out of its container. Compared
/// case-insensitively (APFS is case-insensitive by default).
pub(crate) fn in_mac_app_container(mods: &Path) -> bool {
    mods.to_string_lossy()
        .to_lowercase()
        .contains("/library/containers/")
}

/// Create a file symlink at `link` pointing to `src` (Windows needs Developer Mode or admin).
pub(crate) fn symlink_file(src: &Path, link: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(src, link)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(src, link)
    }
}

/// Zip projection: hardlink (same drive, no duplication) → symlink (when `allow_symlink()`
/// says so; asked only after the hardlink failed) → byte copy. Never overwrites: an
/// `AlreadyExists` from either link strategy is returned as-is, and the copy only ever creates
/// a new file. Never dangles: a missing `src` (the hardlink's `NotFound`, or no file there
/// before a symlink) is an error, since a symlink to nothing would still be created. The
/// strategies are injected so the order is unit-testable.
fn link_zip(
    src: &Path,
    link: &Path,
    allow_symlink: impl FnOnce() -> bool,
    hard: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    soft: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    use std::io::ErrorKind::{AlreadyExists, NotFound};
    match hard(src, link) {
        Ok(()) => return Ok(()),
        Err(e) if matches!(e.kind(), AlreadyExists | NotFound) => return Err(e),
        Err(_) => {}
    }
    if !std::fs::metadata(src)?.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "the archived mod is not a file",
        ));
    }
    if allow_symlink() {
        match soft(src, link) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == AlreadyExists => return Err(e),
            Err(_) => {}
        }
    }
    copy_new(src, link)
}

/// Copy `src` to a **new** file `dst`: refuses (AlreadyExists) rather than truncating an
/// existing file, and removes the partial file if the copy fails — safe, since `create_new`
/// proves we created it.
fn copy_new(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut from = std::fs::File::open(src)?;
    let mut to = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    if let Err(e) = std::io::copy(&mut from, &mut to) {
        drop(to);
        let _ = std::fs::remove_file(dst);
        return Err(e);
    }
    Ok(())
}

/// Distinguishes concurrent symlink probes (within this process; the pid covers others).
static SYMLINK_PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Whether a symlink from the mods folder into the archive works here (always on Linux/macOS
/// for normal drives; on Windows only with Developer Mode). Cheap one-shot probe with unique
/// names, cleaned up afterwards. Never creates a folder: a missing archive or mods folder → false.
pub fn supports_symlinks(layout: &Layout) -> bool {
    if !layout.archive.is_dir() || !layout.mods.is_dir() {
        return false;
    }
    let tag = format!(
        "{}-{}",
        std::process::id(),
        SYMLINK_PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let target = layout.archive.join(format!(".silo-symlink-probe-{tag}"));
    let link = layout.mods.join(format!(".silo-symlink-probe-{tag}.link"));
    if std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
        .is_err()
    {
        return false;
    }
    let ok = symlink_file(&target, &link).is_ok();
    let _ = std::fs::remove_file(&link);
    let _ = std::fs::remove_file(&target);
    ok
}

/// Mount point holding `p` (longest matching prefix) among `disks`, with its free space.
/// Compared as normalized keys: on Windows `canonicalize` yields `\\?\C:\…` while mount points
/// are `C:\`, and `Path::starts_with` treats those prefixes as different.
fn mount_in(disks: &sysinfo::Disks, p: &Path) -> Option<(PathBuf, u64)> {
    let c = std::fs::canonicalize(p).ok()?;
    let windows = cfg!(windows);
    let key = crate::modsroot::normalize_key(&c.to_string_lossy(), windows);
    disks
        .list()
        .iter()
        .filter(|d| {
            let mount = crate::modsroot::normalize_key(&d.mount_point().to_string_lossy(), windows);
            Path::new(&key).starts_with(Path::new(&mount))
        })
        .max_by_key(|d| d.mount_point().components().count())
        .map(|d| (d.mount_point().to_path_buf(), d.available_space()))
}

/// Mount point holding `p`, for same-drive checks and free space (one disk enumeration).
pub(crate) fn mount_of(p: &Path) -> Option<(PathBuf, u64)> {
    mount_in(&sysinfo::Disks::new_with_refreshed_list(), p)
}

/// Whether `a` and `b` live on the same volume (one disk enumeration for both). Unknown →
/// assume same (a hardlink attempt then falls back safely to a copy).
pub(crate) fn same_volume(a: &Path, b: &Path) -> bool {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    match (mount_in(&disks, a), mount_in(&disks, b)) {
        (Some((ma, _)), Some((mb, _))) => ma == mb,
        _ => true,
    }
}

/// Dir projection: `symlink`, or a marked copy when the filesystem refuses links (exFAT/FAT
/// drives, no symlink privilege). Never falls back when `link` already exists — copying into
/// a folder that's already there would merge into it and stamp it as ours.
fn link_dir_or_copy(
    src: &Path,
    link: &Path,
    symlink: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match symlink(src, link) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => copy_dir_projection(src, link),
        other => other,
    }
}

/// Copy a directory as a projection and drop the ownership marker, so a later flatten /
/// deactivate can prove Silo created it (a copied dir has no link identity to compare).
fn copy_dir_projection(src: &Path, dst: &Path) -> std::io::Result<()> {
    copy_dir_all(src, dst)?;
    std::fs::write(dst.join(SILO_MARKER), b"silo\n")
}

/// True only if `link` in the flat root is provably the projection Silo created for archived
/// `src` — so removing it can't destroy a file the user put there themselves. Symlink /
/// junction resolves to `src`; a zip hardlink shares `src`'s file id; a cross-volume copy is
/// byte-identical to `src`; a copied dir carries our marker. Anything else is NOT ours.
fn is_silo_projection(link: &Path, src: &Path, kind: &str) -> bool {
    let Ok(meta) = link.symlink_metadata() else {
        return false;
    };
    let ft = meta.file_type();

    if ft.is_symlink() {
        // Our dir symlink / Windows junction resolves to the archived source.
        return matches!(
            (std::fs::canonicalize(link), std::fs::canonicalize(src)),
            (Ok(a), Ok(b)) if a == b
        );
    }

    if kind == "dir" || ft.is_dir() {
        // Copy-fallback directory projection carries our marker; a user's own folder won't.
        return link.join(SILO_MARKER).is_file();
    }

    // Regular file (zip). Same underlying file id → our hardlink (cheap, the common case).
    if let (Ok(a), Ok(b)) = (
        same_file::Handle::from_path(link),
        same_file::Handle::from_path(src),
    ) {
        if a == b {
            return true;
        }
    }
    // Distinct files: a cross-volume copy projection is byte-identical to the archive; a
    // user's replacement is not. Cheap size gate first, then a content hash.
    let (Ok(lm), Ok(sm)) = (std::fs::metadata(link), std::fs::metadata(src)) else {
        return false;
    };
    if lm.len() != sm.len() {
        return false;
    }
    matches!(
        (
            crate::provenance::sha256_file(link),
            crate::provenance::sha256_file(src),
        ),
        (Ok(a), Ok(b)) if a == b
    )
}

/// A file in the flat root occupying an organized mod's managed name that ISN'T Silo's
/// projection — something the user (or another tool) placed there. Silo never deletes it, but
/// the user should know their intended mod isn't what the game will load from that name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignFile {
    pub tech_name: String,
    pub file_name: String,
    pub kind: String, // "zip" | "dir"
    /// Version of the file currently in the mods folder (what "adopt" would keep).
    pub flat_version: Option<String>,
    /// Version of Silo's managed/archived copy (what "restore" would put back).
    pub managed_version: Option<String>,
}

/// Best-effort mod version from a mod's `modDesc.xml`, for showing which build is which.
fn mod_version(path: &Path, kind: &str) -> Option<String> {
    crate::scan::read_moddesc_xml(path, kind)
        .ok()
        .and_then(|xml| crate::moddesc::parse(&xml).version)
}

/// Scan the flat root for foreign/mismatched files sitting at organized mods' names. Cheap:
/// parked mods leave the flat root empty, so it only identity-checks the handful of entries
/// actually present against their archived source. Reuses [`is_silo_projection`], so a real
/// hardlink/symlink/copy projection is never flagged — only a file Silo can't prove it created.
pub fn detect_foreign_projections(conn: &Connection, layout: &Layout) -> Vec<ForeignFile> {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let archive = layout.archive.clone();
    if !archive.is_dir() {
        return Vec::new();
    }
    // Map each organized mod's file name → its archived path (archive/<Category>/<name>).
    let mut archived: std::collections::HashMap<std::ffi::OsString, PathBuf> =
        std::collections::HashMap::new();
    if let Ok(cats) = std::fs::read_dir(&archive) {
        for cat in cats.flatten() {
            if cat.path().is_dir() {
                if let Ok(entries) = std::fs::read_dir(cat.path()) {
                    for m in entries.flatten() {
                        archived.insert(m.file_name(), m.path());
                    }
                }
            }
        }
    }

    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for e in entries.flatten() {
        let name = e.file_name();
        if name == ARCHIVE || name == "backups" {
            continue;
        }
        // Only a name that maps to an organized mod counts — a plain loose zip with no archived
        // counterpart is an ordinary "unorganized" mod (Organize handles it), not a mismatch.
        let Some(src) = archived.get(&name) else {
            continue;
        };
        let link = e.path();
        let kind = if src.is_dir() { "dir" } else { "zip" };
        let file_name = name.to_string_lossy().into_owned();
        // Fast path: provenance match → provably ours, no hash. Else the careful check (which
        // hashes a copy). Either way, (re)record provenance for a confirmed zip so the next scan
        // — and any deactivate — is O(1) instead of re-hashing gigabytes.
        if matches_provenance(conn, &key, &link, &file_name) || is_silo_projection(&link, src, kind)
        {
            if kind == "zip" {
                if let Some((size, mtime)) = file_stats(&link) {
                    db::record_projection(conn, &key, &file_name, size, mtime);
                }
            }
            continue;
        }
        let tech_name = file_name
            .strip_suffix(".zip")
            .unwrap_or(&file_name)
            .to_string();
        out.push(ForeignFile {
            tech_name,
            file_name,
            kind: kind.to_string(),
            flat_version: mod_version(&link, kind),
            managed_version: mod_version(src, kind),
        });
    }
    out
}

/// Remove a projected entry (hardlink, symlink, junction, or copy) without
/// touching the archived original.
pub(crate) fn remove_link(link: &Path) -> std::io::Result<()> {
    let meta = link.symlink_metadata()?;
    let ft = meta.file_type();
    if ft.is_symlink() {
        // A dir symlink/junction must be removed with remove_dir on Windows.
        if link.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            std::fs::remove_dir(link)
        } else {
            std::fs::remove_file(link)
        }
    } else if ft.is_dir() {
        std::fs::remove_dir_all(link)
    } else {
        std::fs::remove_file(link)
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_silo_projection_distinguishes_ours_from_the_users() {
        let base = std::env::temp_dir().join(format!("silo_proj_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("archived_FS25_Foo.zip");
        std::fs::write(&src, b"PK\x03\x04 canonical bytes").unwrap();
        let link = base.join("FS25_Foo.zip");

        // Our hardlink projection → ours.
        std::fs::hard_link(&src, &link).unwrap();
        assert!(is_silo_projection(&link, &src, "zip"), "hardlink is ours");
        std::fs::remove_file(&link).unwrap();

        // A byte-identical copy (cross-volume fallback) → ours.
        std::fs::copy(&src, &link).unwrap();
        assert!(
            is_silo_projection(&link, &src, "zip"),
            "identical copy is ours"
        );
        std::fs::remove_file(&link).unwrap();

        // The user's own DIFFERENT build at the same name → NOT ours (must be preserved).
        std::fs::write(&link, b"PK\x03\x04 the user's different, longer build").unwrap();
        assert!(
            !is_silo_projection(&link, &src, "zip"),
            "a user's replacement must not read as Silo's"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn is_silo_projection_dir_requires_marker() {
        let base = std::env::temp_dir().join(format!("silo_projdir_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("archived_dir");
        std::fs::create_dir_all(&src).unwrap();
        let link = base.join("FS25_FooDir");
        std::fs::create_dir_all(&link).unwrap();
        std::fs::write(link.join("modDesc.xml"), b"<x/>").unwrap();

        // A plain user directory at the projected name → NOT ours.
        assert!(!is_silo_projection(&link, &src, "dir"));
        // With Silo's ownership marker → ours.
        std::fs::write(link.join(SILO_MARKER), b"silo\n").unwrap();
        assert!(is_silo_projection(&link, &src, "dir"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn dir_projection_copies_when_links_are_refused() {
        let base = std::env::temp_dir().join(format!("silo_dircopy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let src = base.join("archived_dir");
        std::fs::create_dir_all(src.join("textures")).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<x/>").unwrap();
        std::fs::write(src.join("textures").join("a.dds"), b"dds").unwrap();
        let refuse = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) // e.g. exFAT
        };

        // Filesystem refuses links → a marked copy, which reads as ours.
        let link = base.join("FS25_FooDir");
        link_dir_or_copy(&src, &link, refuse).unwrap();
        assert_eq!(std::fs::read(link.join("modDesc.xml")).unwrap(), b"<x/>");
        assert_eq!(
            std::fs::read(link.join("textures").join("a.dds")).unwrap(),
            b"dds"
        );
        assert!(is_silo_projection(&link, &src, "dir"));

        // Name already taken (the user's own folder) → no copy merged into it, no marker.
        let taken = base.join("FS25_UserDir");
        std::fs::create_dir_all(&taken).unwrap();
        let exists = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists))
        };
        assert!(link_dir_or_copy(&src, &taken, exists).is_err());
        assert!(!taken.join("modDesc.xml").exists());
        assert!(!is_silo_projection(&taken, &src, "dir"));

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_never_deletes_unknown_files() {
        let base = std::env::temp_dir().join(format!("silo_flatten_a_{}", std::process::id()));
        let archive = base.join(ARCHIVE);
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(archive.join("Maps")).unwrap(); // empty → should be removed
        std::fs::create_dir_all(archive.join("Vehicles")).unwrap();
        // An orphan the manifest doesn't know about (e.g. a move whose DB write crashed).
        let orphan = archive.join("Vehicles").join("orphan.zip");
        std::fs::write(&orphan, b"PK\x03\x04").unwrap();

        let leftover = cleanup_empty_archive(&archive, true).unwrap();

        assert!(orphan.exists(), "orphan file must NOT be deleted");
        assert_eq!(leftover.len(), 1);
        assert!(
            !archive.join("Maps").exists(),
            "empty dir should be removed"
        );
        assert!(archive.join("Vehicles").exists(), "non-empty dir kept");
        assert!(
            archive.exists(),
            "archive kept because it still holds the orphan"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flatten_never_deletes_the_original_when_no_archive_copy_exists() {
        // Simulate the crash window: a manifest row was written but the move never happened,
        // so the ORIGINAL still sits in the flat root and there is no archived copy. Flatten
        // must keep the file and only clear the stale row — never delete the only copy.
        let base = std::env::temp_dir().join(format!("silo_crashwin_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();

        let key = crate::modsroot::canonical_key(&base);

        let row = OrganizedRow {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
            active: false,
        };
        db::upsert_organized(&conn, &key, &row).unwrap();
        let original = base.join("FS25_Foo.zip");
        std::fs::write(&original, b"PK\x03\x04 the user's only copy").unwrap();
        // No archive/Vehicles/FS25_Foo.zip exists.

        let rep = flatten(&conn, &Layout::default_for(&base));

        assert!(original.exists(), "the only copy must NOT be deleted");
        assert!(
            db::load_organized(&conn, &key).is_empty(),
            "the stale manifest row should be cleared"
        );
        assert!(
            rep.errors.iter().any(|e| e.contains("kept the file")),
            "the kept-file case should be reported, got: {:?}",
            rep.errors
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleanup_removes_a_fully_empty_archive() {
        let base = std::env::temp_dir().join(format!("silo_flatten_b_{}", std::process::id()));
        let archive = base.join(ARCHIVE);
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(archive.join("A").join("B")).unwrap();

        let leftover = cleanup_empty_archive(&archive, true).unwrap();

        assert!(leftover.is_empty());
        assert!(!archive.exists(), "a fully-empty archive tree is removed");

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn detect_foreign_flags_only_non_projections() {
        let root = std::env::temp_dir().join(format!("silo_foreign_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let conn = db::open(&root.join("silo.db")).unwrap();
        let cat = root.join("archive").join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        let src = cat.join("FS25_Foo.zip");
        std::fs::write(&src, b"the real archived mod bytes").unwrap();

        // Nothing in the flat root → nothing flagged.
        assert!(detect_foreign_projections(&conn, &Layout::default_for(&root)).is_empty());

        // A genuine hardlink projection → NOT flagged (it's provably ours).
        let link = root.join("FS25_Foo.zip");
        std::fs::hard_link(&src, &link).unwrap();
        assert!(
            detect_foreign_projections(&conn, &Layout::default_for(&root)).is_empty(),
            "a real hardlink projection must not be flagged"
        );
        std::fs::remove_file(&link).unwrap();

        // A DIFFERENT file at that managed name → flagged as foreign.
        std::fs::write(&link, b"the user's own, different build").unwrap();
        let found = detect_foreign_projections(&conn, &Layout::default_for(&root));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].tech_name, "FS25_Foo");
        assert_eq!(found[0].file_name, "FS25_Foo.zip");

        // A loose zip with NO archived counterpart is an ordinary unorganized mod, not foreign.
        std::fs::write(root.join("FS25_Unrelated.zip"), b"loose").unwrap();
        assert_eq!(
            detect_foreign_projections(&conn, &Layout::default_for(&root)).len(),
            1
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    // A copy-projection deactivates via the provenance fast-path (no hash) once recorded, and a
    // file the user swapped in — same name, different bytes — is never removed.
    #[test]
    fn deactivate_uses_provenance_and_never_deletes_a_swapped_file() {
        let base = std::env::temp_dir().join(format!("silo_prov_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        let bytes = b"PK\x03\x04 canonical archived build";
        std::fs::write(cat.join("FS25_Foo.zip"), bytes).unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: false,
            },
        )
        .unwrap();

        // Activate: set_active copies (temp dir may support hardlinks, but either way it records
        // provenance) and the flat file exists.
        let mut want = std::collections::HashSet::new();
        want.insert("FS25_Foo".to_string());
        let rep = set_active(&conn, &Layout::default_for(&base), &want);
        assert_eq!(rep.changed, 1);
        let flat = base.join("FS25_Foo.zip");
        assert!(flat.exists());
        assert!(
            db::projection_stats(&conn, &key, "FS25_Foo.zip").is_some(),
            "provenance recorded on activate"
        );
        assert!(matches_provenance(&conn, &key, &flat, "FS25_Foo.zip"));

        // Deactivate: provenance matches → removed, provenance forgotten, archive untouched.
        let rep = set_active(
            &conn,
            &Layout::default_for(&base),
            &std::collections::HashSet::new(),
        );
        assert_eq!(rep.changed, 1);
        assert!(!flat.exists(), "our own projection is removed");
        assert!(db::projection_stats(&conn, &key, "FS25_Foo.zip").is_none());
        assert!(cat.join("FS25_Foo.zip").exists(), "archive kept");

        // Now the user drops in their OWN different build at that name, and marks it active in the
        // manifest (as if it were projected). A stale/absent provenance record must NOT let it be
        // deleted — different bytes, so both the fast path and the hash check refuse.
        std::fs::write(&flat, b"the user's OWN irreplaceable build").unwrap();
        db::set_organized_active(&conn, &key, "FS25_Foo", true).unwrap();
        let rep = set_active(
            &conn,
            &Layout::default_for(&base),
            &std::collections::HashSet::new(),
        );
        assert_eq!(rep.skipped, 1, "a swapped-in file is left untouched");
        assert_eq!(
            std::fs::read(&flat).unwrap(),
            b"the user's OWN irreplaceable build",
            "the user's file must survive"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn adopt_promotes_the_mods_folder_file_and_backs_up_the_managed_copy() {
        let base = std::env::temp_dir().join(format!("silo_adopt_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        std::fs::write(cat.join("FS25_Foo.zip"), b"OLD archived build").unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: false,
            },
        )
        .unwrap();
        std::fs::write(base.join("FS25_Foo.zip"), b"NEWER mods-folder build").unwrap();

        adopt_foreign(&conn, &Layout::default_for(&base), "FS25_Foo.zip").unwrap();

        assert_eq!(
            std::fs::read(cat.join("FS25_Foo.zip")).unwrap(),
            b"NEWER mods-folder build",
            "the mods-folder build becomes the managed copy"
        );
        assert!(
            !base.join("FS25_Foo.zip").exists(),
            "parked → the flat slot is emptied"
        );
        let baks: Vec<_> = std::fs::read_dir(base.join("backups"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(baks.len(), 1, "the old managed copy is backed up");
        assert_eq!(
            std::fs::read(baks[0].path()).unwrap(),
            b"OLD archived build"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn restore_puts_silos_copy_back_and_backs_up_the_swap() {
        let base = std::env::temp_dir().join(format!("silo_restore_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        std::fs::write(cat.join("FS25_Foo.zip"), b"MANAGED build").unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        std::fs::write(base.join("FS25_Foo.zip"), b"the user's swapped-in build").unwrap();

        restore_projection(&conn, &Layout::default_for(&base), "FS25_Foo.zip").unwrap();

        assert_eq!(
            std::fs::read(base.join("FS25_Foo.zip")).unwrap(),
            b"MANAGED build",
            "the flat slot is re-projected from the managed copy"
        );
        let baks: Vec<_> = std::fs::read_dir(base.join("backups"))
            .unwrap()
            .flatten()
            .collect();
        assert_eq!(baks.len(), 1, "the swapped-in build is backed up");
        assert_eq!(
            std::fs::read(baks[0].path()).unwrap(),
            b"the user's swapped-in build"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn relink_collapses_a_byte_identical_copy_into_a_hardlink() {
        let base = std::env::temp_dir().join(format!("silo_relink_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        let bytes = b"PK\x03\x04 the canonical archived mod";
        std::fs::write(cat.join("FS25_Foo.zip"), bytes).unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        // A COPY projection: same bytes, distinct file id (what set_active leaves when the
        // flat file pre-existed).
        let flat = base.join("FS25_Foo.zip");
        std::fs::write(&flat, bytes).unwrap();
        assert_ne!(
            same_file::Handle::from_path(&flat).unwrap(),
            same_file::Handle::from_path(cat.join("FS25_Foo.zip")).unwrap(),
            "precondition: the copy is a distinct file"
        );

        let rep = relink_projections(&conn, &Layout::default_for(&base), |_, _| {});

        assert_eq!(rep.upgraded, 1);
        assert_eq!(rep.reclaimed_bytes, bytes.len() as u64);
        assert!(rep.errors.is_empty(), "no errors: {:?}", rep.errors);
        // Now provably Silo's hardlink → cheap to identify, and content preserved.
        assert!(is_silo_projection(&flat, &cat.join("FS25_Foo.zip"), "zip"));
        assert_eq!(std::fs::read(&flat).unwrap(), bytes);
        // The new hardlink's own (size, mtime) is recorded, so deactivate takes the fast path.
        assert_eq!(
            db::projection_stats(&conn, &key, "FS25_Foo.zip"),
            file_stats(&flat)
        );
        assert!(matches_provenance(&conn, &key, &flat, "FS25_Foo.zip"));

        // Idempotent: a second pass finds it already linked, changes nothing.
        let rep2 = relink_projections(&conn, &Layout::default_for(&base), |_, _| {});
        assert_eq!(rep2.upgraded, 0);
        assert_eq!(rep2.already_linked, 1);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn relink_never_touches_the_users_own_replacement() {
        let base = std::env::temp_dir().join(format!("silo_relink_skip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        std::fs::write(cat.join("FS25_Foo.zip"), b"MANAGED build").unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        let flat = base.join("FS25_Foo.zip");
        std::fs::write(&flat, b"the user's OWN different build").unwrap();

        let rep = relink_projections(&conn, &Layout::default_for(&base), |_, _| {});

        // (This machine's temp dir supports hardlinks, so the cloud short-circuit is off here.)
        assert!(!rep.hardlinks_unsupported);
        assert_eq!(rep.upgraded, 0);
        assert_eq!(rep.skipped, 1, "a different file is left untouched");
        assert_eq!(
            std::fs::read(&flat).unwrap(),
            b"the user's OWN different build",
            "the user's file must be preserved byte-for-byte"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn records_from_one_folder_never_authorize_changes_in_another() {
        let base = std::env::temp_dir().join(format!("silo_iso_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(a.join("archive").join("Vehicles")).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();

        // Folder A: one organized mod, projected (active).
        std::fs::write(a.join("archive/Vehicles/FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let key_a = crate::modsroot::canonical_key(&a);
        let row = OrganizedRow {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
            active: false,
        };
        db::upsert_organized(&conn, &key_a, &row).unwrap();
        let on: HashSet<String> = ["FS25_Foo".to_string()].into_iter().collect();
        set_active(&conn, &Layout::default_for(&a), &on);
        assert!(a.join("FS25_Foo.zip").exists());

        // Folder B: the user's own file at the same name, unknown to Silo.
        std::fs::write(b.join("FS25_Foo.zip"), b"PK\x03\x04 the user's own file").unwrap();

        // Deactivating everything in B must not touch B's file, and must not touch A.
        set_active(&conn, &Layout::default_for(&b), &HashSet::new());
        flatten(&conn, &Layout::default_for(&b));
        assert!(
            b.join("FS25_Foo.zip").exists(),
            "B's file is not Silo's — never removed"
        );
        assert!(
            a.join("FS25_Foo.zip").exists(),
            "A's projection is untouched"
        );
        assert!(
            db::load_organized(&conn, &key_a)[0].active,
            "A's state is untouched"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn organize_activate_and_flatten_honor_a_custom_archive() {
        let base = std::env::temp_dir().join(format!("silo_customarch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("elsewhere");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&arch).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, &mods, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        assert!(
            arch.join("Vehicles/FS25_Foo.zip").exists(),
            "filed into the custom archive"
        );
        assert!(!mods.join("archive").exists(), "default archive untouched");

        let on: HashSet<String> = ["FS25_Foo".to_string()].into_iter().collect();
        let rep = set_active(&conn, &layout, &on);
        assert_eq!(rep.changed, 1);
        assert!(
            mods.join("FS25_Foo.zip").exists(),
            "projected into the mods folder"
        );

        let rep = flatten(&conn, &layout);
        assert!(rep.errors.is_empty(), "no errors: {:?}", rep.errors);
        assert_eq!(rep.changed, 1);
        let restored = mods.join("FS25_Foo.zip");
        let meta = std::fs::symlink_metadata(&restored).unwrap();
        assert!(
            meta.file_type().is_file() && !meta.file_type().is_symlink(),
            "restored as a regular file"
        );
        assert_eq!(std::fs::read(&restored).unwrap(), b"PK\x03\x04 foo");
        assert!(
            !arch.join("Vehicles").exists(),
            "the emptied category folder is removed"
        );
        assert!(
            arch.exists(),
            "a user-chosen archive folder is never removed"
        );
        assert!(
            db::load_organized(&conn, &crate::modsroot::canonical_key(&mods)).is_empty(),
            "the manifest is cleared"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flatten_removes_an_emptied_default_archive() {
        let base = std::env::temp_dir().join(format!("silo_defarch_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout::default_for(&mods);
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, &mods, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        assert!(mods.join("archive/Vehicles/FS25_Foo.zip").exists());

        let rep = flatten(&conn, &layout);
        assert!(rep.errors.is_empty(), "no errors: {:?}", rep.errors);
        assert!(mods.join("FS25_Foo.zip").exists());
        assert!(
            !mods.join("archive").exists(),
            "Silo's own emptied archive folder is removed"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn organize_never_overwrites_a_file_already_in_the_archive() {
        let base = std::env::temp_dir().join(format!("silo_noclobber_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(arch.join("Vehicles")).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK\x03\x04 the loose build").unwrap();
        std::fs::write(
            arch.join("Vehicles/FS25_Foo.zip"),
            b"PK\x03\x04 a different build already in the archive",
        )
        .unwrap();
        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, &mods, std::slice::from_ref(&input));

        assert_eq!(rep.changed, 0);
        assert_eq!(rep.skipped, 1);
        assert!(
            rep.errors.iter().any(|e| e.contains("already exists")),
            "the collision is reported, got: {:?}",
            rep.errors
        );
        assert_eq!(
            std::fs::read(mods.join("FS25_Foo.zip")).unwrap(),
            b"PK\x03\x04 the loose build"
        );
        assert_eq!(
            std::fs::read(arch.join("Vehicles/FS25_Foo.zip")).unwrap(),
            b"PK\x03\x04 a different build already in the archive"
        );
        assert!(
            db::load_organized(&conn, &crate::modsroot::canonical_key(&mods)).is_empty(),
            "no manifest row for a mod that wasn't moved"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn organize_never_moves_a_folder_into_itself() {
        let base = std::env::temp_dir().join(format!("silo_selfnest_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        // A loose folder mod at the archive's top level named exactly like its category.
        std::fs::create_dir_all(arch.join("Tools")).unwrap();
        std::fs::write(arch.join("Tools/modDesc.xml"), b"<modDesc/>").unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        let input = ModInput {
            tech_name: "Tools".into(),
            file_name: "Tools".into(),
            kind: "dir".into(),
            category: "Tools".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, &arch, std::slice::from_ref(&input));

        assert_eq!(rep.changed, 0);
        assert_eq!(rep.skipped, 1, "counted as skipped");
        assert_eq!(rep.errors.len(), 1, "reported: {:?}", rep.errors);
        assert!(arch.join("Tools/modDesc.xml").exists(), "left in place");
        assert!(!arch.join("Tools/Tools").exists(), "not nested into itself");
        assert!(db::load_organized(&conn, &crate::modsroot::canonical_key(&mods)).is_empty());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flatten_leaves_a_user_archive_folders_own_content_alone() {
        let base = std::env::temp_dir().join(format!("silo_ownedonly_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(arch.join("ToSort")).unwrap();
        std::fs::write(arch.join("notes.txt"), b"my notes").unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK\x03\x04 foo").unwrap();
        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };

        let rep = apply_organize(&conn, &layout, &mods, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        let rep = flatten(&conn, &layout);

        assert!(
            arch.join("ToSort").is_dir(),
            "the user's empty folder is kept"
        );
        assert!(arch.join("notes.txt").is_file(), "the user's file is kept");
        assert!(
            !arch.join("Vehicles").exists(),
            "the emptied category folder is removed"
        );
        assert!(arch.exists(), "the archive root is kept");
        assert!(
            !rep.errors.iter().any(|e| e.contains("unrecognized")),
            "the user's own files aren't reported: {:?}",
            rep.errors
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn archive_is_default_compares_canonically() {
        let mods = std::env::temp_dir().join(format!("silo_isdef_{}", std::process::id()));
        assert!(Layout::default_for(&mods).archive_is_default());
        let custom = Layout {
            mods: mods.clone(),
            archive: mods.with_file_name("silo_isdef_elsewhere"),
        };
        assert!(!custom.archive_is_default());
    }

    #[cfg(windows)]
    #[test]
    fn archive_is_default_ignores_case_on_windows() {
        let mods = std::env::temp_dir().join(format!("silo_isdefcase_{}", std::process::id()));
        let shouty = PathBuf::from(mods.to_string_lossy().to_uppercase()).join("ARCHIVE");
        let layout = Layout {
            mods: mods.clone(),
            archive: shouty,
        };
        assert!(layout.archive_is_default());
    }

    #[test]
    fn layout_new_accepts_the_default_and_a_sibling() {
        let base = std::env::temp_dir().join(format!("silo_lnew_ok_{}", std::process::id()));
        let mods = base.join("mods");
        let def = Layout::new(mods.clone(), mods.join(ARCHIVE)).unwrap();
        assert!(def.archive_is_default());
        let sib = Layout::new(mods.clone(), base.join("hdd")).unwrap();
        assert_eq!(sib.archive, base.join("hdd"));
        assert!(!sib.archive_is_default());
    }

    #[test]
    fn layout_new_rejects_unsafe_archives() {
        let base = std::env::temp_dir().join(format!("silo_lnew_bad_{}", std::process::id()));
        let mods = base.join("mods");
        assert!(
            Layout::new(PathBuf::from("mods"), base.join("hdd")).is_err(),
            "relative mods"
        );
        assert!(
            Layout::new(mods.clone(), PathBuf::from("hdd")).is_err(),
            "relative archive"
        );
        assert!(
            Layout::new(mods.clone(), mods.clone()).is_err(),
            "same folder"
        );
        assert!(
            Layout::new(mods.clone(), base.clone()).is_err(),
            "archive containing the mods folder"
        );
        assert!(
            Layout::new(mods.clone(), mods.join("stash")).is_err(),
            "non-default archive inside the mods folder"
        );
        assert!(
            Layout::new(mods.clone(), mods.join("archive").join("deeper")).is_err(),
            "inside the default archive is still inside the mods folder"
        );
    }

    #[test]
    fn adopting_files_loose_archive_mods_into_categories() {
        // Distinct temp name: `silo_adopt_{pid}` is already used by the adopt_foreign test,
        // and tests run in parallel in one process.
        let base = std::env::temp_dir().join(format!("silo_adoptloose_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&arch).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        std::fs::write(arch.join("FS25_Loose.zip"), b"PK\x03\x04 loose").unwrap();
        let input = ModInput {
            tech_name: "FS25_Loose".into(),
            file_name: "FS25_Loose.zip".into(),
            kind: "zip".into(),
            category: "Tools".into(),
            subcategory: None,
        };

        let plan = plan_organize(&arch, std::slice::from_ref(&input));
        assert_eq!(plan.len(), 1);
        let rep = apply_organize(&conn, &layout, &arch, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        assert!(arch.join("Tools/FS25_Loose.zip").exists());
        assert!(!arch.join("FS25_Loose.zip").exists());
        assert!(
            !mods.join("FS25_Loose.zip").exists(),
            "adopted mods arrive parked"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_category_never_resolves_to_or_escapes_the_archive() {
        let arch = std::env::temp_dir().join(format!("silo_catdots_{}", std::process::id()));
        // Names that are empty once Windows drops trailing dots/spaces would name the archive.
        for cat in ["..", ".", "", "  ", " .. ", "...", ". ."] {
            let dir = archive_dir(&arch, cat);
            assert_eq!(dir, arch.join("_"), "category {cat:?}");
            assert!(dir.starts_with(&arch) && dir != arch, "category {cat:?}");
        }
        // Ordinary names (and dotted ones that aren't empty when collapsed) are unchanged, so
        // existing archive folders keep their names.
        assert_eq!(archive_dir(&arch, "Vehicles"), arch.join("Vehicles"));
        assert_eq!(archive_dir(&arch, "a/b"), arch.join("a_b"));
        assert_eq!(archive_dir(&arch, "v1.0"), arch.join("v1.0"));
        assert_eq!(archive_dir(&arch, "a. "), arch.join("a. "));
    }

    fn other_err(_: &Path, _: &Path) -> std::io::Result<()> {
        Err(std::io::ErrorKind::Other.into())
    }

    #[test]
    fn zip_projection_falls_back_hardlink_then_symlink_then_copy() {
        let base = std::env::temp_dir().join(format!("silo_zlink_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("a.zip");
        std::fs::write(&src, b"PK\x03\x04 a").unwrap();

        // Hardlink works → the symlink policy is never even consulted.
        let l0 = base.join("l0.zip");
        link_zip(
            &src,
            &l0,
            || panic!("policy must not be consulted when the hardlink works"),
            |s, l| std::fs::hard_link(s, l),
            |_, _| panic!("symlink must not be attempted"),
        )
        .unwrap();

        // Hardlink refused, symlink allowed → a symlink resolving to src.
        let l1 = base.join("l1.zip");
        link_zip(&src, &l1, || true, other_err, symlink_file).unwrap();
        assert!(l1.symlink_metadata().unwrap().file_type().is_symlink() || !cfg!(unix));
        assert!(is_silo_projection(&l1, &src, "zip"));

        // Hardlink and symlink refused → a byte copy.
        let l2 = base.join("l2.zip");
        link_zip(&src, &l2, || true, other_err, other_err).unwrap();
        assert_eq!(std::fs::read(&l2).unwrap(), b"PK\x03\x04 a");

        // Symlinks not allowed → never tried.
        let l3 = base.join("l3.zip");
        link_zip(
            &src,
            &l3,
            || false,
            other_err,
            |_, _| panic!("symlink must not be attempted"),
        )
        .unwrap();
        assert_eq!(std::fs::read(&l3).unwrap(), b"PK\x03\x04 a");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn zip_projection_never_overwrites_an_existing_file() {
        let base = std::env::temp_dir().join(format!("silo_znoclobber_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("a.zip");
        std::fs::write(&src, b"PK\x03\x04 the archived build").unwrap();
        let theirs = base.join("theirs.zip");
        std::fs::write(&theirs, b"the user's own file").unwrap();

        // Real strategies: the hardlink reports AlreadyExists → stop, nothing else is tried.
        let e = link_zip(
            &src,
            &theirs,
            || panic!("policy must not be consulted after AlreadyExists"),
            |s, l| std::fs::hard_link(s, l),
            |_, _| panic!("symlink must not be attempted after AlreadyExists"),
        )
        .unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&theirs).unwrap(), b"the user's own file");

        // A symlink reporting AlreadyExists → no copy attempt either.
        let e = link_zip(
            &src,
            &theirs,
            || true,
            other_err,
            |_, _| Err(std::io::ErrorKind::AlreadyExists.into()),
        )
        .unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);

        // Even if both link strategies fail for another reason, the copy fallback refuses to
        // truncate or replace the existing file.
        assert!(link_zip(&src, &theirs, || true, other_err, other_err).is_err());
        assert_eq!(std::fs::read(&theirs).unwrap(), b"the user's own file");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn copy_fallback_leaves_nothing_behind_when_the_source_is_missing() {
        let base = std::env::temp_dir().join(format!("silo_zcopyfail_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let dst = base.join("l.zip");
        assert!(link_zip(
            &base.join("missing.zip"),
            &dst,
            || true,
            other_err,
            other_err
        )
        .is_err());
        assert!(
            dst.symlink_metadata().is_err(),
            "no partial file under the name"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn symlink_probe_never_creates_the_archive_and_cleans_up() {
        let base = std::env::temp_dir().join(format!("silo_symprobe_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        let missing = Layout {
            mods: mods.clone(),
            archive: base.join("hdd"),
        };
        assert!(!supports_symlinks(&missing));
        assert!(
            !base.join("hdd").exists(),
            "the probe must not create the archive"
        );

        std::fs::create_dir_all(base.join("hdd")).unwrap();
        let _ = supports_symlinks(&missing);
        assert_eq!(
            std::fs::read_dir(&mods).unwrap().count(),
            0,
            "no probe left in mods"
        );
        assert_eq!(
            std::fs::read_dir(base.join("hdd")).unwrap().count(),
            0,
            "no probe left in the archive"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_missing_source_never_becomes_a_dangling_link() {
        let base = std::env::temp_dir().join(format!("silo_zdangle_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let missing = base.join("missing.zip");

        // Real hardlink (fails NotFound); a "symlink" that would succeed regardless of the
        // target, as a real one does on Linux/macOS. The policy allows symlinks.
        let l1 = base.join("l1.zip");
        let e = link_zip(
            &missing,
            &l1,
            || true,
            |s, l| std::fs::hard_link(s, l),
            |_, l| std::fs::write(l, b""),
        )
        .unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::NotFound);
        assert!(l1.symlink_metadata().is_err(), "nothing at the link path");

        // Even if the hardlink failed for another reason, no symlink is made to a missing file.
        let l2 = base.join("l2.zip");
        assert!(link_zip(
            &missing,
            &l2,
            || true,
            other_err,
            |_, l| std::fs::write(l, b"")
        )
        .is_err());
        assert!(l2.symlink_metadata().is_err(), "nothing at the link path");

        // make_link refuses a missing source up front, for folder mods too.
        let layout = Layout::default_for(&base);
        for (name, kind) in [("l3.zip", "zip"), ("l3dir", "dir")] {
            let link = base.join(name);
            let src = base.join(format!("missing_{name}"));
            assert!(make_link(&src, &link, kind, &mut LinkPolicy::new(&layout)).is_err());
            assert!(link.symlink_metadata().is_err(), "nothing at {name}");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn folder_mods_in_a_mac_app_container_are_copied_not_linked() {
        let container = Layout {
            mods: PathBuf::from(
                "/Users/me/Library/Containers/com.astragon.farmingsim25/Data/Library/Application Support/FarmingSimulator2025/mods",
            ),
            archive: PathBuf::from("/Volumes/HDD/silo"),
        };
        assert!(LinkPolicy::new(&container).mods_in_container());
        let plain = Layout::default_for(Path::new(
            "/Users/me/Library/Application Support/FarmingSimulator2025/mods",
        ));
        assert!(!LinkPolicy::new(&plain).mods_in_container());
    }

    /// End to end on a real (fake) container path: the dir projection is a marked copy.
    #[cfg(unix)]
    #[test]
    fn make_link_copies_a_folder_mod_inside_a_container() {
        let base = std::env::temp_dir().join(format!("silo_container_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("Library/Containers/com.example.fs25/Data/mods");
        let archive = base.join("hdd");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(archive.join("Tools/FS25_Dir")).unwrap();
        std::fs::write(archive.join("Tools/FS25_Dir/modDesc.xml"), b"<modDesc/>").unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: archive.clone(),
        };
        let link = mods.join("FS25_Dir");
        make_link(
            &archive.join("Tools/FS25_Dir"),
            &link,
            "dir",
            &mut LinkPolicy::new(&layout),
        )
        .unwrap();
        let meta = link.symlink_metadata().unwrap();
        assert!(
            meta.is_dir() && !meta.file_type().is_symlink(),
            "a real folder"
        );
        assert!(link.join(SILO_MARKER).is_file(), "marked as Silo's copy");
        assert!(link.join("modDesc.xml").is_file());
        // Never merges into an existing folder.
        assert!(make_link(
            &archive.join("Tools/FS25_Dir"),
            &link,
            "dir",
            &mut LinkPolicy::new(&layout)
        )
        .is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn mac_app_store_containers_are_detected() {
        assert!(in_mac_app_container(Path::new(
            "/Users/me/Library/Containers/com.astragon.farmingsim25/Data/Library/Application Support/FarmingSimulator2025/mods"
        )));
        // APFS is case-insensitive by default.
        assert!(in_mac_app_container(Path::new(
            "/Users/me/library/CONTAINERS/com.astragon.farmingsim25/Data/mods"
        )));
        assert!(!in_mac_app_container(Path::new(
            "/Users/me/Library/Application Support/FarmingSimulator2025/mods"
        )));
        assert!(!in_mac_app_container(Path::new(
            r"C:\Users\me\Documents\My Games\FarmingSimulator2025\mods"
        )));
    }

    #[test]
    fn mount_of_finds_the_volume_of_an_existing_folder() {
        let base = std::env::temp_dir().join(format!("silo_mount_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("sub")).unwrap();
        // Regression: Windows' canonical `\\?\C:\…` form must still match the `C:\` mount.
        let (mount, _free) = mount_of(&base).expect("the temp dir sits on a known volume");
        assert!(crate::modsroot::canonical_key(&base).starts_with(
            &crate::modsroot::normalize_key(&mount.to_string_lossy(), cfg!(windows))
        ));
        assert!(same_volume(&base, &base.join("sub")));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_foreign_file_or_link_is_never_mistaken_for_a_symlink_projection() {
        let base = std::env::temp_dir().join(format!("silo_zforeign_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let src = base.join("a.zip");
        std::fs::write(&src, b"PK\x03\x04 a").unwrap();
        let theirs = base.join("theirs.zip");
        std::fs::write(&theirs, b"PK\x03\x04 different").unwrap();
        assert!(!is_silo_projection(&theirs, &src, "zip"));

        #[cfg(unix)]
        {
            // A dangling symlink is not ours.
            let dangling = base.join("dangling.zip");
            std::os::unix::fs::symlink(base.join("gone.zip"), &dangling).unwrap();
            assert!(!is_silo_projection(&dangling, &src, "zip"));
            // A symlink to a different file is not ours — even one with identical bytes.
            let to_theirs = base.join("to_theirs.zip");
            std::os::unix::fs::symlink(&theirs, &to_theirs).unwrap();
            assert!(!is_silo_projection(&to_theirs, &src, "zip"));
            let twin = base.join("twin.zip");
            std::fs::write(&twin, b"PK\x03\x04 a").unwrap();
            let to_twin = base.join("to_twin.zip");
            std::os::unix::fs::symlink(&twin, &to_twin).unwrap();
            assert!(!is_silo_projection(&to_twin, &src, "zip"));
            // A symlink to the archived file is.
            let ours = base.join("ours.zip");
            std::os::unix::fs::symlink(&src, &ours).unwrap();
            assert!(is_silo_projection(&ours, &src, "zip"));
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A default-layout mods folder holding one organized zip `FS25_Foo.zip` in `Vehicles`.
    #[cfg(unix)]
    fn symlink_fixture(tag: &str, active: bool) -> (PathBuf, Connection, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("silo_sym_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let cat = base.join(ARCHIVE).join("Vehicles");
        std::fs::create_dir_all(&cat).unwrap();
        let src = cat.join("FS25_Foo.zip");
        std::fs::write(&src, b"PK\x03\x04 the archived build").unwrap();
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Foo".into(),
                file_name: "FS25_Foo.zip".into(),
                kind: "zip".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active,
            },
        )
        .unwrap();
        let flat = base.join("FS25_Foo.zip");
        (base, conn, src, flat)
    }

    #[cfg(unix)]
    #[test]
    fn deactivate_removes_a_silo_symlink_and_keeps_the_target() {
        let (base, conn, src, flat) = symlink_fixture("deact", true);
        std::os::unix::fs::symlink(&src, &flat).unwrap();
        let rep = set_active(&conn, &Layout::default_for(&base), &HashSet::new());
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(flat.symlink_metadata().is_err(), "the symlink is gone");
        assert_eq!(
            std::fs::read(&src).unwrap(),
            b"PK\x03\x04 the archived build"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn flatten_replaces_a_silo_symlink_with_the_archived_file() {
        let (base, conn, src, flat) = symlink_fixture("flat", true);
        std::os::unix::fs::symlink(&src, &flat).unwrap();
        let rep = flatten(&conn, &Layout::default_for(&base));
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        let meta = flat.symlink_metadata().unwrap();
        assert!(meta.file_type().is_file(), "a real file, not a link");
        assert_eq!(
            std::fs::read(&flat).unwrap(),
            b"PK\x03\x04 the archived build"
        );
        assert!(!src.exists(), "moved back out of the archive");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn relink_across_drives_upgrades_a_copy_to_a_symlink() {
        let (base, conn, src, flat) = symlink_fixture("relink", true);
        std::fs::copy(&src, &flat).unwrap();
        let layout = Layout::default_for(&base);
        let key = crate::modsroot::canonical_key(&base);
        db::record_projection(&conn, &key, "FS25_Foo.zip", 1, 1);

        let rep = relink_with(&conn, &layout, RelinkMode::Symlink, |_, _| {});
        assert_eq!(rep.upgraded, 1, "errors: {:?}", rep.errors);
        assert!(flat.symlink_metadata().unwrap().file_type().is_symlink());
        assert_eq!(std::fs::read(&flat).unwrap(), std::fs::read(&src).unwrap());
        assert!(is_silo_projection(&flat, &src, "zip"));
        assert!(
            db::projection_stats(&conn, &key, "FS25_Foo.zip").is_none(),
            "the stale copy's provenance is forgotten"
        );

        let rep2 = relink_with(&conn, &layout, RelinkMode::Symlink, |_, _| {});
        assert_eq!(rep2.upgraded, 0);
        assert_eq!(rep2.already_linked, 1);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn relink_skips_a_foreign_symlink() {
        let (base, conn, _src, flat) = symlink_fixture("relinkforeign", true);
        let theirs = base.join("theirs.zip");
        std::fs::write(&theirs, b"PK\x03\x04 the archived build").unwrap();
        std::os::unix::fs::symlink(&theirs, &flat).unwrap();
        let rep = relink_with(
            &conn,
            &Layout::default_for(&base),
            RelinkMode::Symlink,
            |_, _| {},
        );
        assert_eq!(rep.skipped, 1);
        assert_eq!(rep.upgraded, 0);
        assert_eq!(std::fs::read_link(&flat).unwrap(), theirs, "left untouched");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn adopt_refuses_a_symlink_in_the_mods_folder() {
        let (base, conn, src, flat) = symlink_fixture("adoptlink", true);
        let theirs = base.join("theirs.zip");
        std::fs::write(&theirs, b"PK\x03\x04 elsewhere").unwrap();
        std::os::unix::fs::symlink(&theirs, &flat).unwrap();
        let err = adopt_foreign(&conn, &Layout::default_for(&base), "FS25_Foo.zip").unwrap_err();
        assert!(err.contains("link"), "{err}");
        assert_eq!(std::fs::read_link(&flat).unwrap(), theirs, "link untouched");
        assert_eq!(
            std::fs::read(&src).unwrap(),
            b"PK\x03\x04 the archived build"
        );
        assert!(!base.join("backups").exists(), "nothing backed up");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn relink_reports_why_it_cannot_link() {
        let base = std::env::temp_dir().join(format!("silo_relinkwhy_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout::default_for(&base);
        let cloud = relink_with(&conn, &layout, RelinkMode::SameVolumeNoHardlinks, |_, _| {});
        assert!(cloud.hardlinks_unsupported && !cloud.cross_drive_copies);
        let cross = relink_with(&conn, &layout, RelinkMode::CrossDriveNoSymlinks, |_, _| {});
        assert!(cross.cross_drive_copies && !cross.hardlinks_unsupported);
        let _ = std::fs::remove_dir_all(&base);
    }
}
