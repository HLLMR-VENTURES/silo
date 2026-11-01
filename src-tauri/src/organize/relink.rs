//! Relink: collapse copy-projections into links, and atomically re-point a zip's mods-folder
//! entry at a new archived file.

use super::*;

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
pub(super) enum RelinkMode {
    Hardlink,
    Symlink,
    /// Same volume, but hardlinks are rejected (cloud-synced folder).
    SameVolumeNoHardlinks,
    /// Archive on another drive and symlinks unavailable.
    CrossDriveNoSymlinks,
}

pub(super) fn relink_with(
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
    // A new entry that reaches no file (the copy went while it was linked) never replaces the
    // working one.
    if !std::fs::metadata(&tmp).is_ok_and(|m| m.is_file()) {
        let _ = remove_path(&tmp);
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "the new copy of the mod isn't there",
        ));
    }
    std::fs::rename(&tmp, link).inspect_err(|_| {
        let _ = remove_path(&tmp);
    })?;
    crate::fslog::moved("move", &tmp, link);
    Ok(())
}

/// `replace_zip_link`, but only for a real link: a hardlink to `dst`, else a symlink where
/// `policy` allows one — never a copy. `Ok(false)` when neither could be made: `link` is left
/// exactly as it was (for a move, the mod's hardlink then stays as its standalone copy instead
/// of being traded for a second full copy). A `dst` that isn't there (any more) is an error,
/// never a link to nothing: like `link_zip`, a `NotFound` or `AlreadyExists` from the hardlink
/// stops here, and the new entry must reach a file before it replaces `link` and again after,
/// so a caller never deletes the old copy behind a dangling link.
pub(crate) fn replace_zip_link_if_linkable(
    link: &Path,
    dst: &Path,
    policy: &mut LinkPolicy,
) -> std::io::Result<bool> {
    replace_zip_link_if_linkable_with(
        link,
        dst,
        || policy.allow_symlink(),
        |s, l| std::fs::hard_link(s, l),
        symlink_file,
    )
}

/// `replace_zip_link_if_linkable` with the link strategies injected (tests).
pub(super) fn replace_zip_link_if_linkable_with(
    link: &Path,
    dst: &Path,
    allow_symlink: impl FnOnce() -> bool,
    hard: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    soft: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<bool> {
    use std::io::ErrorKind::{AlreadyExists, NotFound};
    let reaches_a_file = |p: &Path| std::fs::metadata(p).is_ok_and(|m| m.is_file());
    let gone = || std::io::Error::new(NotFound, "the new copy of the mod isn't there");
    if !reaches_a_file(dst) {
        return Err(gone());
    }
    let tmp = relink_tmp(link);
    let _ = remove_path(&tmp);
    let linked = match hard(dst, &tmp) {
        Ok(()) => true,
        Err(e) if matches!(e.kind(), AlreadyExists | NotFound) => return Err(e),
        Err(_) if allow_symlink() => match soft(dst, &tmp) {
            Ok(()) => true,
            Err(e) if e.kind() == AlreadyExists => return Err(e),
            Err(_) => false,
        },
        Err(_) => false,
    };
    if !linked {
        return Ok(false);
    }
    if !reaches_a_file(&tmp) {
        let _ = remove_path(&tmp);
        return Err(gone());
    }
    std::fs::rename(&tmp, link).inspect_err(|_| {
        let _ = remove_path(&tmp);
    })?;
    crate::fslog::moved("link", dst, link);
    if !reaches_a_file(link) {
        return Err(gone());
    }
    Ok(true)
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
