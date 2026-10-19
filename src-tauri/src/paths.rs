//! Path-safety helpers for the Rust ↔ webview boundary. The webview is our own app, but
//! we treat it as potentially compromised (a hostile catalog string, a future bug), so
//! any file name or path that originated in the frontend/catalog is validated here before
//! it can drive a filesystem write. Cheap, and it turns "write anywhere" into "write only
//! where we intend".

use std::io::Write;
use std::path::{Component, Path, PathBuf};

/// Accept a value only as a plain file name: exactly one normal path component, no
/// separators, no `.`/`..`, not absolute, no NUL. Rejects `../escape`, `a/b`, `C:\x`, etc.
/// so `root.join(name)` provably stays directly under `root`. Also refuses names Windows
/// treats specially: a colon (an NTFS alternate stream, `mod.zip:x`), a trailing dot or space
/// (silently stripped), and reserved device names (`CON`, `nul.zip`, `COM1`).
pub fn safe_file_name(name: &str) -> Result<&str, String> {
    let ok = !name.is_empty()
        && !name.contains(['/', '\\', '\0'])
        && name != "."
        && name != ".."
        && !Path::new(name).is_absolute()
        && Path::new(name).components().count() == 1
        && matches!(
            Path::new(name).components().next(),
            Some(Component::Normal(_))
        )
        && !name.contains(':')
        && !name.ends_with(['.', ' '])
        && !is_device_name(name);
    if ok {
        Ok(name)
    } else {
        Err(format!("refusing an unsafe file name: {name:?}"))
    }
}

/// Windows reserves these names whatever the extension (`nul.zip` is the NUL device), so a
/// mod file can never be called that on any system Silo syncs with. Microsoft's list includes
/// `CONIN$`, `CONOUT$`, and `COM`/`LPT` followed by a digit or a superscript ¹ ² ³.
fn is_device_name(name: &str) -> bool {
    // ASCII-only uppercasing leaves the superscript digits untouched.
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) {
        return true;
    }
    // `get` keeps the split on a char boundary, so a multi-byte name can't panic here.
    matches!(
        (stem.get(..3), stem.get(3..)),
        (
            Some("COM" | "LPT"),
            Some(
                "0" | "1"
                    | "2"
                    | "3"
                    | "4"
                    | "5"
                    | "6"
                    | "7"
                    | "8"
                    | "9"
                    | "\u{b9}"
                    | "\u{b2}"
                    | "\u{b3}"
            )
        )
    )
}

/// True when `target` (after resolving symlinks/`..`) lives inside `root`. Both must
/// exist to canonicalize; for a not-yet-created file, check its parent directory.
pub fn is_within(root: &Path, target: &Path) -> bool {
    match (root.canonicalize(), target.canonicalize()) {
        (Ok(r), Ok(t)) => t.starts_with(r),
        _ => false,
    }
}

/// Reject a path that contains a `..` component — a cheap guard against traversal on a
/// frontend-supplied path we're about to write to, without assuming a specific root
/// (so it doesn't break users with a custom mods folder).
pub fn no_traversal(path: &Path) -> Result<(), String> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        Err("refusing a path that contains '..'".to_string())
    } else {
        Ok(())
    }
}

/// Guard a destination we're about to WRITE: its parent directory must resolve inside one
/// of `allowed_roots`. Prevents a hostile path from redirecting a write outside the mod
/// folder(s). Returns Ok(()) when the parent is under an allowed root.
pub fn ensure_write_under(allowed_roots: &[std::path::PathBuf], dest: &Path) -> Result<(), String> {
    let parent = dest
        .parent()
        .ok_or_else(|| "destination has no parent directory".to_string())?;
    if allowed_roots.iter().any(|r| is_within(r, parent)) {
        Ok(())
    } else {
        Err("refusing to write outside the mods folder".to_string())
    }
}

