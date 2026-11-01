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

/// The parts of a path a mod names for one of its own files, or `None` when it could lead
/// outside the mod: empty, absolute, a leading `//`, a drive or network prefix, a `..`, a `:`
/// anywhere (a drive-relative `C:x`, a stream name), a reserved device name, or a part ending in
/// a dot or space (Windows drops those only at the end of a path, so `a /b` and `a ` would name
/// different folders and the check could look at the wrong one). Both slash kinds separate, on
/// every OS; `.` parts are dropped.
fn member_parts(member: &str) -> Option<Vec<String>> {
    let member = member.replace('\\', "/");
    if member.starts_with('/') || member.contains([':', '\0']) {
        return None;
    }
    let mut parts = Vec::new();
    for c in Path::new(&member).components() {
        match c {
            Component::Normal(part) => {
                let part = part.to_str()?;
                if part.ends_with(['.', ' ']) || is_device_name(part) {
                    return None;
                }
                parts.push(part.to_string());
            }
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!parts.is_empty()).then_some(parts)
}

/// Join a path a mod names for one of its own files (a modDesc `storeItem` or `iconFilename`, a
/// map's fill-type list) onto the mod's unpacked folder. `None` when the path could land outside
/// that folder (see [`member_parts`]). Pure: it doesn't look at the disk; [`open_inside`] does.
pub fn join_inside(root: &Path, member: &str) -> Option<PathBuf> {
    let mut out = root.to_path_buf();
    out.extend(member_parts(member)?);
    Some(out)
}

/// Open a mod's own file by the path it names (checked by [`join_inside`]), only when it is a
/// regular file reached without passing through a link: a symlink or junction inside the folder
/// could lead anywhere. `None` when refused or missing.
pub fn open_inside(root: &Path, member: &str) -> Option<std::fs::File> {
    let path = join_inside(root, member)?;
    let parts: Vec<_> = path.strip_prefix(root).ok()?.components().collect();
    let last = parts.len().checked_sub(1)?;
    let mut at = root.to_path_buf();
    for (i, part) in parts.iter().enumerate() {
        at.push(part);
        let kind = std::fs::symlink_metadata(&at).ok()?.file_type();
        let expected = if i == last {
            kind.is_file()
        } else {
            kind.is_dir()
        };
        if kind.is_symlink() || !expected {
            return None;
        }
    }
    std::fs::File::open(path).ok()
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

/// Create `path` as a new, empty file for writing: a file left there by an earlier run is
/// removed first, then the file is created with `create_new`. The caller must own the name
/// (a download's part claim). The result is always a new file, never one another writer still
/// has open, so two writers can't end up sharing (and changing) the same bytes.
pub fn create_fresh(path: &Path) -> std::io::Result<std::fs::File> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e),
        _ => {}
    }
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
}

/// Rename `from` to `to` (a file or a folder, on the same filesystem) without ever replacing
/// anything at `to`: fails with `AlreadyExists` when `to` is taken, even by an empty folder.
/// The check and the rename are one step on Windows (`MoveFileExW` without
/// `MOVEFILE_REPLACE_EXISTING`), Linux (`renameat2` with `RENAME_NOREPLACE`) and macOS
/// (`renamex_np` with `RENAME_EXCL`). Where the filesystem doesn't support that (some network
/// and FUSE filesystems, an old kernel), it falls back to checking `to` is free right before a
/// plain rename; another program creating `to` in between is then the only gap.
pub fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    match rename_noreplace_native(from, to) {
        Some(Ok(())) => Ok(()),
        Some(Err(e)) => Err(taken_or(to, e)),
        None => {
            #[cfg(test)]
            FALLBACK_RENAMES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            rename_checked(from, to)
        }
    }
}

