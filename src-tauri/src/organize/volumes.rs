//! Volumes and link capability: link probes (hardlink / symlink / folder link), their
//! sweeping, and the mount / same-volume checks that decide how to link.

use super::*;

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

/// Whether a folder link from the mods folder into the archive can be made here, the way
/// `make_link` makes one (`link_dir_native`: a symlink, or on Windows a junction, which a FAT
/// or exFAT mods folder or a network archive refuses): an empty probe folder in `archive`
/// linked into `mods`, both removed again. `Some(worked)`, or `None` when no probe could be
/// made. Never creates `mods` or `archive`; a crash's leftovers are swept by
/// `sweep_link_probes`.
pub(crate) fn dir_links_between(mods: &Path, archive: &Path) -> Option<bool> {
    use std::io::ErrorKind::AlreadyExists;
    if !archive.is_dir() || !mods.is_dir() {
        return None;
    }
    let prefix = format!("{SYMLINK_PROBE}-dir");
    for name in probe_names(&prefix).take(PROBE_TRIES) {
        let target = archive.join(&name);
        match std::fs::create_dir(&target) {
            Ok(()) => {}
            Err(e) if e.kind() == AlreadyExists => continue,
            Err(_) => return None,
        }
        let at = mods.join(format!("{name}.link"));
        let r = link_dir_native(&target, &at);
        let taken = matches!(&r, Err(e) if e.kind() == AlreadyExists);
        if !taken {
            let _ = remove_probe_link(&at);
        }
        // Only ever an empty folder: the probe Silo just made.
        let _ = std::fs::remove_dir(&target);
        match r {
            Ok(()) => return Some(true),
            Err(_) if taken => continue,
            Err(_) => return Some(false),
        }
    }
    None
}

/// Remove a probe link: a file link, or a folder link (a dir symlink or junction, which Windows
/// removes as a folder — `remove_dir` never removes a folder with anything in it).
fn remove_probe_link(at: &Path) -> std::io::Result<()> {
    std::fs::remove_file(at).or_else(|_| std::fs::remove_dir(at))
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
pub(super) fn create_probe_named(
    dir: &Path,
    names: impl Iterator<Item = String>,
) -> Option<PathBuf> {
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
        if !old {
            continue;
        }
        // A probe folder (`dir_links_between`) only while empty; a link of either kind.
        let removed = if meta.is_dir() {
            std::fs::remove_dir(e.path())
        } else {
            remove_probe_link(&e.path())
        };
        if removed.is_ok() {
            crate::fslog::changed("remove-stale-probe", &e.path());
        }
    }
}

/// Mount point holding `p` (longest matching prefix) among `disks`, with its free space.
/// Compared as normalized keys: on Windows `canonicalize` yields `\\?\C:\…` while mount points
/// are `C:\`, and `Path::starts_with` treats those prefixes as different.
fn mount_in(disks: &sysinfo::Disks, p: &Path) -> Option<(PathBuf, u64)> {
    let c = std::fs::canonicalize(p).ok()?;
    let key = crate::modsroot::normalize_key(&c.to_string_lossy(), cfg!(windows));
    let (mounts, list) = listed_mounts(disks);
    let i = pick_mount(Path::new(&key), device_of(&c), &mounts)?;
    let d = &list[i];
    Some((d.mount_point().to_path_buf(), d.available_space()))
}

/// The disk list's mount points (normalized keys) with their device ids, and the disks.
fn listed_mounts(disks: &sysinfo::Disks) -> (Vec<(PathBuf, Option<u64>)>, &[sysinfo::Disk]) {
    let list = disks.list();
    let mounts = list
        .iter()
        .map(|d| {
            let mount =
                crate::modsroot::normalize_key(&d.mount_point().to_string_lossy(), cfg!(windows));
            (PathBuf::from(mount), device_of(d.mount_point()))
        })
        .collect();
    (mounts, list)
}

/// The filesystem's device id for `p` (Unix); `None` elsewhere or when it can't be read.
fn device_of(p: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(p).ok().map(|m| m.dev())
    }
    #[cfg(not(unix))]
    {
        let _ = p;
        None
    }
}

