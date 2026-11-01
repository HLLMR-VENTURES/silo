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

    /// A custom archive whose folder isn't there (its drive unplugged, or moved away). The
    /// default archive is never "missing" — it simply may not exist yet.
    pub fn archive_missing(&self) -> bool {
        !self.archive_is_default() && !self.archive.is_dir()
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
    /// Not errors, but worth telling the user (e.g. where an edited folder copy was kept).
    pub notices: Vec<String>,
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
    let a = root.join(HARDLINK_PROBE);
    let b = root.join(format!("{HARDLINK_PROBE}.link"));
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
                "{}: {} would be filed inside itself, so Silo left it untouched.",
                m.tech_name, m.file_name
            ));
            rep.skipped += 1;
            continue;
        }
        // Never overwrite: a rename replaces an existing destination, and whatever sits there
        // (another build, the user's own file) may be the only copy.
        if to.symlink_metadata().is_ok() {
            rep.errors.push(format!(
                "{}: {} already exists in the archive, so Silo left both untouched.",
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
            Ok(()) => {
                rep.changed += 1;
                // Keep what the scan already knows about this mod, so the rescan after filing
                // doesn't parse it again (#322). Best effort: a miss only costs a fresh parse.
                let _ = db::move_cache_row(conn, &from.to_string_lossy(), &to.to_string_lossy());
            }
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
        sweep_copy_temp(&link);
        // A link into a folder that's gone (a re-pointed archive's old drive) would otherwise
        // count as "already linked" and the mod could never be activated or parked again.
        let stale = match remove_dangling_link(&link) {
            Ok(removed) => removed,
            Err(e) => {
                rep.errors
                    .push(format!("{}: remove stale link: {e}", row.tech_name));
                continue;
            }
        };
        if stale {
            db::forget_projection(conn, &key, &row.file_name);
            if !want {
                if let Err(e) = db::set_organized_active(conn, &key, &row.tech_name, false) {
                    rep.errors.push(format!("{}: manifest: {e}", row.tech_name));
                }
                rep.changed += 1;
                continue;
            }
        }
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
                    "{}: there's no copy in your library archive, so Silo kept the file in your mods folder. It won't delete the only copy.",
                    row.tech_name
                ));
                rep.skipped += 1;
            } else if !(matches_provenance(conn, &key, &link, &row.file_name)
                || is_silo_projection(&link, &src, &row.kind))
            {
                rep.errors.push(format!(
                    "{}: the file in your mods folder isn't the one Silo linked (did you replace it?), so Silo left it alone.",
                    row.tech_name
                ));
                rep.skipped += 1;
            } else {
                match clear_projection(&link, &src, root) {
                    Ok(kept) => {
                        if let Some(kept) = kept {
                            rep.notices
                                .push(kept_copy_notice(&row.file_name, &kept, false));
                        }
                        db::forget_projection(conn, &key, &row.file_name);
                        if let Err(e) = db::set_organized_active(conn, &key, &row.tech_name, false)
                        {
                            rep.errors.push(format!("{}: manifest: {e}", row.tech_name));
                        }
                        rep.changed += 1;
                    }
                    Err(e) => rep.errors.push(format!("{}: {e}", row.tech_name)),
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
pub(crate) fn file_stats(path: &Path) -> Option<(u64, i64)> {
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
    relink_with(conn, layout, relink_mode(layout), progress)
}

/// How a relink pass may link here. Same volume (`same_volume_for_links`: on a mount the disk
/// list doesn't know, where a probe hardlink works) → hardlinks, unless the folder is
/// cloud-synced (every attempt would error 396 — copies stay). Archive on another drive →
/// symlinks, when the OS allows them.
fn relink_mode(layout: &Layout) -> RelinkMode {
    if same_volume_for_links(layout) {
        if supports_hardlinks(&layout.mods) {
            RelinkMode::Hardlink
        } else {
            RelinkMode::SameVolumeNoHardlinks
        }
    } else if symlinks_usable(layout) {
        RelinkMode::Symlink
    } else {
        RelinkMode::CrossDriveNoSymlinks
    }
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
    let tmp = relink_tmp(flat);
    let _ = std::fs::remove_file(&tmp);
    if hard {
        std::fs::hard_link(src, &tmp)?;
    } else {
        symlink_file(src, &tmp)?;
    }
    match std::fs::rename(&tmp, flat) {
        Ok(()) => {
            crate::fslog::moved("link", src, flat);
            Ok(())
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

/// Temp sibling for an atomic link replacement in the mods folder. `scan` skips it, and an
/// archive move clears stale ones.
pub(crate) const RELINK_SUFFIX: &str = ".silo-relink-tmp";

fn relink_tmp(link: &Path) -> PathBuf {
    let name = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "mod".into());
    link.with_file_name(format!("{name}{RELINK_SUFFIX}"))
}

/// Atomically swap a zip's mods-folder entry `link` for a new link to `dst` (hardlink →
/// symlink → copy, per `policy`): make the link at a `RELINK_SUFFIX` temp sibling, then rename
/// it over `link`. At every point `link` is either the old working entry or the new one; a crash
/// leaves at most the temp, which the next attempt replaces (and `scan` skips).
pub(crate) fn replace_zip_link(
    link: &Path,
    dst: &Path,
    policy: &mut LinkPolicy,
) -> std::io::Result<()> {
    let tmp = relink_tmp(link);
    let _ = remove_path(&tmp);
    make_link(dst, &tmp, "zip", policy)?;
    std::fs::rename(&tmp, link).inspect_err(|_| {
        let _ = remove_path(&tmp);
    })?;
    crate::fslog::moved("move", &tmp, link);
    Ok(())
}

/// A zip's mods-folder entry `link` was just (re)made by Silo: refresh its projection record so
/// later deactivates prove it's Silo's without hashing. A symlink is identified structurally and
/// keeps none.
pub(crate) fn note_zip_projection(conn: &Connection, key: &str, file_name: &str, link: &Path) {
    db::forget_projection(conn, key, file_name);
    if let Some((size, mtime)) = file_stats(link) {
        db::record_projection(conn, key, file_name, size, mtime);
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
        return Err("There's no file in your mods folder to use.".into());
    };
    // Promoting a link would file the link itself (not a mod) into the archive and back up the
    // real archived copy out of the way. Refuse before touching anything.
    if fmeta.file_type().is_symlink() {
        return Err(
            "That's a link, not a file, so Silo can't use it. Choose Keep Silo's copy instead."
                .into(),
        );
    }
    let archive = archive_path(&layout.archive, &row.category, file_name);
    // Back up the old archived copy before replacing it.
    if archive.exists() {
        let bak = backup_path(root, file_name)?;
        move_path_as(&archive, &bak, "backup").map_err(|e| format!("backup: {e}"))?;
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
        return Err("There's no copy in your library archive to put back.".into());
    }
    let flat = root.join(file_name);
    // Back up the user's file, then free the slot.
    if flat.symlink_metadata().is_ok() {
        let bak = backup_path(root, file_name)?;
        move_path_as(&flat, &bak, "backup").map_err(|e| format!("backup: {e}"))?;
    }
    if row.active {
        make_link(&archive, &flat, &row.kind, &mut LinkPolicy::new(layout))
            .map_err(|e| format!("re-project: {e}"))?;
    }
    Ok(())
}

/// A free `<root>/backups/<file_name>.<unix_secs>.bak` for a file we're about to replace.
fn backup_path(root: &Path, file_name: &str) -> Result<PathBuf, String> {
    free_backup_path(root, file_name, ".bak").map_err(|e| e.to_string())
}

/// Suffix of the previous version an update keeps in `backups/`. Its own suffix so pruning old
/// update backups can never touch a file the user's adopt/restore kept there.
pub(crate) const UPDATE_BACKUP_SUFFIX: &str = ".update.bak";

/// A free `<root>/backups/<file_name>.<unix_secs>.update.bak` for the version an update replaced.
pub(crate) fn update_backup_path(root: &Path, file_name: &str) -> std::io::Result<PathBuf> {
    free_backup_path(root, file_name, UPDATE_BACKUP_SUFFIX)
}

/// A free `<root>/backups/<name>.<unix_secs>` folder path for a folder copy we're keeping
/// (`-2`, `-3`, … appended if that name is taken, so an earlier backup is never replaced).
fn backup_dir_path(root: &Path, name: &str) -> std::io::Result<PathBuf> {
    free_backup_path(root, name, "")
}

fn free_backup_path(root: &Path, name: &str, suffix: &str) -> std::io::Result<PathBuf> {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    backup_path_at(root, name, secs, suffix)
}

/// A free `<root>/backups/<name>.<secs>[-n]<suffix>` (creates `<root>/backups/`, not the entry).
fn backup_path_at(root: &Path, name: &str, secs: u64, suffix: &str) -> std::io::Result<PathBuf> {
    let dir = root.join("backups");
    std::fs::create_dir_all(&dir)?;
    let mut candidate = dir.join(format!("{name}.{secs}{suffix}"));
    let mut n = 2;
    while candidate.symlink_metadata().is_ok() {
        candidate = dir.join(format!("{name}.{secs}-{n}{suffix}"));
        n += 1;
    }
    Ok(candidate)
}

/// What the user is told when a changed folder copy was moved aside instead of deleted.
/// `restored`: the archived original is back in the mods folder (flatten); otherwise the mod
/// was parked and its slot is simply empty.
fn kept_copy_notice(name: &str, kept: &Path, restored: bool) -> String {
    if restored {
        format!(
            "{name} had changed since Silo copied it, so it was moved to {} instead of being deleted. Your mods folder now has Silo's original.",
            kept.display()
        )
    } else {
        format!(
            "{name} had changed since Silo copied it, so it was kept in {} instead of being deleted.",
            kept.display()
        )
    }
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
            // A stale link (its target gone) holds nothing: drop it, then restore as usual.
            if let Err(e) = remove_dangling_link(&link) {
                rep.errors
                    .push(format!("{}: remove stale link: {e}", row.tech_name));
                continue;
            }
            let occupied = link.symlink_metadata().is_ok();
            // If a file occupies the root slot but it isn't Silo's projection (the user swapped
            // their own build in), don't remove it and don't overwrite it with the archived
            // copy — leave both in place and report the anomaly for the user to resolve.
            if occupied && !is_silo_projection(&link, &src, &row.kind) {
                rep.errors.push(format!(
                    "{}: the file in your mods folder isn't the one Silo linked, so Silo left it and the copy in your library archive alone.",
                    row.tech_name
                ));
                rep.skipped += 1;
                continue;
            }
            // Normal restore: drop our projection to free the slot, then move the archived
            // original back into the root.
            let mut kept = None;
            if occupied {
                match clear_projection(&link, &src, root) {
                    Ok(k) => kept = k,
                    Err(e) => {
                        rep.errors.push(format!("{}: {e}", row.tech_name));
                        continue;
                    }
                }
            }
            let restored = move_path(&src, &link);
            if let Some(kept) = &kept {
                rep.notices
                    .push(kept_copy_notice(&row.file_name, kept, restored.is_ok()));
            }
            if let Err(e) = restored {
                rep.errors.push(format!("{}: restore: {e}", row.tech_name));
                continue;
            }
        } else if link.exists() {
            // A file sits in the root slot but there is NO archived copy behind it. That is
            // the original, not a projection (a projection always has an archive) — this is
            // exactly the state a crash between the manifest write and the move leaves. Never
            // delete it: keep the file, drop the stale manifest row.
            rep.errors.push(format!(
                "{}: there's no copy in your library archive, so Silo kept the file in your mods folder.",
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
                    "Silo kept an unrecognized file in your library archive: {}. Nothing was deleted.",
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
    // Returns true if `dir` is empty once processing finishes. At the top level, Silo's own
    // archive note (`relocate::ARCHIVE_NOTE`) counts as absent.
    fn walk(dir: &Path, top: bool, leftover: &mut Vec<PathBuf>) -> std::io::Result<bool> {
        let mut empty = true;
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            // Use symlink semantics: never follow/recurse a link.
            if entry.file_type()?.is_dir() {
                if walk(&path, false, leftover)? {
                    let _ = std::fs::remove_dir(&path);
                } else {
                    empty = false;
                }
            } else if top && entry.file_name() == crate::relocate::ARCHIVE_NOTE {
                continue;
            } else {
                leftover.push(path);
                empty = false;
            }
        }
        Ok(empty)
    }
    if walk(archive, true, &mut leftover)? && remove_root {
        let _ = std::fs::remove_file(archive.join(crate::relocate::ARCHIVE_NOTE));
        let _ = std::fs::remove_dir(archive);
    }
    Ok(leftover)
}

// ── filesystem primitives ──

/// A hidden temp sibling of `to` (`.{name}.silo-tmp`, which scans skip), where a cross-drive
/// move or a copy projection is built before it's placed at `to`.
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
        Err(_) => return Ok(()),
    }?;
    crate::fslog::changed("remove", p);
    Ok(())
}

/// Move a file/dir, preferring an instant same-volume rename. Only when the rename fails
/// because the two paths are on different drives does it fall back to a copy: copy to a
/// temporary sibling of the destination, rename that into place, and only then remove the
/// source — so a failed or partial copy never leaves a half-written file at `to`, and `from`
/// is never lost until the destination is complete. Any other rename failure (the file is in
/// use, access denied) is returned as is, with nothing copied.
pub(crate) fn move_path(from: &Path, to: &Path) -> std::io::Result<()> {
    move_path_as(from, to, "move")
}

/// [`move_path`], logged as `action` (`backup` for an update's previous version), so each move
/// is one log line that says what it was for.
pub(crate) fn move_path_as(from: &Path, to: &Path, action: &str) -> std::io::Result<()> {
    // The nearest folder above `to` that already exists: every folder made below it is a new
    // name a cross-drive move flushes before deleting its source.
    let anchor = to
        .parent()
        .and_then(|p| p.ancestors().find(|a| a.symlink_metadata().is_ok()))
        .map(Path::to_path_buf);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(from, to) {
        Ok(()) => {}
        // A rename that failed for another reason would fail the copy's final delete the same
        // way, after the copy was placed: two copies, or a half-deleted folder mod.
        Err(e) if !crosses_drives(&e) => return Err(e),
        Err(_) => copy_then_remove(from, to, anchor.as_deref())?,
    }
    crate::fslog::moved(action, from, to);
    Ok(())
}

/// The rename failed only because `from` and `to` are on different drives: Windows
/// `ERROR_NOT_SAME_DEVICE` (17), Unix `EXDEV` (18). (`ErrorKind::CrossesDevices` needs Rust
/// 1.85, above the MSRV.)
fn crosses_drives(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(if cfg!(windows) { 17 } else { 18 })
}

/// The cross-drive half of [`move_path_as`]: copy `from` to `to` through a temporary sibling,
/// then remove `from`. If `from` can't be removed, the copy at `to` (Silo's own, just made) is
/// removed again, so a failure leaves exactly the original. A folder is first renamed aside on
/// its own drive, so a delete that stops halfway never leaves a half-deleted mod under its
/// real name. `anchor` is the highest folder whose entries the move changed (see
/// [`sync_placement`]).
fn copy_then_remove(from: &Path, to: &Path, anchor: Option<&Path>) -> std::io::Result<()> {
    let tmp = tmp_sibling(to);
    remove_path(&tmp)?; // clear any stale temp from a prior aborted run
    let is_dir = from.is_dir();
    // Every copied file is flushed before the temp is placed, and the placement before `from`
    // is removed: a power cut can't leave the delete on disk without the copy.
    let copied = if is_dir {
        copy_dir_all(from, &tmp)
    } else {
        copy_file_synced(from, &tmp)
    };
    if let Err(e) = copied.and_then(|()| place_new(&tmp, to)) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    let undo = |e: std::io::Error| {
        let _ = remove_path(to);
        e
    };
    sync_placement(to, anchor).map_err(undo)?;
    if !is_dir {
        return std::fs::remove_file(from).map_err(undo);
    }
    let aside = aside_sibling(from);
    let _ = remove_path(&aside);
    std::fs::rename(from, &aside).map_err(undo)?;
    if let Err(e) = std::fs::remove_dir_all(&aside) {
        // The mod is in place at `to` and gone from its old name; only the hidden leftover
        // remains (scans skip it, and the next archive move clears it).
        log::warn!("moved, but couldn't remove {}: {e}", aside.display());
    }
    Ok(())
}

/// `.{name}.silo-move-old` beside `p`: where a folder waits to be deleted after a cross-drive
/// copy (a Silo temp name, so scans skip it).
fn aside_sibling(p: &Path) -> PathBuf {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "x".into());
    p.with_file_name(format!(".{name}.silo-move-old"))
}

/// Project `src` into the flat root at `link`. Files → hardlink (no privilege, same volume, no
/// disk duplication); if that fails, a symlink only when the archive is on **another drive**
/// and symlinks work there (`policy`, decided lazily); otherwise a copy. A same-volume folder
/// that rejects hardlinks (cloud-synced) keeps copies. Dirs → OS symlink (on Windows, an NTFS
/// junction when symlinks need Developer Mode), falling back to copy;
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
        link_dir_or_copy(src, link, link_dir_native)
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
    cross_volume: Option<bool>,
}

impl<'a> LinkPolicy<'a> {
    pub(crate) fn new(layout: &'a Layout) -> Self {
        LinkPolicy {
            layout,
            cached: None,
            cross_volume: None,
        }
    }

    /// A policy that decided the archive is on another drive with no symlinks (copies only).
    #[cfg(test)]
    pub(crate) fn copy_only_for_tests(layout: &'a Layout) -> Self {
        LinkPolicy {
            layout,
            cached: Some(false),
            cross_volume: Some(true),
        }
    }

    /// The archive is on another volume than the mods folder (an unknown mount is probed, see
    /// `same_volume_for_links`). Asked at most once.
    fn cross_volume(&mut self) -> bool {
        let layout = self.layout;
        *self
            .cross_volume
            .get_or_insert_with(|| !same_volume_for_links(layout))
    }

    /// Symlinks only for an archive on a different volume, outside a Mac App Store sandbox,
    /// where a probe link actually works.
    pub(crate) fn allow_symlink(&mut self) -> bool {
        if let Some(c) = self.cached {
            return c;
        }
        let allow = self.cross_volume() && symlinks_usable(self.layout);
        self.cached = Some(allow);
        allow
    }

    /// A zip link into this archive can only be a full copy: it's on another drive (no
    /// hardlink) and symlinks aren't allowed.
    pub(crate) fn can_only_copy(&mut self) -> bool {
        self.cross_volume() && !self.allow_symlink()
    }

    /// The mods folder is inside a Mac App Store sandbox, so the game can't follow any link
    /// out of it — folder mods must be projected as marked copies.
    pub(crate) fn mods_in_container(&self) -> bool {
        in_mac_app_container(&self.layout.mods)
    }
}

/// Symlinks from the mods folder into the archive can be created and the game will follow them
/// (what `LinkPolicy` allows across drives, and what the move preflight reports).
pub(crate) fn symlinks_usable(layout: &Layout) -> bool {
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
        Ok(()) => {
            crate::fslog::moved("link", src, link);
            return Ok(());
        }
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
            Ok(()) => {
                crate::fslog::moved("link", src, link);
                return Ok(());
            }
            Err(e) if e.kind() == AlreadyExists => return Err(e),
            Err(_) => {}
        }
    }
    copy_new(src, link)
}