/// Safely overwrite a config / mod-settings XML file. Same authority model as the organizer:
/// the target must be a plain `.xml` file that already exists and whose parent resolves inside
/// one of `allowed_roots` (the FS25 user dir). The current file is backed up to `<name>.bak`
/// (older versions rotate and the original is kept; see `back_up_config`) and that backup MUST
/// succeed — Silo never overwrites without a recoverable copy. The new bytes go to a temp
/// sibling, are flushed, then atomically renamed into place; if the swap fails the original is
/// restored from the backup, so a disk/permission error can't leave a half-written config.
/// Closes the "wrote outside the intended folder" and "overwrote despite a failed backup" gaps
/// a frontend-supplied path otherwise allowed.
pub fn guarded_xml_write(
    allowed_roots: &[PathBuf],
    dest: &Path,
    contents: &str,
) -> Result<(), String> {
    no_traversal(dest)?;
    let is_xml = dest
        .extension()
        .map(|e| e.eq_ignore_ascii_case("xml"))
        .unwrap_or(false);
    if !is_xml {
        return Err("refusing to write a config file that isn't .xml".to_string());
    }
    // These commands edit an existing file; require it so a backup is always possible.
    if !dest.is_file() {
        return Err("config file not found".to_string());
    }
    ensure_write_under(allowed_roots, dest)?;

    // Backup REQUIRED — abort before touching the original if we can't preserve a copy.
    back_up_config(dest)?;

    // Write to a temp sibling, flush, then atomically replace (rename replaces on all OSes).
    let tmp = dest.with_extension("xml.silo-tmp");
    let write = (|| -> Result<(), String> {
        let mut f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        f.write_all(contents.as_bytes())
            .map_err(|e| e.to_string())?;
        f.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, dest).map_err(|e| e.to_string())
    })();
    if let Err(e) = write {
        // The rename is the only step that touches `dest`, and it either replaces it whole or
        // not at all, so the original is still as it was. (Copying the backup over it here
        // would rewrite a good file in place, and could cut it short on a full disk.)
        let _ = std::fs::remove_file(&tmp);
        return Err(format!(
            "config write failed, so the file was left as it was: {e}"
        ));
    }
    crate::fslog::changed("write-config", dest);
    Ok(())
}

/// Previous versions kept beside an edited config: `.bak`, `.bak.2`, `.bak.3`.
const CONFIG_BACKUPS: usize = 3;

/// `dest`'s file name with `suffix` appended, built on the raw `OsString` so a name that isn't
/// valid UTF-8 (possible on Linux) keeps its exact bytes.
fn with_suffix(dest: &Path, suffix: &str) -> PathBuf {
    let mut name = dest.file_name().unwrap_or_default().to_os_string();
    name.push(suffix);
    dest.with_file_name(name)
}

fn backup_slot(dest: &Path, n: usize) -> PathBuf {
    if n == 1 {
        with_suffix(dest, ".bak")
    } else {
        with_suffix(dest, &format!(".bak.{n}"))
    }
}

/// Back up `dest` before it is replaced. The first time, keep the original as
/// `<name>.silo-original.bak` (an existing single `.bak` from an older Silo is older, so it's
/// used instead); that file is never replaced. Then shift `.bak` → `.bak.2` → `.bak.3` (the
/// oldest drops) and copy `dest` to `.bak`. Any failure aborts before `dest` is touched.
fn back_up_config(dest: &Path) -> Result<(), String> {
    let abort = |e: std::io::Error| {
        format!("Silo couldn't back up the file first, so it changed nothing: {e}")
    };
    let original = with_suffix(dest, ".silo-original.bak");
    if !original.exists() {
        let source = if backup_slot(dest, 1).is_file() {
            backup_slot(dest, 1)
        } else {
            dest.to_path_buf()
        };
        keep_original(&source, &original).map_err(abort)?;
    }
    let oldest = backup_slot(dest, CONFIG_BACKUPS);
    if oldest.exists() {
        std::fs::remove_file(&oldest).map_err(abort)?;
    }
    for n in (1..CONFIG_BACKUPS).rev() {
        let from = backup_slot(dest, n);
        if from.exists() {
            std::fs::rename(&from, backup_slot(dest, n + 1)).map_err(abort)?;
        }
    }
    std::fs::copy(dest, backup_slot(dest, 1)).map_err(abort)?;
    crate::fslog::moved("backup-config", dest, &backup_slot(dest, 1));
    Ok(())
}

