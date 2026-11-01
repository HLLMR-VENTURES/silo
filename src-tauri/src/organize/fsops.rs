//! Filesystem primitives: moves (same-drive rename, cross-drive copy-then-remove), no-replace
//! copies flushed to disk, and their temp-file handling.

use super::*;

// ── filesystem primitives ──

/// A new hidden temp sibling of `to` (`.{name}.<token>.silo-tmp`, which scans skip), where a
/// cross-drive move or a copy projection is built before it's placed at `to`. A name of its
/// own each time (#394): nothing is ever removed at it before use, and a leftover from an
/// older version (`.{name}.silo-tmp`) is never touched.
pub(super) fn tmp_sibling(to: &Path) -> PathBuf {
    let name = to
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "x".into());
    crate::scratch::unique_beside(&to.with_file_name(format!(".{name}")), ".silo-tmp")
}

/// Remove the temp `tmp` a copy of `src` just made, after a failure — only once it's proven
/// redundant: every byte it holds is still in `src` (`scratch::redundant_copy`). One that
/// isn't (the source changed or went while it was copied) is kept and logged.
pub(super) fn discard_copy(tmp: &Path, src: &Path) {
    if tmp.symlink_metadata().is_err() {
        return;
    }
    if crate::scratch::redundant_copy(tmp, src) {
        let _ = remove_path(tmp);
    } else {
        log::warn!(
            "kept {}: it may hold the only copy of what was at {}",
            tmp.display(),
            src.display()
        );
    }
}

/// Remove a path whatever it is (file/dir/link), treating "not found" as success.
pub(super) fn remove_path(p: &Path) -> std::io::Result<()> {
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
    move_path_checked(from, to, action, None)
}

/// [`move_path_as`], asking `room` (`LinkPolicy::room_check`) before a cross-drive copy starts.
pub(crate) fn move_path_checked(
    from: &Path,
    to: &Path,
    action: &str,
    room: Option<RoomCheck>,
) -> std::io::Result<()> {
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
        Err(_) => {
            check_room(room, from, to)?;
            copy_then_remove(from, to, anchor.as_deref())?
        }
    }
    crate::fslog::moved(action, from, to);
    Ok(())
}

/// The rename failed only because `from` and `to` are on different drives: Windows
/// `ERROR_NOT_SAME_DEVICE` (17), Unix `EXDEV` (18). (`ErrorKind::CrossesDevices` needs Rust
/// 1.85, above the MSRV.)
pub(super) fn crosses_drives(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(if cfg!(windows) { 17 } else { 18 })
}

/// The cross-drive half of [`move_path_as`]: copy `from` to `to` through a temporary sibling,
/// then remove `from`. If `from` can't be removed, the copy at `to` (Silo's own, just made) is
/// removed again, so a failure leaves exactly the original. A folder is first renamed aside on
/// its own drive, so a delete that stops halfway never leaves a half-deleted mod under its
/// real name. `anchor` is the highest folder whose entries the move changed (see
/// [`sync_placement`]).
pub(super) fn copy_then_remove(
    from: &Path,
    to: &Path,
    anchor: Option<&Path>,
) -> std::io::Result<()> {
    let tmp = tmp_sibling(to);
    let is_dir = from.is_dir();
    // Every copied file is flushed before the temp is placed, and the placement before `from`
    // is removed: a power cut can't leave the delete on disk without the copy. The temp is
    // made exclusively (a folder with `create_dir`, a file by `copy_file_synced`).
    let copied = if is_dir {
        std::fs::create_dir(&tmp).and_then(|()| copy_dir_all(from, &tmp))
    } else {
        copy_file_synced(from, &tmp)
    };
    if let Err(e) = copied.and_then(|()| place_new(&tmp, to)) {
        discard_copy(&tmp, from);
        return Err(e);
    }
    // Undo a placement only while `from` still holds every byte of it.
    let undo = |e: std::io::Error| {
        discard_copy(to, from);
        e
    };
    sync_placement(to, anchor).map_err(undo)?;
    // The original goes only once the placed copy holds the same bytes.
    if !crate::scratch::held_by(from, to) {
        return Err(std::io::Error::other(format!(
            "the copy at {} doesn't match {}, so Silo kept both",
            to.display(),
            from.display()
        )));
    }
    if !is_dir {
        return std::fs::remove_file(from).map_err(undo);
    }
    let aside = aside_sibling(from);
    crate::paths::rename_no_replace(from, &aside).map_err(undo)?;
    if let Err(e) = std::fs::remove_dir_all(&aside) {
        // The mod is in place at `to` and gone from its old name; only the hidden leftover
        // remains (scans skip it, and the next archive move clears it).
        log::warn!("moved, but couldn't remove {}: {e}", aside.display());
    }
    Ok(())
}