/// Copy `src` to a **new** file `dst`: refuses (AlreadyExists) rather than replacing an
/// existing file. The copy is built and flushed under a Silo temp sibling
/// (`.{name}.silo-tmp`, which scans skip) and only then placed at `dst`, so a copy that stops
/// part-way (a full disk, an archive file that won't read, Silo closed mid-copy) never leaves a
/// truncated mod under its real name. A failed copy removes its temp.
fn copy_new(src: &Path, dst: &Path) -> std::io::Result<()> {
    copy_new_with(src, dst, stream_new_synced)
}

/// [`copy_new`] with the write into the temp injected, so a copy cut off part-way is testable.
fn copy_new_with(
    src: &Path,
    dst: &Path,
    write: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let tmp = tmp_sibling(dst);
    remove_path(&tmp)?; // a stale temp from a copy that was cut off
    if let Err(e) = write(src, &tmp).and_then(|()| place_new(&tmp, dst)) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    crate::fslog::moved("copy", src, dst);
    Ok(())
}

/// Stream `src`'s bytes into a new file `dst` and flush it. Only the bytes: a projection never
/// takes the archived file's read-only attribute, which would stop Silo removing it later.
/// Opens `src` first, so a missing source creates nothing.
fn stream_new_synced(src: &Path, dst: &Path) -> std::io::Result<()> {
    let mut from = std::fs::File::open(src)?;
    let mut to = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dst)?;
    std::io::copy(&mut from, &mut to)?;
    to.sync_all()
}

/// Copy one file to a **new** name `dst` (never replacing a file there) and flush it to disk.
/// `std::fs::copy` keeps what it always kept (permissions; on Windows the modified time,
/// attributes and alternate streams). A missing source creates nothing; a copy that fails
/// part-way can leave a partial `dst`, which the callers (always writing to a Silo temp) remove.
fn copy_file_synced(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::metadata(src)?;
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    std::fs::copy(src, dst)?;
    flush_file(dst)
}

/// Flush a file's data and metadata to disk. Unix flushes through a read handle. Windows needs a
/// write handle, so a read-only file (`std::fs::copy` keeps the attribute) is made writable
/// for the moment and set back.
fn flush_file(p: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::File::open(p)?.sync_all()
    }
    #[cfg(windows)]
    {
        let flush = || std::fs::OpenOptions::new().write(true).open(p)?.sync_all();
        let perms = std::fs::metadata(p)?.permissions();
        if !perms.readonly() {
            return flush();
        }
        let mut writable = perms.clone();
        #[allow(clippy::permissions_set_readonly_false)] // Windows: only clears the attribute
        writable.set_readonly(false);
        std::fs::set_permissions(p, writable)?;
        let flushed = flush();
        let restored = std::fs::set_permissions(p, perms);
        flushed.and(restored)
    }
}

/// Make a cross-drive move's placement at `to` durable before its source is deleted. The
/// placed file is flushed again (on Windows that commits NTFS's metadata for it, the new name
/// included), then on Unix every folder from the one holding `to` up to `anchor`, the nearest
/// one that existed before the move (`move_path_as` may have created the folders below it,
/// e.g. a new archive category). Without an anchor, only the folder holding `to`.
fn sync_placement(to: &Path, anchor: Option<&Path>) -> std::io::Result<()> {
    if to.symlink_metadata()?.is_dir() {
        sync_dir(to)?;
    } else {
        flush_file(to)?;
    }
    for dir in placement_dirs(to, anchor) {
        sync_dir(dir)?;
    }
    Ok(())
}

/// The folders whose entries placing `to` changed: the one holding it, up to and including
/// `anchor` (just the one holding it without an anchor).
fn placement_dirs<'a>(to: &'a Path, anchor: Option<&Path>) -> Vec<&'a Path> {
    let mut dirs = Vec::new();
    for dir in to.parent().into_iter().flat_map(Path::ancestors) {
        dirs.push(dir);
        if anchor.map_or(true, |a| dir == a) {
            break;
        }
    }
    dirs
}