/// Copy `source` to `original` so that `original` only ever appears complete, even if Silo
/// stops mid-copy: copy into `<original>.tmp`, flush it, then hard-link it into place (a link
/// never replaces an existing file) and drop the temp. On a filesystem without hard links, fall
/// back to a create-new copy. A partial temp from an earlier crash is removed first.
fn keep_original(source: &Path, original: &Path) -> std::io::Result<()> {
    use std::io::ErrorKind;
    let tmp = with_suffix(original, ".tmp");
    match std::fs::remove_file(&tmp) {
        Err(e) if e.kind() != ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    copy_new(source, &tmp)?;
    let placed = match std::fs::hard_link(&tmp, original) {
        Ok(()) => Ok(true),
        // Another edit kept the original first; it is never replaced.
        Err(e) if e.kind() == ErrorKind::AlreadyExists => Ok(false),
        Err(_) => copy_new(source, original).map(|()| true),
    };
    let _ = std::fs::remove_file(&tmp);
    if placed? {
        crate::fslog::moved("keep-original", source, original);
    }
    Ok(())
}

/// Copy `source` to a new file `to` (never replacing one) and flush it. A partial copy is
/// removed again on failure.
fn copy_new(source: &Path, to: &Path) -> std::io::Result<()> {
    let mut out = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(to)?;
    let copied = std::fs::File::open(source)
        .and_then(|mut src| std::io::copy(&mut src, &mut out))
        .and_then(|_| out.sync_all());
    if let Err(e) = copied {
        drop(out);
        let _ = std::fs::remove_file(to);
        return Err(e);
    }
    Ok(())
}

/// Require that `path` has one of `allowed` extensions (case-insensitive). Defence-in-depth
/// against a compromised webview handing a command a path of an unexpected type.
pub fn require_ext(path: &Path, allowed: &[&str]) -> Result<(), String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext {
        Some(e) if allowed.iter().any(|a| a.eq_ignore_ascii_case(&e)) => Ok(()),
        _ => Err(format!(
            "refusing a path whose type isn't one of {allowed:?}: {path:?}"
        )),
    }
}

/// Validate a user-chosen destination we're about to WRITE (a Save-dialog result that lives
/// outside the mods folder — a loadout, mod-set manifest, or diagnostics report). Enforce: no
/// `..`, an expected extension, and a real existing parent directory — so a compromised webview
/// can only write a known file type into a real folder, never an arbitrary path. The content
/// these commands write is app-generated (JSON / a report), not attacker-controlled.
pub fn safe_outbound(dest: &Path, allowed_ext: &[&str]) -> Result<(), String> {
    no_traversal(dest)?;
    require_ext(dest, allowed_ext)?;
    match dest.parent() {
        Some(p) if p.is_dir() => Ok(()),
        _ => Err("destination folder does not exist".to_string()),
    }
}

/// Validate a user-chosen file we're about to READ (an Open-dialog result). Enforce: no `..`,
/// an expected extension, and that it's an existing regular file — turning "read any path the
/// frontend hands us" into "read an existing file of the expected type".
pub fn safe_inbound(path: &Path, allowed_ext: &[&str]) -> Result<(), String> {
    no_traversal(path)?;
    require_ext(path, allowed_ext)?;
    if path.is_file() {
        Ok(())
    } else {
        Err("file not found".to_string())
    }
}