/// Pure: which of `mounts` (normalized mount point, device id) holds the path `key` (normalized,
/// device id `dev`). With a device id (Unix) the filesystem decides: the first listed mount of
/// that device, so every path on one filesystem names the same mount (a bind-mounted alias, or
/// macOS keeping `/Users` on the data volume listed at `/System/Volumes/Data`, not at `/`), and
/// none when the device isn't listed — an unlisted mount below a listed one (a network share at
/// `/mnt/share` under `/`) is unknown, never its parent. Without device ids (Windows), the
/// longest mount point that contains the path.
pub(super) fn pick_mount(
    key: &Path,
    dev: Option<u64>,
    mounts: &[(PathBuf, Option<u64>)],
) -> Option<usize> {
    match dev {
        Some(dev) => mounts.iter().position(|(_, d)| *d == Some(dev)),
        None => mounts
            .iter()
            .enumerate()
            .filter(|(_, (m, _))| key.starts_with(m))
            .max_by_key(|(_, (m, _))| m.components().count())
            .map(|(i, _)| i),
    }
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

/// Pure: a hardlink from the mods folder into the archive is known to work — the archive
/// isn't on another drive (`cross_volume`) and a probe hardlink (`probe`, asked only then)
/// worked. A probe that couldn't run (`None`) isn't knowing: on a drive Silo can't identify
/// it counts as "might copy", never as "will link".
pub(crate) fn hardlink_known(cross_volume: bool, probe: impl FnOnce() -> Option<bool>) -> bool {
    !cross_volume && probe() == Some(true)
}

/// What the disk list says about `a` and `b` for linking (one enumeration for both):
/// `Some(same)` when it can tell, `None` when it can't (a network share, …) — see
/// `link_relation`.
pub(crate) fn volume_relation(a: &Path, b: &Path) -> Option<bool> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let (mounts, _) = listed_mounts(&disks);
    let at = |p: &Path| {
        let c = std::fs::canonicalize(p).ok()?;
        let key = crate::modsroot::normalize_key(&c.to_string_lossy(), cfg!(windows));
        Some((PathBuf::from(key), device_of(&c)))
    };
    let ((ka, da), (kb, db)) = (at(a)?, at(b)?);
    link_relation((&ka, da), (&kb, db), &mounts)
}

/// Pure: whether two paths (normalized key, device id) are one volume for linking. Different
/// device ids: different volumes. The same device id: one volume only when both sit under the
/// same listed mount point of that device — a hardlink can't cross mount points even within one
/// filesystem (a bind-mounted alias fails with `EXDEV`), so otherwise it's unknown and gets
/// probed. Without device ids (Windows): the longest listed mount point of each, when both are
/// listed.
pub(super) fn link_relation(
    a: (&Path, Option<u64>),
    b: (&Path, Option<u64>),
    mounts: &[(PathBuf, Option<u64>)],
) -> Option<bool> {
    let longest = |key: &Path, dev: Option<u64>| {
        mounts
            .iter()
            .enumerate()
            .filter(|(_, (m, d))| key.starts_with(m) && (dev.is_none() || *d == dev))
            .max_by_key(|(_, (m, _))| m.components().count())
            .map(|(i, _)| i)
    };
    match (a.1, b.1) {
        (Some(da), Some(db)) if da != db => Some(false),
        (Some(_), Some(_)) => match (longest(a.0, a.1), longest(b.0, b.1)) {
            (Some(ia), Some(ib)) if ia == ib => Some(true),
            _ => None,
        },
        _ => match (longest(a.0, None), longest(b.0, None)) {
            (Some(ia), Some(ib)) => Some(ia == ib),
            _ => None,
        },
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
pub(super) fn probe_hardlink(
    mods: &Path,
    archive: &Path,
    link: impl Fn(&Path, &Path) -> std::io::Result<()>,
) -> Option<bool> {
    probe_link(mods, archive, HARDLINK_PROBE, link)
}
