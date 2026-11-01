//! Tests for `organize/volumes.rs`.

use super::*;

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
fn mount_of_finds_the_volume_of_an_existing_folder() {
    let base = std::env::temp_dir().join(format!("silo_mount_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("sub")).unwrap();
    // Regression: Windows' canonical `\\?\C:\…` form must still match the `C:\` mount.
    let (mount, _free) = mount_of(&base).expect("the temp dir sits on a known volume");
    assert!(
        crate::modsroot::canonical_key(&base).starts_with(&crate::modsroot::normalize_key(
            &mount.to_string_lossy(),
            cfg!(windows)
        ))
    );
    assert_eq!(volume_relation(&base, &base.join("sub")), Some(true));
    let _ = std::fs::remove_dir_all(&base);
}

/// The longest listed mount point holds a path only when it is the same filesystem: an
/// unlisted share mounted below `/` is unknown, a path on a filesystem listed elsewhere
/// (macOS's data volume) is that one, and without device ids (Windows) the longest wins
/// (#188).
#[test]
fn pick_mount_checks_the_device_not_just_the_prefix() {
    let m = |p: &str, d: Option<u64>| (PathBuf::from(p), d);
    let mounts = vec![
        m("/", Some(1)),
        m("/home", Some(2)),
        m("/System/Volumes/Data", Some(3)),
    ];
    let pick = |k: &str, d| pick_mount(Path::new(k), d, &mounts);
    assert_eq!(pick("/home/me/mods", Some(2)), Some(1));
    assert_eq!(pick("/opt/x", Some(1)), Some(0));
    assert_eq!(pick("/mnt/share/archive", Some(9)), None, "unlisted share");
    assert_eq!(
        pick("/Users/me/mods", Some(3)),
        Some(2),
        "firmlinked data volume"
    );
    assert_eq!(
        pick("/home/me", None),
        Some(1),
        "no device ids: longest prefix"
    );
    // Two listed mounts of one filesystem: every path on it names the first.
    let aliased = vec![
        m("/", Some(1)),
        m("/data", Some(5)),
        m("/srv/alias", Some(5)),
    ];
    assert_eq!(
        pick_mount(Path::new("/srv/alias/x"), Some(5), &aliased),
        Some(1)
    );
    assert_eq!(pick_mount(Path::new("/data/y"), Some(5), &aliased), Some(1));
    // Windows lists no device ids: drive letters decide.
    let no_dev = vec![m("/c", None), m("/d", None)];
    assert_eq!(pick_mount(Path::new("/d/mods"), None, &no_dev), Some(1));
    assert_eq!(pick_mount(Path::new("/e/mods"), None, &no_dev), None);
}

/// A real folder pair: the probe folder link is made and both probes are removed again; a
/// missing folder is inconclusive and never created (#188).
#[test]
fn a_folder_link_probe_cleans_up_after_itself() {
    let base = std::env::temp_dir().join(format!("silo_dirlink_probe_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let mods = base.join("mods");
    let archive = base.join("lib");
    std::fs::create_dir_all(&mods).unwrap();
    std::fs::create_dir_all(&archive).unwrap();
    // Symlinks on Unix, junctions on NTFS: both work in a temp folder.
    assert_eq!(dir_links_between(&mods, &archive), Some(true));
    assert_eq!(std::fs::read_dir(&mods).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&archive).unwrap().count(), 0);
    assert_eq!(dir_links_between(&mods, &base.join("missing")), None);
    assert!(!base.join("missing").exists());
    let layout = Layout {
        mods: mods.clone(),
        archive: archive.clone(),
    };
    assert!(!dir_link_may_copy(&layout));
    let _ = std::fs::remove_dir_all(&base);
}

/// For linking, one device under two listed mount points (a bind-mounted alias) is unknown —
/// hardlinks can't cross mount points — so it gets probed; different devices are different
/// volumes; one mount point is one volume. Without device ids, the listed mounts decide.
#[test]
fn link_relation_never_assumes_a_hardlink_across_mount_points() {
    let m = |p: &str, d: Option<u64>| (PathBuf::from(p), d);
    let mounts = vec![
        m("/", Some(1)),
        m("/data", Some(5)),
        m("/mnt/library", Some(5)),
    ];
    let rel =
        |a: &str, da, b: &str, db| link_relation((Path::new(a), da), (Path::new(b), db), &mounts);
    assert_eq!(rel("/data/mods", Some(5), "/data/lib", Some(5)), Some(true));
    assert_eq!(rel("/data/mods", Some(5), "/mnt/library/x", Some(5)), None);
    assert_eq!(rel("/data/mods", Some(5), "/home/me", Some(1)), Some(false));
    // An unlisted mount of a device: unknown.
    assert_eq!(rel("/data/mods", Some(5), "/srv/x", Some(5)), None);
    let no_dev = vec![m("/c", None), m("/d", None)];
    let rel = |a: &str, b: &str| link_relation((Path::new(a), None), (Path::new(b), None), &no_dev);
    assert_eq!(rel("/c/mods", "/c/lib"), Some(true));
    assert_eq!(rel("/c/mods", "/d/lib"), Some(false));
    assert_eq!(rel("/c/mods", "/e/lib"), None);
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
/// the hardlink-first path, which copies safely if the link fails. But a hardlink isn't
/// known to work there either, so a new link might be a copy (`can_only_copy`): a move
/// keeps an active mod's hardlink instead of trading it for a possible copy (#188).
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
    assert!(LinkPolicy::new(&layout).can_only_copy());
    let _ = std::fs::remove_dir_all(&base);
}

/// A hardlink counts as working only when a probe made one: never across drives (no probe
/// even runs), never when the probe couldn't run (an unidentified drive), never where the
/// mods folder rejects it (a cloud-synced folder on the same drive) (#188).
#[test]
fn a_hardlink_is_known_to_work_only_when_a_probe_made_one() {
    let asked = std::cell::Cell::new(false);
    let probe = |answer: Option<bool>| {
        let a = &asked;
        move || {
            a.set(true);
            answer
        }
    };
    assert!(hardlink_known(false, probe(Some(true))));
    assert!(!hardlink_known(false, probe(Some(false))), "cloud folder");
    assert!(!hardlink_known(false, probe(None)), "unidentified drive");
    asked.set(false);
    assert!(!hardlink_known(true, probe(Some(true))));
    assert!(!asked.get(), "another drive is never probed");
}

/// A real same-drive pair (a temp folder) where hardlinks work: no copy.
#[test]
fn a_same_drive_archive_where_hardlinks_work_links() {
    let base = std::env::temp_dir().join(format!("silo_hardlinks_work_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let layout = Layout {
        mods: base.join("mods"),
        archive: base.join("mods").join("archive"),
    };
    std::fs::create_dir_all(&layout.archive).unwrap();
    assert_eq!(hardlinks_between(&layout.mods, &layout.archive), Some(true));
    assert!(!LinkPolicy::new(&layout).can_only_copy());
    let left: Vec<_> = std::fs::read_dir(&layout.mods)
        .unwrap()
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        left,
        vec![std::ffi::OsString::from("archive")],
        "probes are removed"
    );
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
    // Probe-like names that aren't probes (#394): data in it, or not a name a probe makes.
    let holds_data = mods.join(".silo-hardlink-probe-1-2.link");
    let notes = mods.join(".silo-hardlink-probe-notes.txt");
    let legacy = mods.join(".silo-hardlink-probe");
    for p in [&old, &old_sym, &fresh] {
        std::fs::write(p, b"").unwrap();
    }
    for p in [&mine, &holds_data, &notes, &legacy] {
        std::fs::write(p, b"x").unwrap();
    }
    let hour_ago = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
    for p in [&old, &old_sym, &mine, &holds_data, &notes, &legacy] {
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
    assert!(holds_data.exists() && notes.exists() && legacy.exists());
    let _ = std::fs::remove_dir_all(&base);
}

/// A crash's folder probe (`dir_links_between`) is swept only while empty; a folder with
/// anything in it, or under another name, is never touched.
#[test]
fn stale_folder_probes_are_swept_only_while_empty() {
    let (base, _mods, archive) = probe_dirs("sweepdir");
    let empty = archive.join(".silo-symlink-probe-dir-1-0");
    let full = archive.join(".silo-symlink-probe-dir-1-1");
    let category = archive.join("Tools");
    for d in [&empty, &full, &category] {
        std::fs::create_dir_all(d).unwrap();
    }
    std::fs::write(full.join("keep.txt"), b"x").unwrap();
    sweep_link_probes(&archive, std::time::Duration::ZERO);
    assert!(!empty.exists());
    assert!(full.join("keep.txt").exists());
    assert!(category.exists());
    let _ = std::fs::remove_dir_all(&base);
}
