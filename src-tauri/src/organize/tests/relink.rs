//! Tests for `organize/relink.rs`.

use super::*;

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

/// The policy says a hardlink works (a probe made one), but this mod's own link fails (a
/// cloud provider that started refusing) and no symlink is allowed, or the symlink fails
/// too: the working entry is kept exactly as it was, never swapped for a copy (#188).
#[test]
fn a_failed_link_keeps_the_working_entry() {
    let base = std::env::temp_dir().join(format!("silo_linkfails_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let link = base.join("FS25_A.zip");
    std::fs::write(&link, b"PK A").unwrap();
    let dst = base.join("new.zip");
    std::fs::write(&dst, b"PK A").unwrap();
    let refused = |_: &Path, _: &Path| Err(std::io::Error::from_raw_os_error(396));
    for allow in [false, true] {
        let r = replace_zip_link_if_linkable_with(&link, &dst, || allow, refused, refused);
        assert!(!r.unwrap(), "no link, so nothing replaced");
        assert!(!crate::relocate::same_file(&link, &dst));
        assert_eq!(std::fs::read(&link).unwrap(), b"PK A");
        assert!(!base.join("FS25_A.zip.silo-relink-tmp").exists());
    }
    // A link that works replaces the entry.
    let r = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        || false,
        |s, l| std::fs::hard_link(s, l),
        refused,
    );
    assert!(r.unwrap());
    assert!(crate::relocate::same_file(&link, &dst));
    let _ = std::fs::remove_dir_all(&base);
}

/// A new copy that vanishes before or while it's linked is an error, never a link to nothing
/// in place of the working entry — with symlinks allowed too (#188).
#[test]
fn a_vanished_new_copy_never_replaces_the_working_entry() {
    let base = std::env::temp_dir().join(format!("silo_vanished_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let link = base.join("FS25_A.zip");
    std::fs::write(&link, b"PK A").unwrap();
    let dst = base.join("new.zip");
    std::fs::write(&dst, b"PK A").unwrap();
    let soft_called = std::cell::Cell::new(false);
    let r = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        || true,
        |_, _| Err(std::io::ErrorKind::NotFound.into()),
        |_, _| {
            soft_called.set(true);
            Ok(())
        },
    );
    assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert!(!soft_called.get(), "never falls through to a symlink");
    // A "link" that reaches no file (the copy went while it was made) is refused too.
    let r = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        || true,
        |_, _| Err(std::io::ErrorKind::Unsupported.into()),
        |_, l| std::fs::create_dir(l),
    );
    assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::NotFound);
    assert_eq!(std::fs::read(&link).unwrap(), b"PK A");
    assert!(!base.join("FS25_A.zip.silo-relink-tmp").exists());
    // Missing from the start: an error, and nothing is attempted.
    std::fs::remove_file(&dst).unwrap();
    let r = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        || true,
        |_, _| panic!("no hardlink to a missing copy"),
        |_, _| panic!("no symlink to a missing copy"),
    );
    assert!(r.is_err());
    assert_eq!(std::fs::read(&link).unwrap(), b"PK A");
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
