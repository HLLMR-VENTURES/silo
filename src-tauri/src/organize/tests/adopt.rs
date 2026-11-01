//! Tests for `organize/adopt.rs`.

use super::*;

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

/// A mods folder at a fresh temp `base` with one managed zip, `FS25_Foo.zip` (Vehicles),
/// archived as `archived`, and `flat` swapped into the mods folder at its name (if any).
/// `backups/` exists already.
fn swap_fixture(tag: &str, archived: &[u8], flat: Option<&[u8]>, active: bool) -> PathBuf {
    let base = temp_dir_for(tag);
    let conn = db::open(&base.join("silo.db")).unwrap();
    let cat = base.join(ARCHIVE).join("Vehicles");
    std::fs::create_dir_all(&cat).unwrap();
    std::fs::create_dir_all(base.join("backups")).unwrap();
    std::fs::write(cat.join("FS25_Foo.zip"), archived).unwrap();
    let row = OrganizedRow {
        tech_name: "FS25_Foo".into(),
        file_name: "FS25_Foo.zip".into(),
        kind: "zip".into(),
        category: "Vehicles".into(),
        subcategory: None,
        active,
    };
    db::upsert_organized(&conn, &crate::modsroot::canonical_key(&base), &row).unwrap();
    if let Some(flat) = flat {
        std::fs::write(base.join("FS25_Foo.zip"), flat).unwrap();
    }
    base
}

fn backups_in(base: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(base.join("backups"))
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .collect()
}

/// Keep `file` from being moved for as long as the guard lives, the way a running FS25 holds
/// a mod it loaded: on Windows a handle that doesn't share delete; on Unix its folder made
/// read-only. `None` where that can't hold (Unix as root).
fn unmovable(file: &Path) -> Option<impl Drop> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        struct Held(#[allow(dead_code)] std::fs::File);
        impl Drop for Held {
            fn drop(&mut self) {}
        }
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1) // FILE_SHARE_READ: readable, not movable
            .open(file)
            .unwrap();
        Some(Held(held))
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        struct ReadOnly(PathBuf);
        impl Drop for ReadOnly {
            fn drop(&mut self) {
                let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
            }
        }
        let dir = file.parent().unwrap().to_path_buf();
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o555)).unwrap();
        let guard = ReadOnly(dir.clone());
        if std::fs::write(dir.join(".probe"), b"").is_ok() {
            let _ = std::fs::remove_file(dir.join(".probe"));
            return None; // root ignores folder permissions
        }
        Some(guard)
    }
}

#[test]
fn a_failed_adopt_puts_the_archived_copy_back() {
    let base = swap_fixture(
        "adopt_rollback",
        b"OLD archived",
        Some(b"NEWER flat"),
        false,
    );
    let conn = db::open(&base.join("silo.db")).unwrap();
    let layout = Layout::default_for(&base);
    let archived = base.join(ARCHIVE).join("Vehicles").join("FS25_Foo.zip");
    let flat = base.join("FS25_Foo.zip");

    let Some(held) = unmovable(&flat) else {
        return;
    };
    let e = adopt_foreign(&conn, &layout, "FS25_Foo.zip").unwrap_err();
    drop(held);
    assert!(e.starts_with("promote:"), "{e}");
    assert!(!e.contains("kept at"), "rolled back: {e}");
    assert_eq!(std::fs::read(&archived).unwrap(), b"OLD archived");
    assert_eq!(std::fs::read(&flat).unwrap(), b"NEWER flat");
    assert!(backups_in(&base).is_empty(), "no backup left behind");

    // Nothing holds it any more: the adopt goes through.
    adopt_foreign(&conn, &layout, "FS25_Foo.zip").unwrap();
    assert_eq!(std::fs::read(&archived).unwrap(), b"NEWER flat");
    drop(conn);
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_failed_restore_puts_the_users_file_back() {
    let base = swap_fixture("restore_rollback", b"MANAGED", Some(b"user's build"), true);
    let conn = db::open(&base.join("silo.db")).unwrap();
    let layout = Layout::default_for(&base);
    let flat = base.join("FS25_Foo.zip");

    let refuse = |_: &Path, _: &Path, _: &str| -> std::io::Result<()> {
        Err(std::io::Error::other("disk full"))
    };
    let e = restore_projection_with(&conn, &layout, "FS25_Foo.zip", refuse).unwrap_err();
    assert_eq!(e, "re-project: disk full");
    assert_eq!(std::fs::read(&flat).unwrap(), b"user's build");
    assert!(backups_in(&base).is_empty(), "no backup left behind");

    // A failed re-project that left something at the name: the user's file is never moved
    // over it, and the error says where it's kept.
    let partial = |_: &Path, link: &Path, _: &str| -> std::io::Result<()> {
        std::fs::write(link, b"PK half")?;
        Err(std::io::Error::other("cut off"))
    };
    let e = restore_projection_with(&conn, &layout, "FS25_Foo.zip", partial).unwrap_err();
    let kept = backups_in(&base);
    assert_eq!(kept.len(), 1);
    assert!(e.contains(&kept[0].display().to_string()), "{e}");
    assert_eq!(std::fs::read(&kept[0]).unwrap(), b"user's build");
    drop(conn);
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
