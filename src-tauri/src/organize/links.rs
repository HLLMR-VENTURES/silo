//! Projection links: making a mod's mods-folder entry (hardlink, symlink, junction or marked
//! copy), the link-strategy policy, and removing projections.

use super::*;

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
/// case never probes (or writes) anything. `can_only_copy` (asked by an archive move before it
/// re-links an active mod's hardlink) also probes a hardlink, once.
pub(crate) struct LinkPolicy<'a> {
    layout: &'a Layout,
    cached: Option<bool>,
    cross_volume: Option<bool>,
    /// A probe hardlink from the mods folder into the archive worked (`hardlink_known`).
    hardlinks: Option<bool>,
}

impl<'a> LinkPolicy<'a> {
    pub(crate) fn new(layout: &'a Layout) -> Self {
        LinkPolicy {
            layout,
            cached: None,
            cross_volume: None,
            hardlinks: None,
        }
    }

    /// A policy that decided the archive is on another drive with no symlinks (copies only).
    #[cfg(test)]
    pub(crate) fn copy_only_for_tests(layout: &'a Layout) -> Self {
        Self::decided_for_tests(layout, true, false, false)
    }

    /// A policy with every answer decided up front: the archive is on another drive
    /// (`cross_volume`), symlinks are allowed, a hardlink is known to work.
    #[cfg(test)]
    pub(crate) fn decided_for_tests(
        layout: &'a Layout,
        cross_volume: bool,
        symlinks: bool,
        hardlinks: bool,
    ) -> Self {
        LinkPolicy {
            layout,
            cached: Some(symlinks),
            cross_volume: Some(cross_volume),
            hardlinks: Some(hardlinks),
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

    /// A hardlink from the mods folder into this archive is known to work (`hardlink_known`).
    /// Asked at most once; probes only when the archive isn't on another drive.
    fn hardlinks_work(&mut self) -> bool {
        if let Some(h) = self.hardlinks {
            return h;
        }
        let layout = self.layout;
        let h = hardlink_known(self.cross_volume(), || {
            hardlinks_between(&layout.mods, &layout.archive)
        });
        self.hardlinks = Some(h);
        h
    }

    /// A new zip link into this archive could end up a full copy: symlinks aren't allowed and
    /// a hardlink isn't known to work — the archive is on another drive, the mods folder
    /// rejects hardlinks (a cloud-synced folder), or Silo can't tell (a drive it can't
    /// identify, where no probe could be made).
    pub(crate) fn can_only_copy(&mut self) -> bool {
        !self.allow_symlink() && !self.hardlinks_work()
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

/// A folder mod's link from the mods folder into this archive might end up a full copy
/// (`make_link`'s dir branch): always inside a Mac App Store container; otherwise unless a probe
/// folder link, made the way `make_link` makes one, actually worked (`dir_links_between`).
pub(crate) fn dir_link_may_copy(layout: &Layout) -> bool {
    in_mac_app_container(&layout.mods)
        || dir_links_between(&layout.mods, &layout.archive) != Some(true)
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
pub(super) fn link_zip(
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

/// Dir projection: `symlink` (a symlink or junction, see [`link_dir_native`]), or a marked copy
/// when the filesystem refuses links (exFAT/FAT drives, a network archive). Never falls back
/// when `link` already exists — copying into a folder that's already there would merge into it
/// and stamp it as ours.
pub(super) fn link_dir_or_copy(
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
