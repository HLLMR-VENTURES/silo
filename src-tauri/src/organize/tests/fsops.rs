//! Tests for `organize/fsops.rs`.

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
