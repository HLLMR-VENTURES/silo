//! Tests for `organize/foreign.rs`.

use super::*;

#[test]
fn is_silo_projection_distinguishes_ours_from_the_users() {
    let base = std::env::temp_dir().join(format!("silo_proj_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let src = base.join("archived_FS25_Foo.zip");
    std::fs::write(&src, b"PK\x03\x04 canonical bytes").unwrap();
    let link = base.join("FS25_Foo.zip");

    // Our hardlink projection → ours.
    std::fs::hard_link(&src, &link).unwrap();
    assert!(is_silo_projection(&link, &src, "zip"), "hardlink is ours");
    std::fs::remove_file(&link).unwrap();

    // A byte-identical copy (cross-volume fallback) → ours.
    std::fs::copy(&src, &link).unwrap();
    assert!(
        is_silo_projection(&link, &src, "zip"),
        "identical copy is ours"
    );
    std::fs::remove_file(&link).unwrap();

    // The user's own DIFFERENT build at the same name → NOT ours (must be preserved).
    std::fs::write(&link, b"PK\x03\x04 the user's different, longer build").unwrap();
    assert!(
        !is_silo_projection(&link, &src, "zip"),
        "a user's replacement must not read as Silo's"
    );

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn is_silo_projection_dir_requires_marker() {
    let base = std::env::temp_dir().join(format!("silo_projdir_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let src = base.join("archived_dir");
    std::fs::create_dir_all(&src).unwrap();
    let link = base.join("FS25_FooDir");
    std::fs::create_dir_all(&link).unwrap();
    std::fs::write(link.join("modDesc.xml"), b"<x/>").unwrap();

    // A plain user directory at the projected name → NOT ours.
    assert!(!is_silo_projection(&link, &src, "dir"));
    // With Silo's ownership marker → ours.
    std::fs::write(link.join(SILO_MARKER), b"silo\n").unwrap();
    assert!(is_silo_projection(&link, &src, "dir"));

    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn detect_foreign_flags_only_non_projections() {
    let root = std::env::temp_dir().join(format!("silo_foreign_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let conn = db::open(&root.join("silo.db")).unwrap();
    let cat = root.join("archive").join("Vehicles");
    std::fs::create_dir_all(&cat).unwrap();
    let src = cat.join("FS25_Foo.zip");
    std::fs::write(&src, b"the real archived mod bytes").unwrap();
    organize_row(&conn, &root, "FS25_Foo", "Vehicles", false);

    // Nothing in the flat root → nothing flagged.
    assert!(detect_foreign_projections(&conn, &Layout::default_for(&root)).is_empty());

    // A genuine hardlink projection → NOT flagged (it's provably ours).
    let link = root.join("FS25_Foo.zip");
    std::fs::hard_link(&src, &link).unwrap();
    assert!(
        detect_foreign_projections(&conn, &Layout::default_for(&root)).is_empty(),
        "a real hardlink projection must not be flagged"
    );
    std::fs::remove_file(&link).unwrap();

    // A DIFFERENT file at that managed name → flagged as foreign.
    std::fs::write(&link, b"the user's own, different build").unwrap();
    let found = detect_foreign_projections(&conn, &Layout::default_for(&root));
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].tech_name, "FS25_Foo");
    assert_eq!(found[0].file_name, "FS25_Foo.zip");

    // A loose zip with NO archived counterpart is an ordinary unorganized mod, not foreign.
    std::fs::write(root.join("FS25_Unrelated.zip"), b"loose").unwrap();
    assert_eq!(
        detect_foreign_projections(&conn, &Layout::default_for(&root)).len(),
        1
    );

    let _ = std::fs::remove_dir_all(&root);
}

// The manifest decides which archived copy a mods-folder name is compared with: a second
// file of the same name in another category, or an unmanaged file in an archive folder, is
// never anyone's counterpart.
#[test]
fn detect_foreign_compares_with_the_recorded_archive_copy_only() {
    let root = std::env::temp_dir().join(format!("silo_foreign_rows_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let archive = root.join(ARCHIVE);
    for c in ["Vehicles", "Tools", "Misc"] {
        std::fs::create_dir_all(archive.join(c)).unwrap();
    }
    let conn = db::open(&root.join("silo.db")).unwrap();
    let layout = Layout::default_for(&root);
    std::fs::write(archive.join("Vehicles/FS25_Foo.zip"), b"the managed build").unwrap();
    std::fs::write(archive.join("Tools/FS25_Foo.zip"), b"a stray second copy").unwrap();
    organize_row(&conn, &root, "FS25_Foo", "Vehicles", true);

    // A copy of the stray (not the recorded) archive file is foreign.
    let flat = root.join("FS25_Foo.zip");
    std::fs::write(&flat, b"a stray second copy").unwrap();
    let found = detect_foreign_projections(&conn, &layout);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].file_name, "FS25_Foo.zip");

    // A copy of the recorded archive file is Silo's projection.
    std::fs::write(&flat, b"the managed build").unwrap();
    assert!(detect_foreign_projections(&conn, &layout).is_empty());

    // An unmanaged file in an archive folder has no row, so its mods-folder twin isn't
    // flagged (Adopt would only answer "Not a Silo-managed mod").
    std::fs::write(archive.join("Misc/FS25_Loose.zip"), b"unmanaged").unwrap();
    std::fs::write(root.join("FS25_Loose.zip"), b"something else").unwrap();
    assert!(detect_foreign_projections(&conn, &layout).is_empty());

    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_foreign_file_or_link_is_never_mistaken_for_a_symlink_projection() {
    let base = std::env::temp_dir().join(format!("silo_zforeign_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let src = base.join("a.zip");
    std::fs::write(&src, b"PK\x03\x04 a").unwrap();
    let theirs = base.join("theirs.zip");
    std::fs::write(&theirs, b"PK\x03\x04 different").unwrap();
    assert!(!is_silo_projection(&theirs, &src, "zip"));

    #[cfg(unix)]
    {
        // A dangling symlink is not ours.
        let dangling = base.join("dangling.zip");
        std::os::unix::fs::symlink(base.join("gone.zip"), &dangling).unwrap();
        assert!(!is_silo_projection(&dangling, &src, "zip"));
        // A symlink to a different file is not ours — even one with identical bytes.
        let to_theirs = base.join("to_theirs.zip");
        std::os::unix::fs::symlink(&theirs, &to_theirs).unwrap();
        assert!(!is_silo_projection(&to_theirs, &src, "zip"));
        let twin = base.join("twin.zip");
        std::fs::write(&twin, b"PK\x03\x04 a").unwrap();
        let to_twin = base.join("to_twin.zip");
        std::os::unix::fs::symlink(&twin, &to_twin).unwrap();
        assert!(!is_silo_projection(&to_twin, &src, "zip"));
        // A symlink to the archived file is.
        let ours = base.join("ours.zip");
        std::os::unix::fs::symlink(&src, &ours).unwrap();
        assert!(is_silo_projection(&ours, &src, "zip"));
    }
    let _ = std::fs::remove_dir_all(&base);
}