/// A new `.{name}.<token>.silo-move-old` beside `p`: where a folder waits to be deleted after a
/// cross-drive copy (a Silo temp name, so scans skip it). A name of its own each time, so
/// nothing is removed at it first.
pub(super) fn aside_sibling(p: &Path) -> PathBuf {
    let name = p
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "x".into());
    crate::scratch::unique_beside(&p.with_file_name(format!(".{name}")), ".silo-move-old")
}

/// Copy `src` to a **new** file `dst`: refuses (AlreadyExists) rather than replacing an
/// existing file. The copy is built and flushed under a Silo temp sibling
/// (`.{name}.silo-tmp`, which scans skip) and only then placed at `dst`, so a copy that stops
/// part-way (a full disk, an archive file that won't read, Silo closed mid-copy) never leaves a
/// truncated mod under its real name. A failed copy removes its temp.
pub(super) fn copy_new(src: &Path, dst: &Path) -> std::io::Result<()> {
    copy_new_with(src, dst, stream_new_synced)
}

/// [`copy_new`] with the write into the temp injected, so a copy cut off part-way is testable.
pub(super) fn copy_new_with(
    src: &Path,
    dst: &Path,
    write: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if dst.symlink_metadata().is_ok() {
        return Err(std::io::ErrorKind::AlreadyExists.into());
    }
    let tmp = tmp_sibling(dst);
    if let Err(e) = write(src, &tmp).and_then(|()| place_new(&tmp, dst)) {
        discard_copy(&tmp, src);
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
pub(super) fn copy_file_synced(src: &Path, dst: &Path) -> std::io::Result<()> {
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
pub(crate) fn sync_placement(to: &Path, anchor: Option<&Path>) -> std::io::Result<()> {
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
pub(super) fn placement_dirs<'a>(to: &'a Path, anchor: Option<&Path>) -> Vec<&'a Path> {
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
/// cloud-synced Documents folder), and for a folder, it is renamed with
/// `paths::rename_no_replace`, which fails rather than replace `dst`. Called under the library
/// lock, which keeps Silo's own writes out.
pub(super) fn place_new(tmp: &Path, dst: &Path) -> std::io::Result<()> {
    use std::io::ErrorKind;
    let taken = || std::io::Error::from(ErrorKind::AlreadyExists);
    if !tmp.symlink_metadata()?.is_dir() {
        match std::fs::hard_link(tmp, dst) {
            Ok(()) => {
                // A leftover second name (it couldn't be dropped) holds no extra data.
                let _ = std::fs::remove_file(tmp);
                return Ok(());
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => return Err(taken()),
            Err(_) => {} // no hardlinks here: no-replace rename below
        }
    }
    let mut tries = 0;
    loop {
        match crate::paths::rename_no_replace(tmp, dst) {
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
pub(crate) fn sync_dir(dir: &Path) -> std::io::Result<()> {
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

/// Copy the folder `src` to `dst`, every file flushed to disk (see [`copy_file_synced`]).
pub(super) fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
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
