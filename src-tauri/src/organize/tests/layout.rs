//! Tests for `Layout` and archive naming (`organize/mod.rs`).

use super::*;

#[test]
fn archive_is_default_compares_canonically() {
    let mods = std::env::temp_dir().join(format!("silo_isdef_{}", std::process::id()));
    assert!(Layout::default_for(&mods).archive_is_default());
    let custom = Layout {
        mods: mods.clone(),
        archive: mods.with_file_name("silo_isdef_elsewhere"),
    };
    assert!(!custom.archive_is_default());
}

#[cfg(windows)]
#[test]
fn archive_is_default_ignores_case_on_windows() {
    let mods = std::env::temp_dir().join(format!("silo_isdefcase_{}", std::process::id()));
    let shouty = PathBuf::from(mods.to_string_lossy().to_uppercase()).join("ARCHIVE");
    let layout = Layout {
        mods: mods.clone(),
        archive: shouty,
    };
    assert!(layout.archive_is_default());
}

#[test]
fn layout_new_accepts_the_default_and_a_sibling() {
    let base = std::env::temp_dir().join(format!("silo_lnew_ok_{}", std::process::id()));
    let mods = base.join("mods");
    let def = Layout::new(mods.clone(), mods.join(ARCHIVE)).unwrap();
    assert!(def.archive_is_default());
    let sib = Layout::new(mods.clone(), base.join("hdd")).unwrap();
    assert_eq!(sib.archive, base.join("hdd"));
    assert!(!sib.archive_is_default());
}

#[test]
fn layout_new_rejects_unsafe_archives() {
    let base = std::env::temp_dir().join(format!("silo_lnew_bad_{}", std::process::id()));
    let mods = base.join("mods");
    assert!(
        Layout::new(PathBuf::from("mods"), base.join("hdd")).is_err(),
        "relative mods"
    );
    assert!(
        Layout::new(mods.clone(), PathBuf::from("hdd")).is_err(),
        "relative archive"
    );
    assert!(
        Layout::new(mods.clone(), mods.clone()).is_err(),
        "same folder"
    );
    assert!(
        Layout::new(mods.clone(), base.clone()).is_err(),
        "archive containing the mods folder"
    );
    assert!(
        Layout::new(mods.clone(), mods.join("stash")).is_err(),
        "non-default archive inside the mods folder"
    );
    assert!(
        Layout::new(mods.clone(), mods.join("archive").join("deeper")).is_err(),
        "inside the default archive is still inside the mods folder"
    );
}

#[test]
fn a_category_never_resolves_to_or_escapes_the_archive() {
    let arch = std::env::temp_dir().join(format!("silo_catdots_{}", std::process::id()));
    // Names that are empty once Windows drops trailing dots/spaces would name the archive.
    for cat in ["..", ".", "", "  ", " .. ", "...", ". ."] {
        let dir = archive_dir(&arch, cat);
        assert_eq!(dir, arch.join("_"), "category {cat:?}");
        assert!(dir.starts_with(&arch) && dir != arch, "category {cat:?}");
    }
    // Ordinary names (and dotted ones that aren't empty when collapsed) are unchanged, so
    // existing archive folders keep their names.
    assert_eq!(archive_dir(&arch, "Vehicles"), arch.join("Vehicles"));
    assert_eq!(archive_dir(&arch, "a/b"), arch.join("a_b"));
    assert_eq!(archive_dir(&arch, "v1.0"), arch.join("v1.0"));
    assert_eq!(archive_dir(&arch, "a. "), arch.join("a. "));
}
