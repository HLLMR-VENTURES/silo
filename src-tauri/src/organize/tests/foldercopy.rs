//! Tests for `organize/foldercopy.rs`.

use super::*;

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
                    .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(10))
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
        rep.notices[0].starts_with("FS25_Dir had changed since Silo copied it, so it was kept in ")
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
        let err = retire_folder_copy_with(&flat, &src, &base, move |_: &Path, _: &Path| Err(fail))
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