/// Put the finished temp `tmp` at `dst` without replacing anything there. A file is hardlinked
/// (which fails if `dst` exists), then the temp dropped; where the folder can't hardlink (a
/// cloud-synced Documents folder), and for a folder, `dst` is checked to be free right before
/// the rename. Called under the library lock, which keeps Silo's own writes out; another
/// program creating `dst` in the moment between the check and the rename is the only gap.
fn place_new(tmp: &Path, dst: &Path) -> std::io::Result<()> {
    use std::io::ErrorKind;
    let taken = || std::io::Error::from(ErrorKind::AlreadyExists);
    if !tmp.symlink_metadata()?.is_dir() {
        match std::fs::hard_link(tmp, dst) {
            Ok(()) => {
                // A leftover second name holds no extra data; the next activation sweeps it.
                let _ = std::fs::remove_file(tmp);
                return Ok(());
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(taken()),
            Err(_) => {} // no hardlinks here: check + rename below
        }
    }
    let mut tries = 0;
    loop {
        if dst.symlink_metadata().is_ok() {
            return Err(taken());
        }
        match std::fs::rename(tmp, dst) {
            // Windows refuses to rename a folder while a file in it is open, and an antivirus
            // or cloud-sync client often opens a just-written file for a moment.
            Err(e) if cfg!(windows) && e.kind() == ErrorKind::PermissionDenied && tries < 10 => {
                tries += 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            other => return other,
        }
    }
}

/// Flush a folder's entries (names just created or renamed in it) to disk. On Unix a file's
/// own flush doesn't cover its name; a filesystem that can't flush a folder (`EINVAL`,
/// unsupported) is let through, any other failure is returned. On Windows it's best effort:
/// the folder is flushed through a backup-semantics handle where NTFS allows it.
fn sync_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::ErrorKind::{InvalidInput, Unsupported};
        match std::fs::File::open(dir)?.sync_all() {
            Err(e) if matches!(e.kind(), InvalidInput | Unsupported) => Ok(()),
            other => other,
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        if let Ok(d) = std::fs::OpenOptions::new()
            .write(true)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(dir)
        {
            let _ = d.sync_all();
        }
        Ok(())
    }
}

/// Remove the Silo temp a copy into `link` left when it was cut off (Silo closed or crashed
/// mid-copy). Scans already skip it; this frees the space. The name is Silo's own
/// (`.{name}.silo-tmp`), so nothing else puts anything there.
fn sweep_copy_temp(link: &Path) {
    let tmp = tmp_sibling(link);
    if tmp.symlink_metadata().is_err() {
        return;
    }
    if let Err(e) = remove_path(&tmp) {
        log::warn!("couldn't remove the leftover {}: {e}", tmp.display());
    }
}

/// Name prefixes of the link probes below (`scan::is_silo_temp` skips them; the fixed-name
/// cloud check, `supports_hardlinks`, shares the hardlink one).
pub(crate) const HARDLINK_PROBE: &str = ".silo-hardlink-probe";
pub(crate) const SYMLINK_PROBE: &str = ".silo-symlink-probe";

/// Distinguishes concurrent link probes (within this process; the pid covers others).
static PROBE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many names a probe tries before giving up on a folder full of stale probes.
const PROBE_TRIES: usize = 8;

/// Probe names `<prefix>-<pid>-<seq>`, a fresh sequence number each.
fn probe_names(prefix: &str) -> impl Iterator<Item = String> + '_ {
    std::iter::repeat_with(move || {
        let seq = PROBE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        format!("{prefix}-{}-{seq}", std::process::id())
    })
    .take(PROBE_TRIES)
}

/// A new, empty probe file in `dir` under the first of `names` that's free (created
/// exclusively; a name a stale probe holds moves on to the next). `None` when none can be made
/// — the folder is missing or refuses files — which says nothing about links.
fn create_probe_named(dir: &Path, names: impl Iterator<Item = String>) -> Option<PathBuf> {
    for name in names {
        let p = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&p)
        {
            Ok(_) => return Some(p),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return None,
        }
    }
    None
}

/// A probe file in `archive` linked into `mods` by `link` (as `<probe name>.link`), both
/// removed again — never a link that was already there. `Some(worked)`, or `None` when no
/// probe could be made (missing folders, a folder that refuses files): inconclusive. Never
/// creates a folder.
fn probe_link(
    mods: &Path,
    archive: &Path,
    prefix: &str,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Option<bool> {
    if !archive.is_dir() || !mods.is_dir() {
        return None;
    }
    for _ in 0..PROBE_TRIES {
        let target = create_probe_named(archive, probe_names(prefix))?;
        let name = target.file_name()?.to_string_lossy().into_owned();
        let at = mods.join(format!("{name}.link"));
        let r = link(&target, &at);
        let taken = matches!(&r, Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists);
        if !taken {
            let _ = std::fs::remove_file(&at);
        }
        let _ = std::fs::remove_file(&target);
        match r {
            Ok(()) => return Some(true),
            // A stale probe link holds that name: try the next one.
            Err(_) if taken => continue,
            Err(_) => return Some(false),
        }
    }
    None
}

/// Whether a symlink from the mods folder into the archive works here (always on Linux/macOS
/// for normal drives; on Windows only with Developer Mode). Cheap one-shot probe with unique
/// names, cleaned up afterwards. Never creates a folder; no probe possible → false (symlinks
/// are then simply not used).
pub fn supports_symlinks(layout: &Layout) -> bool {
    probe_link(&layout.mods, &layout.archive, SYMLINK_PROBE, symlink_file) == Some(true)
}

/// Remove link probes (`HARDLINK_PROBE*`, `SYMLINK_PROBE*`) a crash left in `dir`, when older
/// than `min_age` (a probe running right now is younger). Only those names, only files or
/// links — never a folder or anything else. The caller holds the library lock.
pub(crate) fn sweep_link_probes(dir: &Path, min_age: std::time::Duration) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().to_lowercase();
        if !(name.starts_with(HARDLINK_PROBE) || name.starts_with(SYMLINK_PROBE)) {
            continue;
        }
        let Ok(meta) = e.path().symlink_metadata() else {
            continue;
        };
        let old = meta
            .modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|age| age >= min_age);
        if old && !meta.is_dir() && std::fs::remove_file(e.path()).is_ok() {
            crate::fslog::changed("remove-stale-probe", &e.path());
        }
    }
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

/// Pure: both mounts (`mount_of`) are known and the same. An unknown mount — a network share,
/// anything the disk list doesn't show — is never the same drive. For the move's free-space
/// check, where guessing "same" could let a move run out of room.
pub(crate) fn known_same_mount(a: &Option<(PathBuf, u64)>, b: &Option<(PathBuf, u64)>) -> bool {
    matches!((a, b), (Some((ma, _)), Some((mb, _))) if ma == mb)
}

/// What the disk list says about `a` and `b` (one enumeration for both): `Some(same)` when it
/// knows both mounts, `None` when it doesn't know one of them (a network share, …).
pub(crate) fn volume_relation(a: &Path, b: &Path) -> Option<bool> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    match (mount_in(&disks, a), mount_in(&disks, b)) {
        (Some((ma, _)), Some((mb, _))) => Some(ma == mb),
        _ => None,
    }
}

/// Pure: "same volume" for linking — the disk list's answer when it has one, else what
/// `probe` finds (asked only then). A probe that couldn't run (`None`) is inconclusive and
/// counts as the same volume: a hardlink is then tried first and falls back to a copy safely.
pub(crate) fn same_volume_or_probe(
    relation: Option<bool>,
    probe: impl FnOnce() -> Option<bool>,
) -> bool {
    relation.or_else(probe).unwrap_or(true)
}

/// Whether links from `layout.mods` into `layout.archive` should treat them as one volume:
/// known mounts decide, an unknown one is probed (`hardlinks_between`). Every link decision
/// (`LinkPolicy`, Optimize links' `relink_mode`) goes through this, so they always agree.
pub(crate) fn same_volume_for_links(layout: &Layout) -> bool {
    same_volume_or_probe(volume_relation(&layout.mods, &layout.archive), || {
        hardlinks_between(&layout.mods, &layout.archive)
    })
}

/// Whether a hardlink from the mods folder into the archive can actually be made (`probe_link`
/// with `hard_link`): `Some(true)` / `Some(false)`, or `None` when no probe could be made.
pub(crate) fn hardlinks_between(mods: &Path, archive: &Path) -> Option<bool> {
    probe_hardlink(mods, archive, |s, l| std::fs::hard_link(s, l))
}

/// `hardlinks_between` with the link injected (tests).
fn probe_hardlink(
    mods: &Path,
    archive: &Path,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Option<bool> {
    probe_link(mods, archive, HARDLINK_PROBE, link)
}

/// Dir projection: `symlink` (a symlink or junction, see [`link_dir_native`]), or a marked copy
/// when the filesystem refuses links (exFAT/FAT drives, a network archive). Never falls back
/// when `link` already exists — copying into a folder that's already there would merge into it
/// and stamp it as ours.
fn link_dir_or_copy(
    src: &Path,
    link: &Path,
    symlink: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    match symlink(src, link) {
        Err(e) if e.kind() != std::io::ErrorKind::AlreadyExists => copy_dir_projection(src, link),
        Ok(()) => {
            crate::fslog::moved("link", src, link);
            Ok(())
        }
        other => other,
    }
}

/// Create a directory link at `link` pointing at `src`. Windows tries a symlink first
/// (Developer Mode), then an NTFS junction, which needs no privilege. Unix uses a symlink.
/// Errors bubble up so the caller can fall back to a marked copy.
#[cfg(windows)]
pub(crate) fn link_dir_native(src: &Path, link: &Path) -> std::io::Result<()> {
    match std::os::windows::fs::symlink_dir(src, link) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Err(e),
        Err(_) => junction_to(src, link),
    }
}

/// An NTFS junction at `link` to `src`. Never replaces anything already at `link`. A junction
/// can only point at a local volume, so a network target (a UNC path, or a mapped drive, which
/// canonicalizes to one) is refused with `Unsupported` and the caller falls back to a copy. If
/// the junction can't be made for another reason (not NTFS), the empty folder the crate created
/// for it is removed again (`remove_dir` only ever removes an empty folder or a link).
#[cfg(windows)]
pub(crate) fn junction_to(src: &Path, link: &Path) -> std::io::Result<()> {
    if link.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    // Junction targets must be absolute; the crate strips the `\\?\` verbatim prefix itself.
    let target = std::fs::canonicalize(src)?;
    if is_network_path(&target.to_string_lossy()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "a junction can't point at a network folder",
        ));
    }
    junction::create(&target, link).inspect_err(|e| {
        if e.kind() != std::io::ErrorKind::AlreadyExists {
            let _ = std::fs::remove_dir(link);
        }
    })
}