/// How often [`rename_no_replace`] fell back to check-then-rename (tests only).
#[cfg(test)]
static FALLBACK_RENAMES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A failed rename whose destination exists is reported as `AlreadyExists` (Windows can say
/// "access denied" when `to` is a folder), so callers can tell "taken" from a real failure.
fn taken_or(to: &Path, e: std::io::Error) -> std::io::Error {
    if e.kind() != std::io::ErrorKind::AlreadyExists && to.symlink_metadata().is_ok() {
        std::io::Error::from(std::io::ErrorKind::AlreadyExists)
    } else {
        e
    }
}

/// The fallback: check `to` is free, then rename.
fn rename_checked(from: &Path, to: &Path) -> std::io::Result<()> {
    if to.symlink_metadata().is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    std::fs::rename(from, to)
}

/// The OS's no-replace rename; `None` when this system or filesystem doesn't offer one.
#[cfg(windows)]
fn rename_noreplace_native(from: &Path, to: &Path) -> Option<std::io::Result<()>> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;
    let wide =
        |p: &Path| -> Vec<u16> { long_path(p).as_os_str().encode_wide().chain([0]).collect() };
    let (a, b) = (wide(from), wide(to));
    // SAFETY: both are NUL-terminated UTF-16 strings that outlive the call. Flags 0: no
    // MOVEFILE_REPLACE_EXISTING (an existing `to` fails) and no MOVEFILE_COPY_ALLOWED.
    let ok = unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), 0) };
    Some(if ok != 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    })
}

/// `p` in a form `MoveFileExW` accepts past 260 characters: its parent canonicalized (the
/// `\\?\` form) with the name joined on. Short paths, and any that can't be resolved, are
/// passed as given.
#[cfg(windows)]
fn long_path(p: &Path) -> PathBuf {
    const MAX_PATH: usize = 260;
    if p.as_os_str().len() < MAX_PATH {
        return p.to_path_buf();
    }
    match (p.parent().map(std::fs::canonicalize), p.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => p.to_path_buf(),
    }
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn rename_noreplace_native(from: &Path, to: &Path) -> Option<std::io::Result<()>> {
    let (a, b) = (c_path(from)?, c_path(to)?);
    // SAFETY: both are NUL-terminated C strings that outlive the call.
    let r = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            a.as_ptr(),
            libc::AT_FDCWD,
            b.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    unix_result(r)
}

#[cfg(target_os = "macos")]
fn rename_noreplace_native(from: &Path, to: &Path) -> Option<std::io::Result<()>> {
    let (a, b) = (c_path(from)?, c_path(to)?);
    // SAFETY: both are NUL-terminated C strings that outlive the call.
    let r = unsafe { libc::renamex_np(a.as_ptr(), b.as_ptr(), libc::RENAME_EXCL) };
    unix_result(r)
}

#[cfg(not(any(
    windows,
    target_os = "macos",
    all(target_os = "linux", target_env = "gnu")
)))]
fn rename_noreplace_native(_from: &Path, _to: &Path) -> Option<std::io::Result<()>> {
    None
}

/// A path as a C string; `None` (use the fallback) if it holds a NUL byte, which no real path
/// does.
#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
fn c_path(p: &Path) -> Option<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::CString::new(p.as_os_str().as_bytes()).ok()
}

