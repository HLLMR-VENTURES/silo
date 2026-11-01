//! Copy-fallback folder projections: the ownership marker and listing, proving a copy
//! unchanged, and retiring it (removed, or kept in `backups/` when edited).

use super::*;

/// The ownership marker of a folder copy (v2): every file Silo wrote, so a later park or
/// flatten can tell an untouched copy (safe to delete) from one the user edited.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct CopyListing {
    v: u32,
    files: Vec<ListedFile>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
pub(super) struct ListedFile {
    /// Relative to the copy, '/'-separated.
    path: String,
    size: u64,
    mtime_ms: u64,
}

/// Every regular file under `dir` except the marker, sorted by path. Links are not followed.
pub(super) fn listing_of(dir: &Path) -> std::io::Result<Vec<ListedFile>> {
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
/// built and flushed under a new Silo temp sibling (`.{name}.<token>.silo-tmp`), marker included,
/// and only then renamed to `dst`: a copy that stops part-way never leaves a partial mod under
/// its real name, and a failed copy removes its temp once it's proven redundant
/// (`discard_copy`). Never merges into an existing `dst`.
pub(crate) fn copy_dir_projection(src: &Path, dst: &Path) -> std::io::Result<()> {
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let tmp = tmp_sibling(dst);
    // Made exclusively: a temp name that's somehow taken is left alone.
    std::fs::create_dir(&tmp)?;
    if let Err(e) = build_dir_copy(src, &tmp).and_then(|()| place_new(&tmp, dst)) {
        discard_copy(&tmp, src);
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
pub(super) fn marker_listing(dir: &Path) -> std::io::Result<Option<Vec<ListedFile>>> {
    let raw = std::fs::read(dir.join(SILO_MARKER))?;
    Ok(match serde_json::from_slice::<CopyListing>(&raw) {
        Ok(l) if l.v == 2 => Some(l.files),
        _ => None,
    })
}

/// What [`copy_state`] found a marked folder copy to be.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum CopyState {
    /// Provably what Silo wrote.
    Unchanged,
    /// Its v2 listing shows a file added, removed, resized or re-timed: the user changed it.
    Edited,
    /// A legacy marker (no listing) and the copy doesn't match the archived copy, or couldn't
    /// be compared with it: an edit and a damaged archive copy look the same (#399).
    Unproven,
}

/// Whether the marked folder copy at `dir` is what Silo wrote. With a v2 listing: the same
/// files with the same sizes and modification times (any added, removed, resized or re-timed
/// file counts as changed). Limit: an edit that keeps a file's size and lands within the same
/// timestamp tick as the copy (same millisecond, or FAT's 2 s) can't be seen; in practice an
/// edit comes much later. A legacy marker has no listing, so the copy is compared with the
/// archived `src` by relative path, size and SHA-256. `Err` when the marker or the copy can't
/// be read: nothing is proven either way.
pub(super) fn copy_state(dir: &Path, src: &Path) -> std::io::Result<CopyState> {
    Ok(match marker_listing(dir)? {
        Some(files) if listing_of(dir)? == files => CopyState::Unchanged,
        Some(_) => CopyState::Edited,
        None if same_tree_contents(dir, src).unwrap_or(false) => CopyState::Unchanged,
        None => CopyState::Unproven,
    })
}

/// True only when the marked folder copy at `dir` is provably what Silo wrote ([`copy_state`]).
#[cfg(test)]
pub(crate) fn copy_unchanged(dir: &Path, src: &Path) -> std::io::Result<bool> {
    copy_state(dir, src).map(|s| s == CopyState::Unchanged)
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

/// Remove a marked folder copy only if unchanged and its archived copy `src` still holds every
/// file of it (`scratch::copy_held_by_archive`); an unchanged copy the archive doesn't hold is
/// kept where it is, with an error saying why (it may be the only good copy, #399), and so is a
/// legacy-marked copy that doesn't match `src` (an edit or a damaged archive copy, which Silo
/// can't tell apart). A copy its v2 listing shows was changed is moved to
/// `<mods_root>/backups/<name>.<secs>/` and that path returned so the caller can tell the user.
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
pub(super) fn retire_folder_copy_with(
    link: &Path,
    src: &Path,
    mods_root: &Path,
    rename: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<Option<PathBuf>> {
    match copy_state(link, src) {
        Ok(CopyState::Unchanged) => {
            if !crate::scratch::copy_held_by_archive(link, src) {
                return Err(not_held_by_archive(link, src));
            }
            std::fs::remove_dir_all(link)?;
            crate::fslog::changed("unlink", link);
            return Ok(None);
        }
        Ok(CopyState::Edited) => {}
        // Only a v2 listing that was read proves an edit; anything less stays where it is
        // (#399). A legacy marker only says the copy differs from the archived copy, which
        // may be the archive's damage.
        Ok(CopyState::Unproven) => return Err(legacy_copy_differs(link, src)),
        Err(e) => return Err(couldnt_check(link, &e)),
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
/// through [`retire_folder_copy`] (an edited one is kept in backups, its path returned). A zip
/// (a copy, or a hardlink) goes only while the archived `src` is a real zip holding the same
/// bytes (`scratch::copy_held_by_archive`); otherwise it's kept, with an error saying why
/// (#399). Anything else (a link) through [`remove_link`].
pub(super) fn clear_projection(
    link: &Path,
    src: &Path,
    mods_root: &Path,
) -> std::io::Result<Option<PathBuf>> {
    let meta = link.symlink_metadata()?;
    let real = !meta.file_type().is_symlink();
    if real && meta.is_dir() && link.join(SILO_MARKER).is_file() {
        return retire_folder_copy(link, src, mods_root);
    }
    if real && meta.is_file() && !crate::scratch::copy_held_by_archive(link, src) {
        return Err(not_held_by_archive(link, src));
    }
    remove_link(link).map(|()| None)
}

/// What the user is told when Silo couldn't read a marked copy (or its marker) to tell whether it
/// changed: it's left where it is.
fn couldnt_check(link: &Path, e: &std::io::Error) -> std::io::Error {
    let name = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| link.display().to_string());
    std::io::Error::other(format!(
        "Silo couldn't check whether {name} changed since it was copied ({e}), so it left it in your mods folder."
    ))
}

/// What the user is told when a legacy-marked copy (no listing of what Silo wrote) doesn't match
/// its archived copy: it may have been changed, or the archive copy may be damaged.
fn legacy_copy_differs(link: &Path, src: &Path) -> std::io::Error {
    log::warn!(
        "kept {}: it doesn't match the archived copy at {} and has no listing to tell why",
        link.display(),
        src.display()
    );
    let name = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| link.display().to_string());
    std::io::Error::other(format!(
        "{name} stays in your mods folder: it doesn't match the copy in your library archive, and Silo can't tell whether it was changed or the archive copy is damaged. To turn the mod off, move it out of your mods folder yourself."
    ))
}

/// What the user is told when a copy in the mods folder is kept because its archived copy is
/// missing, a link, or doesn't hold the same bytes: removing it could lose the only good copy.
fn not_held_by_archive(link: &Path, src: &Path) -> std::io::Error {
    log::warn!(
        "kept {}: the archived copy at {} doesn't hold the same bytes",
        link.display(),
        src.display()
    );
    let name = link
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| link.display().to_string());
    std::io::Error::other(format!(
        "{name} stays in your mods folder: Silo couldn't confirm the copy in your library archive matches it (it may be missing, damaged or incomplete), so removing this one could lose the only good copy."
    ))
}