/// A Windows path on a network share: `\\server\share\…`, or the verbatim / device forms
/// `\\?\UNC\…` and `\\.\UNC\…`. Local verbatim paths (`\\?\C:\…`) are not.
pub(crate) fn is_network_path(p: &str) -> bool {
    let rest = p.strip_prefix(r"\\?\").or_else(|| p.strip_prefix(r"\\.\"));
    match rest {
        Some(rest) => rest
            .get(..4)
            .is_some_and(|s| s.eq_ignore_ascii_case(r"UNC\")),
        None => p.starts_with(r"\\"),
    }
}

#[cfg(unix)]
pub(crate) fn link_dir_native(src: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(src, link)
}

/// The ownership marker of a folder copy (v2): every file Silo wrote, so a later park or
/// flatten can tell an untouched copy (safe to delete) from one the user edited.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CopyListing {
    v: u32,
    files: Vec<ListedFile>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
struct ListedFile {
    /// Relative to the copy, '/'-separated.
    path: String,
    size: u64,
    mtime_ms: u64,
}

/// Every regular file under `dir` except the marker, sorted by path. Links are not followed.
fn listing_of(dir: &Path) -> std::io::Result<Vec<ListedFile>> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(std::io::Error::other)?;
        if !entry.file_type().is_file() {
            continue;
        }
        let rel = entry
            .path()
            .strip_prefix(dir)
            .map_err(std::io::Error::other)?;
        if rel == Path::new(SILO_MARKER) {
            continue;
        }
        // Ask the file itself: Windows' directory-enumeration data that walkdir caches can be
        // stale for a file that's still open (its size/mtime aren't flushed to the entry yet).
        let meta = std::fs::metadata(entry.path())?;
        let mtime_ms = meta
            .modified()?
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        out.push(ListedFile {
            path: rel.to_string_lossy().replace('\\', "/"),
            size: meta.len(),
            mtime_ms,
        });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Copy a directory as a projection and drop the ownership marker, so a later flatten /
/// deactivate can prove Silo created it (a copied dir has no link identity to compare). The
/// marker lists the copy's own files as written, so later edits can be detected. The copy is
/// built and flushed under a Silo temp sibling (`.{name}.silo-tmp`), marker included, and only
/// then renamed to `dst`: a copy that stops part-way never leaves a partial mod under its real
/// name, and a failed copy removes its temp. Never merges into an existing `dst`.
fn copy_dir_projection(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let tmp = tmp_sibling(dst);
    remove_path(&tmp)?; // a stale temp from a copy that was cut off
    if let Err(e) = build_dir_copy(src, &tmp).and_then(|()| place_new(&tmp, dst)) {
        let _ = remove_path(&tmp);
        return Err(e);
    }
    crate::fslog::moved("copy", src, dst);
    Ok(())
}

/// The body of [`copy_dir_projection`]: copy `src` into `tmp` and write the marker, all flushed.
fn build_dir_copy(src: &Path, tmp: &Path) -> std::io::Result<()> {
    copy_dir_all(src, tmp)?;
    let listing = CopyListing {
        v: 2,
        files: listing_of(tmp)?,
    };
    let json = serde_json::to_vec(&listing).map_err(std::io::Error::other)?;
    // Replaces a marker that came with `src` (a copy that was filed back into the archive).
    let mut marker = std::fs::File::create(tmp.join(SILO_MARKER))?;
    std::io::Write::write_all(&mut marker, &json)?;
    marker.sync_all()?;
    sync_dir(tmp)
}

/// The v2 listing in `dir`'s marker; `None` for a legacy (`silo\n`) or unreadable marker.
fn marker_listing(dir: &Path) -> std::io::Result<Option<Vec<ListedFile>>> {
    let raw = std::fs::read(dir.join(SILO_MARKER))?;
    Ok(match serde_json::from_slice::<CopyListing>(&raw) {
        Ok(l) if l.v == 2 => Some(l.files),
        _ => None,
    })
}

/// True only when the marked folder copy at `dir` is provably what Silo wrote. With a v2
/// listing: the same files with the same sizes and modification times (any added, removed,
/// resized or re-timed file counts as changed). Limit: an edit that keeps a file's size and
/// lands within the same timestamp tick as the copy (same millisecond, or FAT's 2 s) can't be
/// seen; in practice an edit comes much later. A legacy marker has no listing, so the copy is
/// compared with the archived `src` by relative path, size and SHA-256.
pub(crate) fn copy_unchanged(dir: &Path, src: &Path) -> std::io::Result<bool> {
    match marker_listing(dir)? {
        Some(files) => Ok(listing_of(dir)? == files),
        None => same_tree_contents(dir, src),
    }
}

/// Same relative paths, sizes and SHA-256 contents (the marker is ignored).
fn same_tree_contents(copy: &Path, src: &Path) -> std::io::Result<bool> {
    let a = listing_of(copy)?;
    let b = listing_of(src)?;
    if a.len() != b.len()
        || a.iter()
            .zip(&b)
            .any(|(x, y)| x.path != y.path || x.size != y.size)
    {
        return Ok(false);
    }
    let sha = |p: PathBuf| crate::provenance::sha256_file(&p).map_err(std::io::Error::other);
    for f in &a {
        if sha(copy.join(&f.path))? != sha(src.join(&f.path))? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Remove a marked folder copy only if unchanged; otherwise move it to
/// `<mods_root>/backups/<name>.<secs>/` and return that path so the caller can tell the user.
/// The kept copy loses Silo's marker: it is the user's folder now.
pub(crate) fn retire_folder_copy(
    link: &Path,
    src: &Path,
    mods_root: &Path,
) -> std::io::Result<Option<PathBuf>> {
    retire_folder_copy_with(link, src, mods_root, |a, b| std::fs::rename(a, b))
}

/// [`retire_folder_copy`] with the move injected. `backups/` sits beside the copy in the mods
/// folder, so the move is always a same-volume rename: never a copy-then-delete, which could
/// half-copy a folder with a file in use and then fail to delete it. If the rename fails, the
/// copy is left exactly as it was (marker included) and the error says why.
fn retire_folder_copy_with(
    link: &Path,
    src: &Path,
    mods_root: &Path,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<Option<PathBuf>> {
    if copy_unchanged(link, src).unwrap_or(false) {
        std::fs::remove_dir_all(link)?;
        crate::fslog::changed("unlink", link);
        return Ok(None);
    }
    let name = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "folder".into());
    let backups = mods_root.join("backups");
    let created = backups.symlink_metadata().is_err();
    let dest = backup_dir_path(mods_root, &name)?;
    if let Err(e) = rename(link, &dest) {
        // Don't leave behind an empty backups/ this call made (remove_dir refuses a non-empty one).
        if created {
            let _ = std::fs::remove_dir(&backups);
        }
        return Err(std::io::Error::other(backup_move_error(&name, &e)));
    }
    crate::fslog::moved("keep-edited-copy", link, &dest);
    let _ = std::fs::remove_file(dest.join(SILO_MARKER));
    Ok(Some(dest))
}

/// What the user is told when moving a changed copy to backups failed. Access denied or a
/// Windows sharing/lock violation (os error 32 / 33) almost always means a program has a file
/// in it open; anything else is reported as is.
fn backup_move_error(name: &str, e: &std::io::Error) -> String {
    let in_use = e.kind() == std::io::ErrorKind::PermissionDenied
        || (cfg!(windows) && matches!(e.raw_os_error(), Some(32) | Some(33)));
    if in_use {
        format!(
            "Couldn't move {name} to backups because a file in it is in use. Close any program using it (the game, an editor) and try again."
        )
    } else {
        format!("Couldn't move {name} to backups ({e}), so Silo left it in place.")
    }
}

/// Free a mods-folder slot holding Silo's projection of `src`. A marked folder copy goes
/// through [`retire_folder_copy`] (an edited one is kept in backups, its path returned);
/// anything else through [`remove_link`].
fn clear_projection(link: &Path, src: &Path, mods_root: &Path) -> std::io::Result<Option<PathBuf>> {
    let meta = link.symlink_metadata()?;
    if meta.is_dir() && !meta.file_type().is_symlink() && link.join(SILO_MARKER).is_file() {
        return retire_folder_copy(link, src, mods_root);
    }
    remove_link(link).map(|()| None)
}

/// True only if `link` in the flat root is provably the projection Silo created for archived
/// `src` — so removing it can't destroy a file the user put there themselves. Symlink /
/// junction resolves to `src`; a zip hardlink shares `src`'s file id; a cross-volume copy is
/// byte-identical to `src`; a copied dir carries our marker. Anything else is NOT ours.
pub(crate) fn is_silo_projection(link: &Path, src: &Path, kind: &str) -> bool {
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

/// Remove a symlink or junction at `link` whose target is gone (e.g. into an archive on a drive
/// that's no longer there, after a re-point). At an organized mod's mods-folder name that's
/// Silo's stale projection, and a link holds no data. Returns whether it removed one; never
/// touches a regular file, a folder, a working link, or a missing path.
pub(crate) fn remove_dangling_link(link: &Path) -> std::io::Result<bool> {
    let Ok(meta) = link.symlink_metadata() else {
        return Ok(false);
    };
    if !meta.file_type().is_symlink() || link.metadata().is_ok() {
        return Ok(false);
    }
    // A dangling dir link can't report itself as a dir on Windows: `remove_file` refuses it,
    // and `remove_dir` then removes the link itself (never a folder's contents).
    std::fs::remove_file(link).or_else(|_| std::fs::remove_dir(link))?;
    crate::fslog::changed("unlink", link);
    Ok(true)
}

/// Remove a projected entry (hardlink, symlink, junction, or copy) without
/// touching the archived original. A real folder is removed only when it is Silo's v2-marked
/// copy and still untouched; any other folder (the user's own, an edited or legacy copy) is
/// refused. Park and flatten go through [`clear_projection`], which keeps edited copies.
pub(crate) fn remove_link(link: &Path) -> std::io::Result<()> {
    let meta = link.symlink_metadata()?;
    let ft = meta.file_type();
    let removed = if ft.is_symlink() {
        // A dir symlink/junction must be removed with remove_dir on Windows. On Unix every
        // symlink — even one to a folder — is unlinked as a file: rmdir on it fails (ENOTDIR).
        if cfg!(windows) && link.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            std::fs::remove_dir(link)
        } else {
            std::fs::remove_file(link)
        }
    } else if ft.is_dir() {
        let untouched = link.join(SILO_MARKER).is_file()
            && match marker_listing(link)? {
                Some(files) => listing_of(link)? == files,
                None => false,
            };
        if !untouched {
            let name = link
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| link.display().to_string());
            return Err(std::io::Error::other(format!(
                "{name} isn't a copy Silo made, or it has changed since, so Silo left it alone."
            )));
        }
        std::fs::remove_dir_all(link)
    } else {
        std::fs::remove_file(link)
    };
    removed?;
    crate::fslog::changed("unlink", link);
    Ok(())
}

/// Copy the folder `src` to `dst`, every file flushed to disk (see [`copy_file_synced`]).
fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            copy_file_synced(&entry.path(), &to)?;
        }
    }
    sync_dir(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A rename refused for any reason but "different drive" moves nothing and copies nothing:
    /// the old fallback copied, then failed the delete, leaving two copies.
    #[cfg(windows)]
    #[test]
    fn a_file_in_use_is_not_copied_when_its_rename_fails() {
        use std::os::windows::fs::OpenOptionsExt;
        let base = std::env::temp_dir().join(format!("silo_move_inuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let from = base.join("FS25_A.zip");
        std::fs::write(&from, b"PK").unwrap();
        let to = base.join("archive/Misc/FS25_A.zip");
        // Held open without delete sharing, like the game holding a mod zip.
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ
            .open(&from)
            .unwrap();
        let r = move_path(&from, &to);
        drop(held);
        assert!(r.is_err());
        assert!(from.exists());
        assert!(!to.exists(), "nothing copied");
        assert!(!tmp_sibling(&to).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(unix)]
    #[test]
    fn a_refused_rename_is_not_copied() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("silo_move_ro_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ro = base.join("ro");
        std::fs::create_dir_all(&ro).unwrap();
        let from = ro.join("FS25_A.zip");
        std::fs::write(&from, b"PK").unwrap();
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
        let to = base.join("archive/Misc/FS25_A.zip");
        let r = move_path(&from, &to);
        std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        if r.is_ok() {
            // Running as root: permissions don't apply, so there's nothing to test.
            let _ = std::fs::remove_dir_all(&base);
            return;
        }
        assert!(from.exists());
        assert!(!to.exists(), "nothing copied");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn only_a_different_drive_counts_as_crossing_drives() {
        let (cross, in_use) = if cfg!(windows) { (17, 32) } else { (18, 13) };
        assert!(crosses_drives(&std::io::Error::from_raw_os_error(cross)));
        assert!(!crosses_drives(&std::io::Error::from_raw_os_error(in_use)));
        assert!(!crosses_drives(&std::io::Error::other("x")));
    }

    /// The cross-drive copy: a source that can't be removed takes the copy back out, so the
    /// failure leaves exactly the original (file and folder).
    #[cfg(windows)]
    #[test]
    fn a_cross_drive_copy_whose_source_cant_be_removed_is_undone() {
        use std::os::windows::fs::OpenOptionsExt;
        let base = std::env::temp_dir().join(format!("silo_move_undo_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("FS25_Dir/data")).unwrap();
        let file = base.join("FS25_A.zip");
        std::fs::write(&file, b"PK").unwrap();
        let inner = base.join("FS25_Dir/data/x.i3d");
        std::fs::write(base.join("FS25_Dir/modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(&inner, b"x").unwrap();
        let open = |p: &Path| {
            std::fs::OpenOptions::new()
                .read(true)
                .share_mode(1) // FILE_SHARE_READ: copyable, not deletable
                .open(p)
                .unwrap()
        };

        let to = base.join("other/FS25_A.zip");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        let held = open(&file);
        assert!(copy_then_remove(&file, &to, to.parent()).is_err());
        drop(held);
        assert!(file.exists() && !to.exists());

        let dir = base.join("FS25_Dir");
        let to = base.join("other/FS25_Dir");
        let held = open(&inner);
        assert!(copy_then_remove(&dir, &to, to.parent()).is_err());
        drop(held);
        assert!(inner.exists(), "the folder is whole");
        assert!(dir.join("modDesc.xml").exists());
        assert!(!to.exists() && !aside_sibling(&dir).exists());

        // Nothing held: the folder moves and no leftover remains.
        copy_then_remove(&dir, &to, to.parent()).unwrap();
        assert!(to.join("data/x.i3d").exists() && !dir.exists());
        assert!(!aside_sibling(&dir).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn cleaning_an_empty_archive_removes_silos_own_note() {
        let base = std::env::temp_dir().join(format!("silo_clean_note_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let archive = base.join("hdd");
        std::fs::create_dir_all(archive.join("Vehicles")).unwrap();
        std::fs::write(archive.join(crate::relocate::ARCHIVE_NOTE), b"{}").unwrap();
        let leftover = cleanup_empty_archive(&archive, true).unwrap();
        assert!(
            leftover.is_empty(),
            "the note is not reported as a leftover: {leftover:?}"
        );
        assert!(!archive.exists());
        // Kept when the root is kept (a user-chosen folder stays as it is).
        std::fs::create_dir_all(&archive).unwrap();
        std::fs::write(archive.join(crate::relocate::ARCHIVE_NOTE), b"{}").unwrap();
        assert!(cleanup_empty_archive(&archive, false).unwrap().is_empty());
        assert!(archive.join(crate::relocate::ARCHIVE_NOTE).exists());
        // An archive that still holds something keeps its note too.
        let orphan = archive.join("Tools").join("orphan.zip");
        std::fs::create_dir_all(orphan.parent().unwrap()).unwrap();
        std::fs::write(&orphan, b"PK").unwrap();
        assert_eq!(cleanup_empty_archive(&archive, true).unwrap(), vec![orphan]);
        assert!(archive.join(crate::relocate::ARCHIVE_NOTE).exists());
        let _ = std::fs::remove_dir_all(&base);
    }

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
            "B's file is not Silo's. Never removed"
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

    /// #322: filing a mod carries its scan-cache row to the archived path, so the scan after
    /// filing reuses it instead of parsing every filed mod again.
    #[test]
    fn filing_keeps_the_scan_cache_so_the_next_scan_reuses_it() {
        let base = std::env::temp_dir().join(format!("silo_filecache_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let arch = mods.join("archive");
        std::fs::create_dir_all(&mods).unwrap();
        let mut conn = db::open(&base.join("silo.db")).unwrap();
        let layout = Layout {
            mods: mods.clone(),
            archive: arch.clone(),
        };
        std::fs::write(mods.join("FS25_Foo.zip"), b"PK foo").unwrap();

        // First scan: parsed fresh, then cached the way scan_mods caches it.
        let scan = |conn: &Connection| {
            crate::scan::scan_layouts(
                vec![(mods.clone(), vec![arch.clone()])],
                &db::load_cache(conn),
                |_, _| {},
            )
        };
        let first = scan(&conn);
        assert_eq!(first.fresh_paths.len(), 1, "parsed once");
        let rows: Vec<(String, u64, u64, String)> = first
            .result
            .mods
            .iter()
            .map(|m| {
                (
                    m.path.clone(),
                    m.mtime_ms,
                    m.size,
                    serde_json::to_string(m).unwrap(),
                )
            })
            .collect();
        db::upsert_many(&mut conn, &rows).unwrap();

        let input = ModInput {
            tech_name: "FS25_Foo".into(),
            file_name: "FS25_Foo.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
        };
        let rep = apply_organize(&conn, &layout, &mods, std::slice::from_ref(&input));
        assert_eq!(rep.changed, 1);
        let filed = arch.join("Vehicles").join("FS25_Foo.zip");
        assert!(filed.exists());

        // The scan after filing finds the mod at its new path without parsing it again.
        let second = scan(&conn);
        assert!(
            second.fresh_paths.is_empty(),
            "reused the cache, parsed: {:?}",
            second.fresh_paths
        );
        let m = &second.result.mods[0];
        assert_eq!(m.path, filed.to_string_lossy());
        assert!(m.organized);
        let cache = db::load_cache(&conn);
        assert!(cache.contains_key(filed.to_string_lossy().as_ref()));
        assert!(!cache.contains_key(mods.join("FS25_Foo.zip").to_string_lossy().as_ref()));
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

    /// Make `file` unreadable for as long as the returned guard lives: on Windows a handle that
    /// shares nothing (like a locked file), on Unix a dangling symlink in its place (like a
    /// cloud placeholder that won't download).
    fn unreadable(file: &Path) -> impl Drop {
        struct Guard(Option<std::fs::File>, PathBuf);
        impl Drop for Guard {
            fn drop(&mut self) {
                self.0.take();
                if cfg!(unix) {
                    let _ = std::fs::remove_file(&self.1);
                    let _ = std::fs::write(&self.1, b"ok");
                }
            }
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            std::fs::write(file, b"ok").unwrap();
            let held = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(file)
                .unwrap();
            Guard(Some(held), file.to_path_buf())
        }
        #[cfg(unix)]
        {
            let _ = std::fs::remove_file(file);
            std::os::unix::fs::symlink(file.with_file_name("gone"), file).unwrap();
            Guard(None, file.to_path_buf())
        }
    }

    #[test]
    fn a_folder_copy_that_stops_part_way_leaves_nothing_under_the_mods_name() {
        let base = temp_dir_for("dircopy_partial");
        let src = base.join("archived").join("FS25_Dir");
        std::fs::create_dir_all(src.join("data")).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(src.join("data").join("a.i3d"), b"i3d").unwrap();
        let bad = src.join("data").join("b.dds");
        let dst = base.join("mods").join("FS25_Dir");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();

        let guard = unreadable(&bad);
        assert!(copy_dir_projection(&src, &dst).is_err());
        assert!(
            dst.symlink_metadata().is_err(),
            "no partial mod at its name"
        );
        assert!(
            tmp_sibling(&dst).symlink_metadata().is_err(),
            "the temp is removed"
        );
        drop(guard);

        // Readable again: the whole copy lands, marked, with no temp left behind.
        copy_dir_projection(&src, &dst).unwrap();
        assert_eq!(
            std::fs::read(dst.join("data").join("b.dds")).unwrap(),
            b"ok"
        );
        assert!(is_silo_projection(&dst, &src, "dir"));
        assert!(copy_unchanged(&dst, &src).unwrap());
        assert!(tmp_sibling(&dst).symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_zip_copy_is_placed_whole_and_never_over_an_existing_entry() {
        let base = temp_dir_for("zipcopy_place");
        let src = base.join("FS25_A.zip");
        std::fs::write(&src, b"PK\x03\x04 whole").unwrap();
        let dst = base.join("mods").join("FS25_A.zip");
        std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
        // A temp a cut-off copy left behind is replaced, never placed.
        std::fs::write(tmp_sibling(&dst), b"PK half").unwrap();

        copy_new(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"PK\x03\x04 whole");
        assert!(tmp_sibling(&dst).symlink_metadata().is_err());

        // The name is taken: refused, the entry untouched, no temp left.
        std::fs::write(&dst, b"user's").unwrap();
        let e = copy_new(&src, &dst).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dst).unwrap(), b"user's");
        assert!(tmp_sibling(&dst).symlink_metadata().is_err());

        // Placing never replaces a file, or an empty folder (which a Unix rename would).
        let tmp = base.join("mods").join(".FS25_A.zip.silo-tmp");
        std::fs::write(&tmp, b"new").unwrap();
        assert!(place_new(&tmp, &dst).is_err());
        assert_eq!(std::fs::read(&dst).unwrap(), b"user's");
        let tmp_dir = base.join("mods").join(".FS25_D.silo-tmp");
        std::fs::create_dir_all(&tmp_dir).unwrap();
        std::fs::write(tmp_dir.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let empty = base.join("mods").join("FS25_D");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(place_new(&tmp_dir, &empty).is_err());
        assert!(std::fs::read_dir(&empty).unwrap().next().is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// Set or clear a file's read-only flag.
    fn set_readonly(p: &Path, on: bool) {
        let mut perms = std::fs::metadata(p).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)] // a test file in a temp folder
        perms.set_readonly(on);
        std::fs::set_permissions(p, perms).unwrap();
    }

    #[test]
    fn a_read_only_file_still_copies_and_flushes_and_keeps_its_flag() {
        let base = temp_dir_for("copy_readonly");
        let src = base.join("modDesc.xml");
        std::fs::write(&src, b"<modDesc/>").unwrap();
        set_readonly(&src, true);
        let dst = base.join("copy.xml");
        copy_file_synced(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"<modDesc/>");
        assert!(std::fs::metadata(&dst).unwrap().permissions().readonly());
        // Never over an existing file.
        assert!(copy_file_synced(&src, &dst).is_err());
        set_readonly(&dst, false);

        // A cut-off folder copy's temp holding a read-only file is still cleared, and the
        // next copy lands.
        let folder = base.join("FS25_Dir");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::copy(&src, folder.join("modDesc.xml")).unwrap();
        let dst = base.join("mods").join("FS25_Dir");
        std::fs::create_dir_all(tmp_sibling(&dst)).unwrap();
        std::fs::copy(&src, tmp_sibling(&dst).join("modDesc.xml")).unwrap();
        copy_dir_projection(&folder, &dst).unwrap();
        assert!(tmp_sibling(&dst).symlink_metadata().is_err());
        assert!(is_silo_projection(&dst, &folder, "dir"));
        set_readonly(&src, false);
        set_readonly(&folder.join("modDesc.xml"), false);
        set_readonly(&dst.join("modDesc.xml"), false);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_zip_copy_cut_off_after_some_bytes_leaves_nothing_under_the_mods_name() {
        let base = temp_dir_for("zipcopy_cutoff");
        let src = base.join("FS25_A.zip");
        std::fs::write(&src, b"PK\x03\x04 whole").unwrap();
        let dst = base.join("FS25_A_flat.zip");
        let disk_full = |_: &Path, tmp: &Path| -> std::io::Result<()> {
            std::fs::write(tmp, b"PK\x03")?;
            Err(std::io::Error::other("no space left on device"))
        };
        assert!(copy_new_with(&src, &dst, disk_full).is_err());
        assert!(dst.symlink_metadata().is_err(), "nothing at the real name");
        assert!(
            tmp_sibling(&dst).symlink_metadata().is_err(),
            "temp removed"
        );

        // A flush that fails after the whole copy was written: same.
        let flush_fails = |s: &Path, tmp: &Path| -> std::io::Result<()> {
            std::fs::copy(s, tmp)?;
            Err(std::io::Error::other("flush failed"))
        };
        assert!(copy_new_with(&src, &dst, flush_fails).is_err());
        assert!(dst.symlink_metadata().is_err() && tmp_sibling(&dst).symlink_metadata().is_err());

        copy_new(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), b"PK\x03\x04 whole");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_move_flushes_up_to_the_first_folder_that_already_existed() {
        let base = temp_dir_for("move_anchor");
        let from = base.join("FS25_A.zip");
        std::fs::write(&from, b"PK").unwrap();
        // New archive and category folders below an existing one.
        let to = base.join("new_archive").join("Tools").join("FS25_A.zip");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        copy_then_remove(&from, &to, Some(&base)).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"PK");
        assert!(!from.exists());
        let tools = to.parent().unwrap();
        assert_eq!(
            placement_dirs(&to, Some(&base)),
            [tools, tools.parent().unwrap(), base.as_path()]
        );
        assert_eq!(placement_dirs(&to, None), [tools]);
        assert_eq!(placement_dirs(&to, Some(tools)), [tools]);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_folder_filed_back_with_silos_marker_still_projects() {
        let base = temp_dir_for("dircopy_marker");
        let src = base.join("archived").join("FS25_Dir");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(src.join(SILO_MARKER), b"silo\n").unwrap();
        let dst = base.join("FS25_Dir");
        copy_dir_projection(&src, &dst).unwrap();
        assert!(marker_listing(&dst).unwrap().is_some(), "a fresh v2 marker");
        assert!(copy_unchanged(&dst, &src).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_move_never_replaces_what_is_at_its_destination() {
        let base = temp_dir_for("move_noreplace");
        let from = base.join("FS25_A.zip");
        std::fs::write(&from, b"PK mine").unwrap();
        let to = base.join("other").join("FS25_A.zip");
        std::fs::create_dir_all(to.parent().unwrap()).unwrap();
        std::fs::write(&to, b"PK theirs").unwrap();
        assert!(copy_then_remove(&from, &to, to.parent()).is_err());
        assert_eq!(std::fs::read(&from).unwrap(), b"PK mine");
        assert_eq!(std::fs::read(&to).unwrap(), b"PK theirs");
        assert!(tmp_sibling(&to).symlink_metadata().is_err());

        // Free destination: copied whole, source gone, no temp.
        std::fs::remove_file(&to).unwrap();
        copy_then_remove(&from, &to, to.parent()).unwrap();
        assert_eq!(std::fs::read(&to).unwrap(), b"PK mine");
        assert!(!from.exists() && tmp_sibling(&to).symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_cut_off_copy_temp_is_swept_and_the_next_activation_projects_the_mod() {
        let base = temp_dir_for("copytemp_sweep");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let mods = base.join("mods");
        let layout = Layout::default_for(&mods);
        let key = crate::modsroot::canonical_key(&mods);
        let zip = archive_path(&layout.archive, "Misc", "FS25_A.zip");
        let dir = archive_path(&layout.archive, "Misc", "FS25_D");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&zip, b"PK\x03\x04 whole").unwrap();
        std::fs::write(dir.join("modDesc.xml"), b"<modDesc/>").unwrap();
        for (name, kind) in [("FS25_A.zip", "zip"), ("FS25_D", "dir")] {
            let row = OrganizedRow {
                tech_name: name.trim_end_matches(".zip").into(),
                file_name: name.into(),
                kind: kind.into(),
                category: "Misc".into(),
                subcategory: None,
                active: false,
            };
            db::upsert_organized(&conn, &key, &row).unwrap();
        }
        // What Silo closing mid-copy leaves: half-written temps, nothing at the real names.
        let zip_tmp = tmp_sibling(&mods.join("FS25_A.zip"));
        let dir_tmp = tmp_sibling(&mods.join("FS25_D"));
        std::fs::write(&zip_tmp, b"PK half").unwrap();
        std::fs::create_dir_all(&dir_tmp).unwrap();
        std::fs::write(dir_tmp.join("modDesc.xml"), b"<mod").unwrap();
        assert!(crate::scan::is_silo_temp(".FS25_A.zip.silo-tmp"));
        assert!(crate::scan::is_silo_temp(".FS25_D.silo-tmp"));

        let on: HashSet<String> = ["FS25_A", "FS25_D"].iter().map(|s| s.to_string()).collect();
        let rep = set_active(&conn, &layout, &on);
        assert!(rep.errors.is_empty(), "{:?}", rep.errors);
        assert_eq!(rep.changed, 2);
        assert!(zip_tmp.symlink_metadata().is_err() && dir_tmp.symlink_metadata().is_err());
        assert_eq!(
            std::fs::read(mods.join("FS25_A.zip")).unwrap(),
            b"PK\x03\x04 whole"
        );
        assert_eq!(
            std::fs::read(mods.join("FS25_D").join("modDesc.xml")).unwrap(),
            b"<modDesc/>"
        );
        assert!(is_silo_projection(&mods.join("FS25_D"), &dir, "dir"));
        drop(conn);
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
        assert_eq!(volume_relation(&base, &base.join("sub")), Some(true));
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn known_same_mount_needs_both_mounts_known_and_equal() {
        let c = Some((PathBuf::from("C:\\"), 1));
        let d = Some((PathBuf::from("D:\\"), 1));
        assert!(known_same_mount(&c, &c.clone()));
        assert!(!known_same_mount(&c, &d));
        assert!(!known_same_mount(&c, &None));
        assert!(!known_same_mount(&None, &None));
    }

    /// On a mount the disk list can't identify where no probe can even be made (a path that
    /// won't resolve), the answer is inconclusive, not "another drive": linking falls back to
    /// the hardlink-first path, which copies safely if the link fails.
    #[test]
    fn an_unknown_mount_where_no_probe_can_run_is_inconclusive() {
        let base = std::env::temp_dir().join(format!("silo_unknown_mount_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        // Never created, so its mount can't be found and no probe can run there.
        let unknown = base.join("not-mounted");
        assert!(mount_of(&unknown).is_none());
        assert_eq!(volume_relation(&mods, &unknown), None);
        assert_eq!(hardlinks_between(&mods, &unknown), None);
        assert!(!unknown.exists(), "never creates a missing folder");
        let layout = Layout {
            mods: mods.clone(),
            archive: unknown,
        };
        assert!(same_volume_for_links(&layout));
        assert!(!LinkPolicy::new(&layout).can_only_copy());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_disk_list_decides_when_it_knows_and_the_probe_only_when_it_doesnt() {
        let probed = std::cell::Cell::new(false);
        let probe = |answer: Option<bool>| {
            let p = &probed;
            move || {
                p.set(true);
                answer
            }
        };
        assert!(same_volume_or_probe(Some(true), probe(Some(false))));
        assert!(!same_volume_or_probe(Some(false), probe(Some(true))));
        assert!(!probed.get(), "a known answer never probes");
        assert!(same_volume_or_probe(None, probe(Some(true))));
        assert!(probed.get());
        // A probe link that fails: another drive.
        assert!(!same_volume_or_probe(None, probe(Some(false))));
        // No probe could be made: inconclusive, so not "another drive".
        assert!(same_volume_or_probe(None, probe(None)));
    }

    fn probe_dirs(tag: &str) -> (PathBuf, PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("silo_probe_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (mods, archive) = (base.join("mods"), base.join("archive"));
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::create_dir_all(&archive).unwrap();
        (base, mods, archive)
    }

    #[test]
    fn hardlinks_between_probes_and_leaves_nothing_behind() {
        let (base, mods, archive) = probe_dirs("hl");
        // The temp dir is one volume (and not cloud-synced): the probe link works.
        assert_eq!(hardlinks_between(&mods, &archive), Some(true));
        assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
        // A link that fails is a real answer: another drive. Still nothing left behind.
        let refuse = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::Error::other("cross-device link"))
        };
        assert_eq!(probe_hardlink(&mods, &archive, refuse), Some(false));
        assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_probe_name_already_taken_moves_on_to_the_next() {
        let (base, _mods, archive) = probe_dirs("taken");
        std::fs::write(archive.join("p-1"), b"a stale probe").unwrap();
        let names = ["p-1".to_string(), "p-2".to_string()];
        let made = create_probe_named(&archive, names.into_iter()).unwrap();
        assert_eq!(made, archive.join("p-2"));
        assert_eq!(
            std::fs::read(archive.join("p-1")).unwrap(),
            b"a stale probe"
        );
        // A folder it can't create in: no probe (inconclusive), never an error.
        assert_eq!(
            create_probe_named(&base.join("missing"), ["p-3".to_string()].into_iter()),
            None
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn stale_link_probes_are_swept_and_fresh_ones_and_mods_are_kept() {
        let (base, mods, _archive) = probe_dirs("sweep");
        let old = mods.join(".silo-hardlink-probe-1-0.link");
        let old_sym = mods.join(".silo-symlink-probe-1-0.link");
        let fresh = mods.join(".silo-hardlink-probe-1-1.link");
        let mine = mods.join("FS25_A.zip");
        for p in [&old, &old_sym, &fresh, &mine] {
            std::fs::write(p, b"x").unwrap();
        }
        let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for p in [&old, &old_sym, &mine] {
            std::fs::File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(hour_ago)
                .unwrap();
        }
        sweep_link_probes(&mods, std::time::Duration::from_secs(600));
        assert!(!old.exists() && !old_sym.exists(), "stale probes removed");
        assert!(fresh.exists(), "a probe running right now is kept");
        assert!(mine.exists(), "never anything else");
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

    /// A symlink at `flat` into a drive that's gone. `None` where file symlinks can't be made
    /// (Windows without Developer Mode) — the test is skipped there.
    fn dangling_link(base: &Path, flat: &Path) -> Option<()> {
        symlink_file(&base.join("unplugged").join("FS25_Foo.zip"), flat).ok()
    }

    #[test]
    fn a_dangling_link_at_a_mods_name_is_stale_and_never_blocks_activation() {
        let (base, conn, src, flat) = symlink_fixture("dangle", false);
        let layout = Layout::default_for(&base);
        let key = crate::modsroot::canonical_key(&base);
        let foo: HashSet<String> = ["FS25_Foo".to_string()].into_iter().collect();
        if dangling_link(&base, &flat).is_none() {
            return;
        }
        // Activate: the stale link is removed and a working projection made.
        let rep = set_active(&conn, &layout, &foo);
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert_eq!(
            std::fs::read(&flat).unwrap(),
            b"PK\x03\x04 the archived build"
        );
        assert!(db::load_organized(&conn, &key)[0].active);
        // Deactivate over a dangling link: removed, and the manifest says inactive.
        std::fs::remove_file(&flat).unwrap();
        dangling_link(&base, &flat).unwrap();
        let rep = set_active(&conn, &layout, &HashSet::new());
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(flat.symlink_metadata().is_err());
        assert!(!db::load_organized(&conn, &key)[0].active);
        // Flatten over a dangling link: the archived file comes home.
        dangling_link(&base, &flat).unwrap();
        let rep = flatten(&conn, &layout);
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(flat.symlink_metadata().unwrap().file_type().is_file());
        assert!(!src.exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A dangling dir junction (no privilege needed, unlike symlinks): link, then delete the
    /// target. False if `mklink` failed.
    #[cfg(windows)]
    fn dangling_junction(link: &Path, gone: &Path) -> bool {
        std::fs::create_dir_all(gone).unwrap();
        let out = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(link)
            .arg(gone)
            .output();
        std::fs::remove_dir_all(gone).unwrap();
        match out {
            Ok(o) if o.status.success() => true,
            Ok(o) => {
                eprintln!(
                    "mklink: {}{}",
                    String::from_utf8_lossy(&o.stdout),
                    String::from_utf8_lossy(&o.stderr)
                );
                false
            }
            Err(e) => {
                eprintln!("mklink: {e}");
                false
            }
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_dangling_junction_at_a_folder_mods_name_never_blocks_activation() {
        let base = std::env::temp_dir().join(format!("silo_junction_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let cat = base.join(ARCHIVE).join("Maps");
        std::fs::create_dir_all(cat.join("FS25_Map")).unwrap();
        std::fs::write(cat.join("FS25_Map/modDesc.xml"), b"<modDesc/>").unwrap();
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Map".into(),
                file_name: "FS25_Map".into(),
                kind: "dir".into(),
                category: "Maps".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        let flat = base.join("FS25_Map");
        // Junctions need no privilege on NTFS, so this always runs on Windows.
        let gone = base.join("unplugged").join("FS25_Map");
        assert!(dangling_junction(&flat, &gone), "mklink /J failed");
        let layout = Layout::default_for(&base);
        // Park: the stale junction goes, the manifest says parked.
        let rep = set_active(&conn, &layout, &HashSet::new());
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(flat.symlink_metadata().is_err());
        assert!(!db::load_organized(&conn, &key)[0].active);
        // Activate over another stale junction: a working projection replaces it.
        assert!(dangling_junction(&flat, &gone));
        let want: HashSet<String> = ["FS25_Map".to_string()].into_iter().collect();
        let rep = set_active(&conn, &layout, &want);
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(flat.join("modDesc.xml").is_file());
        assert!(
            cat.join("FS25_Map/modDesc.xml").is_file(),
            "the archived mod is untouched"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[cfg(windows)]
    #[test]
    fn a_folder_mod_is_projected_as_a_link_not_a_copy_on_windows() {
        // Junctions need no privilege on NTFS, so this must never fall back to a copy.
        let base = std::env::temp_dir().join(format!("silo_junction_proj_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let archive_mod = mods.join(ARCHIVE).join("Misc").join("FS25_Dir");
        std::fs::create_dir_all(&archive_mod).unwrap();
        std::fs::write(archive_mod.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let link = mods.join("FS25_Dir");
        let layout = Layout::default_for(&mods);

        make_link(&archive_mod, &link, "dir", &mut LinkPolicy::new(&layout)).unwrap();

        let ft = link.symlink_metadata().unwrap().file_type();
        assert!(
            ft.is_symlink(),
            "expected a junction or symlink, got a real folder"
        );
        assert!(
            !link.join(SILO_MARKER).exists(),
            "must not be a marked copy"
        );
        assert!(is_silo_projection(&link, &archive_mod, "dir"));
        // An edit made through the link lands in the archived mod.
        std::fs::write(link.join("edit.txt"), b"hello").unwrap();
        assert!(archive_mod.join("edit.txt").is_file());
        // Removing the link never touches the archived mod.
        remove_link(&link).unwrap();
        assert!(link.symlink_metadata().is_err());
        assert!(archive_mod.join("modDesc.xml").is_file());
        assert!(archive_mod.join("edit.txt").is_file());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// The junction branch on its own: this machine may have Developer Mode (so `symlink_dir`
    /// wins in `make_link`), but users and the CI runner usually don't.
    #[cfg(windows)]
    #[test]
    fn junction_to_links_a_folder_without_privilege() {
        let base = std::env::temp_dir().join(format!("silo_junction_to_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let target = base.join(ARCHIVE).join("Misc").join("FS25_Dir");
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let link = base.join("FS25_Dir");

        junction_to(&target, &link).unwrap();

        assert!(junction::exists(&link).unwrap(), "a real NTFS junction");
        assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
        assert!(is_silo_projection(&link, &target, "dir"));
        assert_eq!(
            std::fs::read(link.join("modDesc.xml")).unwrap(),
            b"<modDesc/>"
        );
        remove_link(&link).unwrap();
        assert!(link.symlink_metadata().is_err());
        assert!(
            target.join("modDesc.xml").is_file(),
            "the target is untouched"
        );

        // Never replaces or merges into something already at the link path.
        let theirs = base.join("FS25_Theirs");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("mine.txt"), b"keep").unwrap();
        let err = junction_to(&target, &theirs).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(theirs.join("mine.txt")).unwrap(), b"keep");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remove_dangling_link_never_touches_a_file_or_a_working_link() {
        let (base, _conn, src, flat) = symlink_fixture("notdangle", true);
        std::fs::write(&flat, b"the user's own build").unwrap();
        assert!(!remove_dangling_link(&flat).unwrap());
        assert_eq!(std::fs::read(&flat).unwrap(), b"the user's own build");
        std::fs::remove_file(&flat).unwrap();
        if symlink_file(&src, &flat).is_ok() {
            assert!(!remove_dangling_link(&flat).unwrap());
            assert!(flat.exists(), "a working link stays");
        }
        assert!(!remove_dangling_link(&base.join("nothing-here")).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A default-layout mods folder holding one organized zip `FS25_Foo.zip` in `Vehicles`.
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
    fn deactivate_removes_a_silo_folder_symlink_and_keeps_the_folder() {
        // rmdir on a symlink fails on Unix (ENOTDIR), so a folder link must be unlinked.
        let (base, conn, _, _) = symlink_fixture("deactdir", false);
        let src = base.join(ARCHIVE).join("Vehicles").join("FS25_Dir");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let key = crate::modsroot::canonical_key(&base);
        db::upsert_organized(
            &conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Dir".into(),
                file_name: "FS25_Dir".into(),
                kind: "dir".into(),
                category: "Vehicles".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        let link = base.join("FS25_Dir");
        std::os::unix::fs::symlink(&src, &link).unwrap();
        let rep = set_active(&conn, &Layout::default_for(&base), &HashSet::new());
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(link.symlink_metadata().is_err(), "the folder link is gone");
        assert!(
            src.join("modDesc.xml").is_file(),
            "the archived folder stays"
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

    fn temp_dir_for(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("silo_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    #[test]
    fn an_untouched_folder_copy_is_recognised_as_unchanged() {
        let base = temp_dir_for("copy_unchanged");
        let src = base.join("src");
        std::fs::create_dir_all(src.join("sub")).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(src.join("sub").join("a.lua"), b"print(1)").unwrap();
        let copy = base.join("copy");
        copy_dir_projection(&src, &copy).unwrap();
        assert!(copy_unchanged(&copy, &src).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn an_edited_added_or_removed_file_makes_the_copy_changed() {
        for edit in ["modify", "add", "remove"] {
            let base = temp_dir_for(&format!("copy_changed_{edit}"));
            let src = base.join("src");
            std::fs::create_dir_all(&src).unwrap();
            std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
            std::fs::write(src.join("a.lua"), b"print(1)").unwrap();
            let copy = base.join("copy");
            copy_dir_projection(&src, &copy).unwrap();
            match edit {
                "modify" => {
                    // Same size: only the mtime tells. Push it well past the listed one so the
                    // test can't land in the same millisecond (or FAT's 2 s granularity).
                    std::fs::write(copy.join("a.lua"), b"print(2)").unwrap();
                    std::fs::File::options()
                        .write(true)
                        .open(copy.join("a.lua"))
                        .unwrap()
                        .set_modified(
                            std::time::SystemTime::now() + std::time::Duration::from_secs(10),
                        )
                        .unwrap();
                }
                "add" => std::fs::write(copy.join("new.lua"), b"x").unwrap(),
                _ => std::fs::remove_file(copy.join("a.lua")).unwrap(),
            }
            assert!(
                !copy_unchanged(&copy, &src).unwrap(),
                "{edit} must count as changed"
            );
            let _ = std::fs::remove_dir_all(&base);
        }
    }

    #[test]
    fn a_legacy_marker_is_compared_against_the_archived_source() {
        let base = temp_dir_for("copy_legacy");
        let src = base.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let copy = base.join("copy");
        std::fs::create_dir_all(&copy).unwrap();
        std::fs::write(copy.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(copy.join(SILO_MARKER), b"silo\n").unwrap(); // pre-v2 marker
        assert!(copy_unchanged(&copy, &src).unwrap());
        std::fs::write(copy.join("modDesc.xml"), b"<modDesX/>").unwrap(); // same size, new bytes
        assert!(!copy_unchanged(&copy, &src).unwrap());
        std::fs::write(copy.join("extra.lua"), b"x").unwrap();
        assert!(!copy_unchanged(&copy, &src).unwrap());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn remove_link_never_deletes_a_real_folder_it_cannot_prove_untouched() {
        let base = temp_dir_for("rmlink_dir");
        // A user's own folder (no marker): refused, contents kept.
        let theirs = base.join("FS25_Theirs");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("modDesc.xml"), b"<modDesc/>").unwrap();
        assert!(remove_link(&theirs).is_err());
        assert!(theirs.join("modDesc.xml").is_file());
        // A legacy-marked copy: no listing to prove it, refused.
        std::fs::write(theirs.join(SILO_MARKER), b"silo\n").unwrap();
        assert!(remove_link(&theirs).is_err());
        assert!(theirs.join("modDesc.xml").is_file());
        // Silo's v2 copy: removed while untouched, refused once edited.
        let src = base.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let copy = base.join("FS25_Copy");
        copy_dir_projection(&src, &copy).unwrap();
        std::fs::write(copy.join("edit.lua"), b"x").unwrap();
        assert!(remove_link(&copy).is_err());
        assert!(copy.join("edit.lua").is_file());
        std::fs::remove_file(copy.join("edit.lua")).unwrap();
        remove_link(&copy).unwrap();
        assert!(copy.symlink_metadata().is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A default-layout mods folder at `base` with an active organized folder mod `FS25_Dir`
    /// (archived in `Misc`) whose mods-folder entry is a marked COPY. Returns (src, flat).
    fn folder_copy_fixture(base: &Path, conn: &Connection) -> (PathBuf, PathBuf) {
        let key = crate::modsroot::canonical_key(base);
        let src = base.join(ARCHIVE).join("Misc").join("FS25_Dir");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        std::fs::write(src.join("a.lua"), b"print(1)").unwrap();
        db::upsert_organized(
            conn,
            &key,
            &OrganizedRow {
                tech_name: "FS25_Dir".into(),
                file_name: "FS25_Dir".into(),
                kind: "dir".into(),
                category: "Misc".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();
        let flat = base.join("FS25_Dir");
        copy_dir_projection(&src, &flat).unwrap();
        (src, flat)
    }

    /// The single folder under `<mods>/backups/`, if exactly one exists.
    fn only_backup(base: &Path) -> PathBuf {
        let found: Vec<PathBuf> = std::fs::read_dir(base.join("backups"))
            .unwrap()
            .flatten()
            .map(|e| e.path())
            .collect();
        assert_eq!(found.len(), 1, "one backup: {found:?}");
        found.into_iter().next().unwrap()
    }

    #[test]
    fn parking_an_untouched_folder_copy_just_removes_it() {
        let base = temp_dir_for("park_copy_clean");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let (src, flat) = folder_copy_fixture(&base, &conn);
        let rep = set_active(&conn, &Layout::default_for(&base), &HashSet::new());
        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(rep.notices.is_empty(), "{:?}", rep.notices);
        assert!(flat.symlink_metadata().is_err());
        assert!(!base.join("backups").exists());
        assert!(src.join("a.lua").is_file());
        drop(conn);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parking_a_changed_folder_copy_moves_it_to_backups() {
        let base = temp_dir_for("park_copy_edit");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let (src, flat) = folder_copy_fixture(&base, &conn);
        std::fs::write(flat.join("a.lua"), b"print('my fix')").unwrap();

        let rep = set_active(&conn, &Layout::default_for(&base), &HashSet::new());

        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        assert!(
            flat.symlink_metadata().is_err(),
            "the mods-folder slot is free"
        );
        assert!(!db::load_organized(&conn, &key)[0].active);
        assert_eq!(std::fs::read(src.join("a.lua")).unwrap(), b"print(1)");
        let kept = only_backup(&base);
        let name = kept.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("FS25_Dir."), "{name}");
        assert_eq!(
            std::fs::read(kept.join("a.lua")).unwrap(),
            b"print('my fix')"
        );
        assert!(
            !kept.join(SILO_MARKER).exists(),
            "the kept copy is the user's now"
        );
        assert_eq!(rep.notices.len(), 1);
        assert!(
            rep.notices[0]
                .starts_with("FS25_Dir had changed since Silo copied it, so it was kept in ")
                && rep.notices[0].contains(&name),
            "{:?}",
            rep.notices
        );
        drop(conn);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_failed_rename_to_backups_leaves_the_copy_exactly_as_it_was() {
        let base = temp_dir_for("retire_inuse");
        let src = base.join(ARCHIVE).join("Misc").join("FS25_Dir");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let flat = base.join("FS25_Dir");
        copy_dir_projection(&src, &flat).unwrap();
        std::fs::write(flat.join("edit.lua"), b"mine").unwrap();
        let marker = std::fs::read(flat.join(SILO_MARKER)).unwrap();
        let in_use_msg = "Couldn't move FS25_Dir to backups because a file in it is in use. Close any program using it (the game, an editor) and try again.";

        let mut cases: Vec<(std::io::Error, String)> = vec![
            (
                std::io::ErrorKind::PermissionDenied.into(),
                in_use_msg.to_string(),
            ),
            (
                std::io::Error::other("the disk is full"),
                "Couldn't move FS25_Dir to backups (the disk is full), so Silo left it in place."
                    .to_string(),
            ),
        ];
        if cfg!(windows) {
            // ERROR_SHARING_VIOLATION / ERROR_LOCK_VIOLATION
            cases.push((
                std::io::Error::from_raw_os_error(32),
                in_use_msg.to_string(),
            ));
            cases.push((
                std::io::Error::from_raw_os_error(33),
                in_use_msg.to_string(),
            ));
        }
        for (fail, want) in cases {
            let err =
                retire_folder_copy_with(&flat, &src, &base, move |_: &Path, _: &Path| Err(fail))
                    .unwrap_err();
            assert_eq!(err.to_string(), want);
            assert_eq!(std::fs::read(flat.join("edit.lua")).unwrap(), b"mine");
            assert_eq!(std::fs::read(flat.join(SILO_MARKER)).unwrap(), marker);
            assert!(flat.join("modDesc.xml").is_file());
            assert!(!base.join("backups").exists(), "no empty backups folder");
        }

        // A backups/ folder that was already there is never removed.
        std::fs::create_dir_all(base.join("backups")).unwrap();
        let fail = |_: &Path, _: &Path| -> std::io::Result<()> {
            Err(std::io::ErrorKind::PermissionDenied.into())
        };
        assert!(retire_folder_copy_with(&flat, &src, &base, fail).is_err());
        assert!(base.join("backups").is_dir(), "the existing backups/ stays");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn backup_folder_names_never_collide() {
        let base = temp_dir_for("backup_names");
        let first = backup_path_at(&base, "FS25_Dir", 100, "").unwrap();
        assert_eq!(first, base.join("backups").join("FS25_Dir.100"));
        std::fs::create_dir_all(&first).unwrap();
        let second = backup_path_at(&base, "FS25_Dir", 100, "").unwrap();
        assert_eq!(second, base.join("backups").join("FS25_Dir.100-2"));
        std::fs::create_dir_all(&second).unwrap();
        assert_eq!(
            backup_path_at(&base, "FS25_Dir", 100, "").unwrap(),
            base.join("backups").join("FS25_Dir.100-3")
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn file_backup_names_never_collide() {
        let base = temp_dir_for("backup_file_names");
        let first = backup_path_at(&base, "FS25_A.zip", 100, UPDATE_BACKUP_SUFFIX).unwrap();
        assert_eq!(
            first,
            base.join("backups").join("FS25_A.zip.100.update.bak")
        );
        std::fs::write(&first, b"v1").unwrap();
        assert_eq!(
            backup_path_at(&base, "FS25_A.zip", 100, UPDATE_BACKUP_SUFFIX).unwrap(),
            base.join("backups").join("FS25_A.zip.100-2.update.bak")
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn parking_a_legacy_marked_copy_compares_it_with_the_archive() {
        for edited in [false, true] {
            let base = temp_dir_for(&format!("park_legacy_{edited}"));
            let conn = db::open(&base.join("silo.db")).unwrap();
            let (src, flat) = folder_copy_fixture(&base, &conn);
            // Rewrite the projection as a pre-v2 copy: plain copy plus the old marker.
            std::fs::remove_dir_all(&flat).unwrap();
            copy_dir_all(&src, &flat).unwrap();
            std::fs::write(flat.join(SILO_MARKER), b"silo\n").unwrap();
            if edited {
                std::fs::write(flat.join("a.lua"), b"print(9)").unwrap(); // same size
            }

            let rep = set_active(&conn, &Layout::default_for(&base), &HashSet::new());

            assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
            assert!(flat.symlink_metadata().is_err());
            assert_eq!(std::fs::read(src.join("a.lua")).unwrap(), b"print(1)");
            if edited {
                let kept = only_backup(&base);
                assert_eq!(std::fs::read(kept.join("a.lua")).unwrap(), b"print(9)");
                assert_eq!(rep.notices.len(), 1);
            } else {
                assert!(!base.join("backups").exists());
                assert!(rep.notices.is_empty());
            }
            drop(conn);
            let _ = std::fs::remove_dir_all(&base);
        }
    }

    #[test]
    fn a_copy_whose_marker_was_deleted_is_left_alone() {
        let base = temp_dir_for("copy_no_marker");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let (src, flat) = folder_copy_fixture(&base, &conn);
        std::fs::remove_file(flat.join(SILO_MARKER)).unwrap();
        std::fs::write(flat.join("a.lua"), b"print('mine')").unwrap();
        let layout = Layout::default_for(&base);

        let rep = set_active(&conn, &layout, &HashSet::new());
        assert_eq!(rep.changed, 0);
        assert_eq!(rep.errors.len(), 1, "{:?}", rep.errors);
        assert_eq!(std::fs::read(flat.join("a.lua")).unwrap(), b"print('mine')");

        let rep = flatten(&conn, &layout);
        assert_eq!(rep.changed, 0);
        assert_eq!(std::fs::read(flat.join("a.lua")).unwrap(), b"print('mine')");
        assert!(src.join("a.lua").is_file(), "the archived copy stays put");
        assert_eq!(db::load_organized(&conn, &key).len(), 1, "still organized");
        assert!(!base.join("backups").exists());
        drop(conn);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn network_paths_are_recognised() {
        assert!(is_network_path(r"\\server\share\mods"));
        assert!(is_network_path(r"\\?\UNC\server\share\mods"));
        assert!(is_network_path(r"\\?\unc\server\share"));
        assert!(is_network_path(r"\\.\UNC\server\share"));
        assert!(!is_network_path(r"\\?\C:\Users\me\mods"));
        assert!(!is_network_path(r"\\.\C:\Users\me\mods"));
        assert!(!is_network_path(r"C:\Users\me\mods"));
        assert!(!is_network_path("/home/me/mods"));
        assert!(!is_network_path(r"\\?\UN"));
    }

    /// A working junction at an organized folder mod's name that points somewhere other than
    /// the archive is the user's: park and flatten leave it, and its target, alone.
    #[cfg(windows)]
    #[test]
    fn a_junction_to_a_folder_outside_the_archive_is_never_removed() {
        let base = temp_dir_for("junction_foreign");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let key = crate::modsroot::canonical_key(&base);
        let (src, flat) = folder_copy_fixture(&base, &conn);
        std::fs::remove_dir_all(&flat).unwrap();
        let theirs = base.join("elsewhere").join("FS25_Dir");
        std::fs::create_dir_all(&theirs).unwrap();
        std::fs::write(theirs.join("modDesc.xml"), b"<theirs/>").unwrap();
        junction_to(&theirs, &flat).unwrap();
        assert!(!is_silo_projection(&flat, &src, "dir"));
        let layout = Layout::default_for(&base);

        let rep = set_active(&conn, &layout, &HashSet::new());
        assert_eq!(rep.changed, 0, "{:?}", rep.errors);
        assert!(junction::exists(&flat).unwrap(), "the junction stays");

        let rep = flatten(&conn, &layout);
        assert_eq!(rep.changed, 0, "{:?}", rep.errors);
        assert!(junction::exists(&flat).unwrap(), "the junction stays");
        assert_eq!(
            std::fs::read(theirs.join("modDesc.xml")).unwrap(),
            b"<theirs/>"
        );
        assert!(src.join("a.lua").is_file(), "the archived copy stays put");
        assert_eq!(db::load_organized(&conn, &key).len(), 1);
        drop(conn);
        let _ = std::fs::remove_dir(&flat);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn flattening_a_changed_folder_copy_keeps_the_edits_in_backups() {
        let base = temp_dir_for("flat_copy_edit");
        let conn = db::open(&base.join("silo.db")).unwrap();
        let (src, flat) = folder_copy_fixture(&base, &conn);
        std::fs::write(flat.join("new.lua"), b"added by me").unwrap();

        let rep = flatten(&conn, &Layout::default_for(&base));

        assert_eq!(rep.changed, 1, "errors: {:?}", rep.errors);
        // The archived original is back in the mods folder as a plain folder.
        let meta = flat.symlink_metadata().unwrap();
        assert!(meta.is_dir() && !meta.file_type().is_symlink());
        assert_eq!(std::fs::read(flat.join("a.lua")).unwrap(), b"print(1)");
        assert!(!flat.join("new.lua").exists());
        assert!(!src.exists(), "moved back out of the archive");
        // The edited copy is kept.
        let kept = only_backup(&base);
        assert_eq!(std::fs::read(kept.join("new.lua")).unwrap(), b"added by me");
        assert_eq!(rep.notices.len(), 1, "{:?}", rep.notices);
        assert!(
            rep.notices[0]
                .starts_with("FS25_Dir had changed since Silo copied it, so it was moved to ")
                && rep.notices[0].ends_with("Your mods folder now has Silo's original."),
            "{:?}",
            rep.notices
        );
        drop(conn);
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