/// A no-replace rename's result. The flag being unsupported (`EINVAL` from a filesystem that
/// doesn't know it, `ENOSYS` from an old kernel, `ENOTSUP`/`EOPNOTSUPP`) means "use the
/// fallback" (`None`).
#[cfg(any(target_os = "macos", all(target_os = "linux", target_env = "gnu")))]
fn unix_result(r: libc::c_int) -> Option<std::io::Result<()>> {
    if r == 0 {
        return Some(Ok(()));
    }
    let e = std::io::Error::last_os_error();
    let unsupported = [libc::EINVAL, libc::ENOSYS, libc::ENOTSUP, libc::EOPNOTSUPP];
    if e.raw_os_error().is_some_and(|c| unsupported.contains(&c)) {
        None
    } else {
        Some(Err(e))
    }
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

    #[test]
    fn member_paths_inside_the_mod_folder_are_joined() {
        let root = Path::new("mods").join("FS25_A");
        let nested = root.join("xml").join("store").join("tractor.xml");
        assert_eq!(join_inside(&root, "icon.dds"), Some(root.join("icon.dds")));
        assert_eq!(
            join_inside(&root, "xml/store/tractor.xml"),
            Some(nested.clone())
        );
        assert_eq!(
            join_inside(&root, r"xml\store\tractor.xml"),
            Some(nested.clone())
        );
        assert_eq!(
            join_inside(&root, "./xml/./store/tractor.xml"),
            Some(nested)
        );
        assert_eq!(
            join_inside(&root, "maps/my map/map.xml"),
            Some(root.join("maps").join("my map").join("map.xml"))
        );
    }

    #[test]
    fn member_paths_outside_the_mod_folder_are_refused() {
        let root = Path::new("mods").join("FS25_A");
        for bad in [
            "",
            ".",
            "./",
            "/x.xml",
            r"\x.xml",
            "//server/share/x.xml",
            r"\\server\share\x.xml",
            r"\\?\C:\x.xml",
            r"\\.\x",
            "C:/x.xml",
            r"C:\x.xml",
            "C:x.xml",
            "c:",
            "../x.xml",
            r"..\x.xml",
            "xml/../../x.xml",
            "xml/..",
            "icon.dds:x",
            "nul",
            "xml/con.xml",
            "a\0b.xml",
            "xml /store.xml",
            "xml./store.xml",
            "store.xml.",
            "store.xml ",
            ".../x.xml",
            ".. /x.xml",
        ] {
            assert_eq!(join_inside(&root, bad), None, "{bad:?}");
            assert!(open_inside(&root, bad).is_none(), "{bad:?}");
        }
    }

    #[test]
    fn only_regular_files_inside_the_mod_folder_are_opened() {
        let base = std::env::temp_dir().join(format!("silo_member_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("FS25_A");
        std::fs::create_dir_all(root.join("xml")).unwrap();
        std::fs::write(root.join("xml").join("store.xml"), "<x/>").unwrap();
        std::fs::write(base.join("outside.xml"), "<y/>").unwrap();

        assert!(open_inside(&root, "xml/store.xml").is_some());
        assert!(open_inside(&root, r"xml\store.xml").is_some());
        assert!(open_inside(&root, "../outside.xml").is_none());
        let absolute = base.join("outside.xml");
        assert!(open_inside(&root, absolute.to_str().unwrap()).is_none());
        assert!(open_inside(&root, "xml").is_none(), "a folder");
        assert!(open_inside(&root, "missing.xml").is_none());
        assert!(open_inside(&root, "xml/store.xml/x").is_none());

        let _ = std::fs::remove_dir_all(&base);
    }

    /// A mod folder `FS25_A` with a file `outside.xml` next to it, for the link tests.
    fn link_fixture(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("{name}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("FS25_A");
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(base.join("outside.xml"), "<y/>").unwrap();
        (base, root)
    }

    /// A link inside the mod folder isn't followed, to a file or through a folder.
    #[cfg(unix)]
    #[test]
    fn a_linked_member_is_not_opened() {
        let (base, root) = link_fixture("silo_member_link");
        std::os::unix::fs::symlink(base.join("outside.xml"), root.join("file.xml")).unwrap();
        std::os::unix::fs::symlink(&base, root.join("up")).unwrap();
        assert!(
            root.join("up").join("outside.xml").is_file(),
            "the link works"
        );
        assert!(open_inside(&root, "file.xml").is_none());
        assert!(open_inside(&root, "up/outside.xml").is_none());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A junction inside the mod folder isn't followed. Making one needs no special rights.
    #[cfg(windows)]
    #[test]
    fn a_junction_in_the_mod_folder_is_not_followed() {
        let (base, root) = link_fixture("silo_member_junction");
        let up = root.join("up");
        crate::organize::junction_to(&base, &up).unwrap();
        assert!(up.join("outside.xml").is_file(), "the junction works");
        assert!(open_inside(&root, "up/outside.xml").is_none());
        std::fs::remove_dir(&up).unwrap(); // the junction only, not what it points at
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `pivot ` (trailing space) is a different folder from `pivot` in the middle of a path, but
    /// the same at its end, so a junction named `pivot ` beside a real `pivot` folder is refused.
    #[cfg(windows)]
    #[test]
    fn a_junction_named_like_a_real_folder_is_not_followed() {
        let (base, root) = link_fixture("silo_member_pivot");
        std::fs::create_dir_all(root.join("pivot")).unwrap();
        // Only the verbatim form can make a name ending in a space.
        let verbatim = std::fs::canonicalize(&root).unwrap().join("pivot ");
        crate::organize::junction_to(&base, &verbatim).unwrap();
        assert!(verbatim.join("outside.xml").is_file(), "the junction works");
        assert!(open_inside(&root, "pivot /outside.xml").is_none());
        std::fs::remove_dir(&verbatim).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A folder mod Silo put in `mods/` as a junction still has its own files read.
    #[cfg(windows)]
    #[test]
    fn a_mod_folder_reached_through_a_junction_still_opens() {
        let (base, root) = link_fixture("silo_member_projected");
        std::fs::create_dir_all(root.join("xml")).unwrap();
        std::fs::write(root.join("xml").join("store.xml"), "<x/>").unwrap();
        let projected = base.join("FS25_A_projected");
        crate::organize::junction_to(&root, &projected).unwrap();
        assert!(open_inside(&projected, "xml/store.xml").is_some());
        std::fs::remove_dir(&projected).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A symlink inside the mod folder isn't followed. Windows needs Developer Mode or admin
    /// rights to make one; without them there is nothing to check.
    #[cfg(windows)]
    #[test]
    fn a_linked_member_is_not_opened() {
        const PRIVILEGE_NOT_HELD: i32 = 1314;
        let (base, root) = link_fixture("silo_member_link");
        let file = std::os::windows::fs::symlink_file(base.join("outside.xml"), root.join("f.xml"));
        let dir = std::os::windows::fs::symlink_dir(&base, root.join("up"));
        for made in [&file, &dir] {
            if let Err(e) = made {
                assert_eq!(e.raw_os_error(), Some(PRIVILEGE_NOT_HELD), "{e}");
            }
        }
        if file.is_ok() {
            assert!(root.join("f.xml").is_file(), "the link works");
            assert!(open_inside(&root, "f.xml").is_none());
        }
        if dir.is_ok() {
            assert!(open_inside(&root, "up/outside.xml").is_none());
            std::fs::remove_dir(root.join("up")).unwrap();
        }
        if file.is_err() && dir.is_err() {
            eprintln!("skipped: making a symlink needs Developer Mode or admin rights");
        }
        let _ = std::fs::remove_dir_all(&base);
    }

    /// A fresh scratch folder for one test.
    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("silo_rnr_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn rename_no_replace_moves_into_a_free_name() {
        let d = scratch("free");
        std::fs::write(d.join("a.part"), b"new").unwrap();
        rename_no_replace(&d.join("a.part"), &d.join("a.zip")).unwrap();
        assert_eq!(std::fs::read(d.join("a.zip")).unwrap(), b"new");
        assert!(d.join("a.part").symlink_metadata().is_err());
        // A folder too.
        std::fs::create_dir(d.join("tmp_dir")).unwrap();
        std::fs::write(d.join("tmp_dir").join("modDesc.xml"), b"<modDesc/>").unwrap();
        rename_no_replace(&d.join("tmp_dir"), &d.join("FS25_D")).unwrap();
        assert!(d.join("FS25_D").join("modDesc.xml").is_file());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_no_replace_never_replaces_an_existing_destination() {
        let d = scratch("taken");
        let (part, dest) = (d.join("a.part"), d.join("a.zip"));
        std::fs::write(&part, b"new").unwrap();
        std::fs::write(&dest, b"user's only copy").unwrap();
        let e = rename_no_replace(&part, &dest).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dest).unwrap(), b"user's only copy");
        assert_eq!(std::fs::read(&part).unwrap(), b"new", "the source stays");

        // Not over an empty folder (a plain Unix rename would replace it) ...
        let tmp_dir = d.join("tmp_dir");
        std::fs::create_dir(&tmp_dir).unwrap();
        std::fs::write(tmp_dir.join("modDesc.xml"), b"<modDesc/>").unwrap();
        let empty = d.join("FS25_D");
        std::fs::create_dir(&empty).unwrap();
        let e = rename_no_replace(&tmp_dir, &empty).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert!(std::fs::read_dir(&empty).unwrap().next().is_none());
        // ... nor a file over a folder, nor a folder over a file.
        let e = rename_no_replace(&part, &empty).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        let e = rename_no_replace(&tmp_dir, &dest).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&dest).unwrap(), b"user's only copy");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn rename_no_replace_refuses_a_name_taken_in_another_case_where_names_ignore_case() {
        let d = scratch("case");
        std::fs::write(d.join("probe"), b"").unwrap();
        let case_insensitive = d.join("PROBE").symlink_metadata().is_ok();
        std::fs::write(d.join("a.part"), b"new").unwrap();
        std::fs::write(d.join("FS25_X.zip"), b"theirs").unwrap();
        let r = rename_no_replace(&d.join("a.part"), &d.join("fs25_x.zip"));
        if case_insensitive {
            assert_eq!(r.unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        } else {
            r.unwrap();
        }
        assert_eq!(std::fs::read(d.join("FS25_X.zip")).unwrap(), b"theirs");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// Windows, Linux (glibc) and macOS take the one-step no-replace rename, never the
    /// check-then-rename fallback, on an ordinary local folder.
    #[cfg(any(
        windows,
        target_os = "macos",
        all(target_os = "linux", target_env = "gnu")
    ))]
    #[test]
    fn the_native_no_replace_rename_is_used_where_the_system_has_one() {
        // Through the wrapper the placement code calls: never the fallback here (no other test
        // reaches it on these systems either).
        let d0 = scratch("native_wrapper");
        std::fs::write(d0.join("a.part"), b"new").unwrap();
        std::fs::write(d0.join("a.zip"), b"theirs").unwrap();
        assert!(rename_no_replace(&d0.join("a.part"), &d0.join("a.zip")).is_err());
        rename_no_replace(&d0.join("a.part"), &d0.join("b.zip")).unwrap();
        assert_eq!(
            FALLBACK_RENAMES.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "check-then-rename was used"
        );
        let _ = std::fs::remove_dir_all(&d0);

        let d = scratch("native");
        std::fs::write(d.join("a.part"), b"new").unwrap();
        std::fs::write(d.join("a.zip"), b"theirs").unwrap();
        let r = rename_noreplace_native(&d.join("a.part"), &d.join("a.zip"))
            .expect("this system has a no-replace rename");
        assert_eq!(
            taken_or(&d.join("a.zip"), r.unwrap_err()).kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(d.join("a.zip")).unwrap(), b"theirs");
        rename_noreplace_native(&d.join("a.part"), &d.join("b.zip"))
            .expect("this system has a no-replace rename")
            .unwrap();
        assert_eq!(std::fs::read(d.join("b.zip")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_checked_fallback_refuses_a_destination_that_already_exists() {
        let d = scratch("fallback");
        std::fs::write(d.join("a.part"), b"new").unwrap();
        std::fs::write(d.join("a.zip"), b"theirs").unwrap();
        let e = rename_checked(&d.join("a.part"), &d.join("a.zip")).unwrap_err();
        assert_eq!(e.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(d.join("a.zip")).unwrap(), b"theirs");
        rename_checked(&d.join("a.part"), &d.join("b.zip")).unwrap();
        assert_eq!(std::fs::read(d.join("b.zip")).unwrap(), b"new");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn create_fresh_replaces_a_leftover_with_a_new_file() {
        let d = scratch("fresh");
        let part = d.join("FS25_X.zip.part");
        std::fs::write(&part, b"half a download from a crash").unwrap();
        // A writer still holding the leftover keeps its own (now nameless) file.
        let mut old = std::fs::OpenOptions::new().write(true).open(&part).unwrap();
        let mut f = create_fresh(&part).unwrap();
        f.write_all(b"PK new").unwrap();
        drop(f);
        old.write_all(b"late bytes").unwrap();
        drop(old);
        assert_eq!(std::fs::read(&part).unwrap(), b"PK new");
        let _ = std::fs::remove_dir_all(&d);
    }
}
