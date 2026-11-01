//! Tests for `organize/relink.rs`.

use super::*;
use crate::scratch::Journal;

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
        let r = replace_zip_link_if_linkable_with(&link, &dst, None, || allow, refused, refused);
        assert!(!r.unwrap(), "no link, so nothing replaced");
        assert!(!crate::relocate::same_file(&link, &dst));
        assert_eq!(std::fs::read(&link).unwrap(), b"PK A");
        assert!(!base.join("FS25_A.zip.silo-relink-tmp").exists());
    }
    // A link that works replaces the entry.
    let r = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        None,
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
        None,
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
        None,
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
        None,
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

/// A relink temp is recorded before it's made and forgotten once it's renamed in: nothing is
/// left recorded or at a scratch name, and the fixed name older versions used is never touched.
#[test]
fn a_relink_records_its_temp_and_leaves_nothing() {
    let base = temp_dir_for("relinkjournal");
    let dst = base.join("archive.zip");
    std::fs::write(&dst, b"PK new").unwrap();
    let link = base.join("FS25_A.zip");
    std::fs::write(&link, b"PK old").unwrap();
    let legacy = base.join("FS25_A.zip.silo-relink-tmp");
    std::fs::write(&legacy, b"an older version's").unwrap();
    let journal = crate::scratch::tests::MemJournal::default();
    let seen = std::cell::Cell::new(false);
    let hard = |s: &Path, l: &Path| {
        // Recorded before it's made, under a name of its own.
        let e = journal.entries();
        seen.set(e.len() == 1 && e[0].path == l && e[0].from == s && l != legacy.as_path());
        std::fs::hard_link(s, l)
    };
    let linked = replace_zip_link_if_linkable_with(
        &link,
        &dst,
        Some(&journal),
        || false,
        hard,
        |_, _| panic!("no symlink needed"),
    )
    .unwrap();
    assert!(linked && seen.get());
    assert!(journal.entries().is_empty());
    assert_eq!(std::fs::read(&link).unwrap(), b"PK new");
    assert_eq!(std::fs::read(&legacy).unwrap(), b"an older version's");
    let names: Vec<String> = std::fs::read_dir(&base)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");
    let _ = std::fs::remove_dir_all(&base);
}

/// A link that can't be made leaves no record and removes nothing.
#[test]
fn a_failed_relink_forgets_its_temp_and_removes_nothing() {
    let base = temp_dir_for("relinkfail");
    let dst = base.join("archive.zip");
    std::fs::write(&dst, b"PK new").unwrap();
    let link = base.join("FS25_A.zip");
    std::fs::write(&link, b"PK old").unwrap();
    let journal = crate::scratch::tests::MemJournal::default();
    let taken = |_: &Path, _: &Path| Err(std::io::ErrorKind::AlreadyExists.into());
    let e = replace_zip_link_if_linkable_with(&link, &dst, Some(&journal), || false, taken, taken)
        .unwrap_err();
    assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
    assert!(journal.entries().is_empty());
    assert_eq!(std::fs::read(&link).unwrap(), b"PK old");
    let _ = std::fs::remove_dir_all(&base);
}

/// The folder re-point makes the new link beside the old one and swaps it in: on Unix by one
/// rename (the old link is never missing), elsewhere by removing the old link first. Either way
/// it ends pointing at the new copy with nothing left recorded or at a scratch name.
#[test]
fn a_folder_link_is_repointed_through_a_temp() {
    let base = temp_dir_for("dirrepoint");
    let mods = base.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let layout = Layout::default_for(&mods);
    let old = base.join("old/FS25_D");
    let new = base.join("new/FS25_D");
    for d in [&old, &new] {
        std::fs::create_dir_all(d).unwrap();
        std::fs::write(d.join("modDesc.xml"), b"<modDesc/>").unwrap();
    }
    let link = mods.join("FS25_D");
    make_link(&old, &link, "dir", &mut LinkPolicy::new(&layout)).unwrap();
    let journal = crate::scratch::tests::MemJournal::default();
    let mut policy = LinkPolicy::new(&layout).with_journal(&journal);
    replace_dir_link(&link, &new, &old, &mut policy).unwrap();
    assert_eq!(
        std::fs::canonicalize(&link).unwrap(),
        std::fs::canonicalize(&new).unwrap()
    );
    assert!(link.symlink_metadata().unwrap().file_type().is_symlink());
    assert!(journal.entries().is_empty());
    assert_eq!(
        std::fs::read_dir(&mods).unwrap().count(),
        1,
        "only the link"
    );
    assert!(
        old.join("modDesc.xml").is_file(),
        "the old copy is untouched"
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// A new folder copy that isn't there: the old link is kept as it was.
#[test]
fn a_folder_repoint_to_a_missing_copy_keeps_the_old_link() {
    let base = temp_dir_for("dirrepointgone");
    let mods = base.join("mods");
    std::fs::create_dir_all(&mods).unwrap();
    let layout = Layout::default_for(&mods);
    let old = base.join("old/FS25_D");
    std::fs::create_dir_all(&old).unwrap();
    std::fs::write(old.join("modDesc.xml"), b"<modDesc/>").unwrap();
    let link = mods.join("FS25_D");
    make_link(&old, &link, "dir", &mut LinkPolicy::new(&layout)).unwrap();
    let journal = crate::scratch::tests::MemJournal::default();
    let mut policy = LinkPolicy::new(&layout).with_journal(&journal);
    assert!(replace_dir_link(&link, &base.join("new/FS25_D"), &old, &mut policy).is_err());
    assert!(link.join("modDesc.xml").is_file(), "old link kept");
    assert!(journal.entries().is_empty());
    let _ = std::fs::remove_dir_all(&base);
}

/// An archived zip that is a link (to an identical file elsewhere) isn't a copy Silo can stand
/// behind: the relink pass leaves the real copy in the mods folder alone (#399).
#[test]
fn relink_never_trades_a_copy_for_a_link_to_an_archived_link() {
    let base = std::env::temp_dir().join(format!("silo_relink_archlink_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let cat = base.join(ARCHIVE).join("Vehicles");
    std::fs::create_dir_all(&cat).unwrap();
    let conn = db::open(&base.join("silo.db")).unwrap();
    organize_row(&conn, &base, "FS25_Foo", "Vehicles", true);
    let bytes = b"PK\x03\x04 the archived build";
    let elsewhere = base.join("elsewhere.zip");
    std::fs::write(&elsewhere, bytes).unwrap();
    let src = cat.join("FS25_Foo.zip");
    if symlink_file(&elsewhere, &src).is_err() {
        drop(conn);
        let _ = std::fs::remove_dir_all(&base);
        return; // no file symlinks here (Windows without Developer Mode)
    }
    let flat = base.join("FS25_Foo.zip");
    std::fs::write(&flat, bytes).unwrap();
    for mode in [RelinkMode::Hardlink, RelinkMode::Symlink] {
        let rep = relink_with(&conn, &Layout::default_for(&base), mode, |_, _| {});
        assert_eq!(rep.upgraded, 0, "{mode:?}: {:?}", rep.errors);
        assert_eq!(rep.skipped, 1, "{mode:?}");
        let m = flat.symlink_metadata().unwrap();
        assert!(m.is_file() && !m.file_type().is_symlink(), "{mode:?}");
        assert_eq!(std::fs::read(&flat).unwrap(), bytes);
    }
    drop(conn);
    let _ = std::fs::remove_file(&src);
    let _ = std::fs::remove_dir_all(&base);
}