/// The largest shared file Silo reads (a `.silo` set, a `.silomp` Multiplayer check file, or a
/// collection `.json`). Real ones are a few hundred KB at most; this keeps a huge or hostile file
/// from being read into memory whole.
pub const SHARED_FILE_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// Read a user-picked text file (checked by [`safe_inbound`]) of at most `max_bytes`. Reads one
/// byte past the limit instead of trusting the file's size, which can change between the check
/// and the read.
pub fn read_inbound_text(
    path: &Path,
    allowed_ext: &[&str],
    max_bytes: u64,
) -> Result<String, String> {
    use std::io::Read;
    safe_inbound(path, allowed_ext)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(max_bytes.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > max_bytes {
        return Err("That file is too large to be a Silo file.".to_string());
    }
    Ok(text)
}

/// Read an FS25 config/settings XML confined to the game user dir — the read-side mirror of
/// [`guarded_xml_write`]. The target must be an existing `.xml` whose parent resolves inside one
/// of `allowed_roots`, so a config-reading command can't be turned into an arbitrary-file read.
pub fn guarded_xml_read(allowed_roots: &[PathBuf], path: &Path) -> Result<String, String> {
    no_traversal(path)?;
    require_ext(path, &["xml"])?;
    if !path.is_file() {
        return Err("config file not found".to_string());
    }
    ensure_write_under(allowed_roots, path)?;
    std::fs::read_to_string(path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_plain_names_rejects_traversal() {
        assert!(safe_file_name("FS25_Foo.zip").is_ok());
        assert!(safe_file_name("a mod (v1).zip").is_ok());
        for bad in [
            "",
            ".",
            "..",
            "../x",
            "a/b",
            "a\\b",
            "/etc/passwd",
            "C:\\Windows\\x",
            "sub/FS25_Foo.zip",
            "with\0nul",
        ] {
            assert!(safe_file_name(bad).is_err(), "should reject {bad:?}");
        }
    }

    #[test]
    fn rejects_stream_and_device_names() {
        for bad in [
            "mod.zip:evil",
            "a:b",
            "CON",
            "con.zip",
            "Nul.txt",
            "nul.tar.gz",
            "AUX",
            "PRN.zip",
            "COM1",
            "com9.zip",
            "COM0.zip",
            "LPT1.xml",
            "lpt9",
            "COM\u{b9}.zip",
            "lpt\u{b2}",
            "LPT\u{b3}.xml",
            "CONIN$",
            "conout$.zip",
            "mod.zip.",
            "mod.zip ",
            " ",
        ] {
            assert!(safe_file_name(bad).is_err(), "should reject {bad:?}");
        }
        for ok in [
            "CONSOLE.zip",
            "FS25_con.zip",
            "COM10.zip",
            "FS25_Aux_Tools.zip",
            "LPTX.zip",
            "COM\u{b9}0.zip",
            "CONIN.zip",
            ".silo-archive.json",
            "FS25_Mod v1.2.zip",
        ] {
            assert!(safe_file_name(ok).is_ok(), "should accept {ok:?}");
        }
    }

    #[test]
    fn guarded_xml_write_backs_up_and_confines() {
        let root = std::env::temp_dir().join(format!("silo_cfg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let roots = vec![root.clone()];

        // Happy path: existing .xml under the root is backed up, then replaced atomically.
        let dest = root.join("gameSettings.xml");
        std::fs::write(&dest, "<old/>").unwrap();
        guarded_xml_write(&roots, &dest, "<new/>").unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "<new/>");
        assert_eq!(
            std::fs::read_to_string(dest.with_extension("xml.bak")).unwrap(),
            "<old/>",
            "the previous contents must be backed up"
        );
        assert!(
            !root.join("gameSettings.xml.silo-tmp").exists(),
            "no temp left behind"
        );

        // Outside the allowed root → rejected (a sibling temp dir, definitely not under root).
        let outside = std::env::temp_dir().join(format!("silo_cfg_out_{}.xml", std::process::id()));
        std::fs::write(&outside, "<x/>").unwrap();
        assert!(guarded_xml_write(&roots, &outside, "<hacked/>").is_err());
        assert_eq!(
            std::fs::read_to_string(&outside).unwrap(),
            "<x/>",
            "untouched"
        );

        // Non-.xml under the root → rejected.
        let notxml = root.join("notes.txt");
        std::fs::write(&notxml, "hi").unwrap();
        assert!(guarded_xml_write(&roots, &notxml, "<x/>").is_err());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }

    #[test]
    fn config_edits_keep_the_original_and_three_previous_versions() {
        let root = std::env::temp_dir().join(format!("silo_cfg_rot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let roots = vec![root.clone()];
        let dest = root.join("gameSettings.xml");
        std::fs::write(&dest, "<v0/>").unwrap();
        for v in 1..=5 {
            guarded_xml_write(&roots, &dest, &format!("<v{v}/>")).unwrap();
        }
        let read = |n: &str| std::fs::read_to_string(root.join(n)).unwrap();
        assert_eq!(read("gameSettings.xml"), "<v5/>");
        assert_eq!(
            read("gameSettings.xml.silo-original.bak"),
            "<v0/>",
            "never replaced"
        );
        assert_eq!(read("gameSettings.xml.bak"), "<v4/>");
        assert_eq!(read("gameSettings.xml.bak.2"), "<v3/>");
        assert_eq!(read("gameSettings.xml.bak.3"), "<v2/>");
        assert!(!root.join("gameSettings.xml.bak.4").exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_backup_from_an_older_silo_becomes_the_original() {
        // Silo 0.9 kept a single .bak: the oldest copy there is, so the best "original".
        let root = std::env::temp_dir().join(format!("silo_cfg_old_bak_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dest = root.join("gameSettings.xml");
        std::fs::write(&dest, "<v1/>").unwrap();
        std::fs::write(root.join("gameSettings.xml.bak"), "<v0/>").unwrap();
        guarded_xml_write(std::slice::from_ref(&root), &dest, "<v2/>").unwrap();
        let read = |n: &str| std::fs::read_to_string(root.join(n)).unwrap();
        assert_eq!(read("gameSettings.xml.silo-original.bak"), "<v0/>");
        assert_eq!(read("gameSettings.xml.bak"), "<v1/>");
        assert_eq!(read("gameSettings.xml.bak.2"), "<v0/>");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_stale_partial_original_is_replaced_and_cleaned_up() {
        // A crash mid-copy leaves only the temp file; the original must never be the partial.
        let root = std::env::temp_dir().join(format!("silo_cfg_stale_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let dest = root.join("gameSettings.xml");
        std::fs::write(&dest, "<v0 full='yes'/>").unwrap();
        std::fs::write(root.join("gameSettings.xml.silo-original.bak.tmp"), "<v0").unwrap();
        guarded_xml_write(std::slice::from_ref(&root), &dest, "<v1/>").unwrap();
        let read = |n: &str| std::fs::read_to_string(root.join(n)).unwrap();
        assert_eq!(
            read("gameSettings.xml.silo-original.bak"),
            "<v0 full='yes'/>"
        );
        assert!(!root.join("gameSettings.xml.silo-original.bak.tmp").exists());
        assert_eq!(read("gameSettings.xml"), "<v1/>");
        let _ = std::fs::remove_dir_all(&root);
    }

    // Linux only: macOS (APFS) refuses file names that aren't valid UTF-8.
    #[cfg(target_os = "linux")]
    #[test]
    fn backups_keep_a_non_utf8_file_name() {
        use std::ffi::OsStr;
        use std::os::unix::ffi::OsStrExt;
        let root = std::env::temp_dir().join(format!("silo_cfg_bytes_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let named = |b: &[u8]| root.join(OsStr::from_bytes(b));
        let dest = named(b"caf\xe9.xml");
        std::fs::write(&dest, "<v1/>").unwrap();
        std::fs::write(named(b"caf\xe9.xml.bak"), "<v0/>").unwrap();
        guarded_xml_write(std::slice::from_ref(&root), &dest, "<v2/>").unwrap();
        let read = |b: &[u8]| std::fs::read_to_string(named(b)).unwrap();
        assert_eq!(
            read(b"caf\xe9.xml.silo-original.bak"),
            "<v0/>",
            "older .bak found"
        );
        assert_eq!(read(b"caf\xe9.xml.bak"), "<v1/>");
        assert_eq!(read(b"caf\xe9.xml.bak.2"), "<v0/>");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn require_ext_matches_case_insensitively() {
        assert!(require_ext(Path::new("a.silo"), &["silo"]).is_ok());
        assert!(require_ext(Path::new("a.SILO"), &["silo"]).is_ok());
        assert!(require_ext(Path::new("a.json"), &["silo", "json"]).is_ok());
        assert!(require_ext(Path::new("a.exe"), &["silo", "json"]).is_err());
        assert!(require_ext(Path::new("noext"), &["silo"]).is_err());
    }

    #[test]
    fn inbound_text_is_read_only_up_to_its_limit() {
        let root = std::env::temp_dir().join(format!("silo_in_limit_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let f = root.join("set.silo");
        std::fs::write(&f, "0123456789").unwrap();

        assert_eq!(read_inbound_text(&f, &["silo"], 10).unwrap(), "0123456789");
        let big = read_inbound_text(&f, &["silo"], 9).unwrap_err();
        assert!(big.contains("too large"), "{big}");
        assert!(read_inbound_text(&f, &["json"], 10).is_err(), "wrong type");
        assert!(read_inbound_text(&root.join("missing.silo"), &["silo"], 10).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn safe_outbound_and_inbound_confine_type_and_existence() {
        let root = std::env::temp_dir().join(format!("silo_io_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();

        // outbound: parent must exist + extension must match; the file itself need not exist yet.
        assert!(safe_outbound(&root.join("my.silo"), &["silo"]).is_ok());
        assert!(safe_outbound(&root.join("my.exe"), &["silo"]).is_err());
        assert!(safe_outbound(&root.join("nope").join("my.silo"), &["silo"]).is_err());
        assert!(safe_outbound(Path::new("../escape.silo"), &["silo"]).is_err());

        // inbound: must be an existing file of the right type.
        let f = root.join("in.silomp");
        std::fs::write(&f, "{}").unwrap();
        assert!(safe_inbound(&f, &["silomp", "json"]).is_ok());
        assert!(safe_inbound(&f, &["silo"]).is_err()); // wrong type
        assert!(safe_inbound(&root.join("missing.silomp"), &["silomp"]).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn guarded_xml_read_confines_to_root() {
        let root = std::env::temp_dir().join(format!("silo_rd_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let roots = vec![root.clone()];

        let inside = root.join("gameSettings.xml");
        std::fs::write(&inside, "<x/>").unwrap();
        assert_eq!(guarded_xml_read(&roots, &inside).unwrap(), "<x/>");

        // outside the allowed root → rejected (no arbitrary-file read)
        let outside = std::env::temp_dir().join(format!("silo_rd_out_{}.xml", std::process::id()));
        std::fs::write(&outside, "<secret/>").unwrap();
        assert!(guarded_xml_read(&roots, &outside).is_err());

        // wrong type inside the root → rejected
        let txt = root.join("notes.txt");
        std::fs::write(&txt, "hi").unwrap();
        assert!(guarded_xml_read(&roots, &txt).is_err());

        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_file(&outside);
    }
}
