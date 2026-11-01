//! Mod updates that can't corrupt your library: download beside the mod, swap with
//! renames, keep the old version in backups/, and recover if Silo stops mid-swap.
//!
//! The swap (spec §4), all under the library lock:
//! a. note whether the mods folder holds Silo's projection of `dest`;
//! b. rename `dest` → `<dest>.silo-update-old` (same folder, atomic);
//! c. rename the downloaded part → `dest`;
//! d. if it was projected, re-point the projection at the new file (temp + rename);
//! e. move the old file to `<mods>/backups/`, record it there, and keep the newest
//!    [`KEEP_BACKUPS`] of the backups Silo recorded making ([`prune_backups`]).
//!
//! The download streams into a part file of its own (`<dest>.<token>.silo-update.part`),
//! recorded in the scratch journal before it's made (`crate::PartClaim`); only a recorded part
//! no download claims is ever removed (#400).
//!
//! If c or d fails the swap is undone (the old version is `dest` again). A crash at any step
//! leaves either `dest` or its `-old` twin (or both), and [`recover_interrupted`] finishes or
//! rolls back from there. Nothing is ever truncated or overwritten in place, and the old
//! version is never deleted until a newer one is in backups.

use crate::net::RespExt;
use crate::organize::{self, LinkPolicy};
use crate::scratch::{self, Journal, Role};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Ends the download's name (`<dest>.<token>.silo-update.part`), beside the mod it replaces.
/// `scan` skips it.
pub const PART_SUFFIX: &str = ".silo-update.part";
/// The previous version, set aside during the swap. `scan` skips it.
pub const OLD_SUFFIX: &str = ".silo-update-old";
/// Previous versions kept per mod file.
pub const KEEP_BACKUPS: usize = 3;

/// 4 GiB bounds a runaway or hostile stream without rejecting a large map mod. Every mod
/// download uses it (`siloapi::download_to_part` too), so a mod that installs also updates.
pub(crate) const MAX_BYTES: u64 = 4 * 1024 * 1024 * 1024;

/// The name downloads of `dest` are told apart by (the old fixed part name; the part itself
/// gets a name of its own).
pub fn part_path(dest: &Path) -> PathBuf {
    with_suffix(dest, PART_SUFFIX)
}

pub fn old_path(dest: &Path) -> PathBuf {
    with_suffix(dest, OLD_SUFFIX)
}

/// The file name a download address names: its last path segment, percent-decoded, without
/// query or fragment. None when there is none, or it isn't valid UTF-8 once decoded.
pub(crate) fn asset_file_name(url: &str) -> Option<String> {
    let u = url::Url::parse(url).ok()?;
    let last = u.path_segments()?.next_back()?;
    let bytes = last.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        match (
            bytes[i],
            bytes.get(i + 1).and_then(|&b| hex(b)),
            bytes.get(i + 2).and_then(|&b| hex(b)),
        ) {
            (b'%', Some(h), Some(l)) => {
                out.push((h * 16 + l) as u8);
                i += 3;
            }
            (b, _, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok().filter(|n| !n.is_empty())
}

/// Refuse an update whose release file isn't named exactly like the installed file `dest`
/// (#391). FS25 knows a mod by its file name, so writing `FS25_X_v2.zip` over `FS25_X.zip`
/// would be renaming a mod zip: the game, and every multiplayer peer, would see a different
/// mod. Silo never renames one; the user can install that release from Browse instead.
pub(crate) fn check_asset_name(url: &str, dest: &Path) -> Result<(), String> {
    let installed = dest
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    match asset_file_name(url) {
        Some(asset) if asset == installed => Ok(()),
        Some(asset) => Err(format!(
            "The new release file is named {asset}, not {installed}. Installing it would make \
             the game see a different mod, so Silo didn't update {installed}. If you want it, \
             install it from Browse."
        )),
        None => Err(format!(
            "Silo couldn't tell what the new release file is called, so it didn't update \
             {installed}: a file with a different name would make the game see a different mod."
        )),
    }
}

fn with_suffix(p: &Path, suffix: &str) -> PathBuf {
    let mut s = p.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Download the update at `url` into a part file of its own beside `dest`, recorded in `db`'s
/// scratch journal first, and prove it's a whole zip of the same mod. Runs outside the library
/// lock; nothing in the library changes. The returned claim holds the part (another download
/// of the same mod is refused meanwhile) and removes the part it made when dropped, so a failed
/// or abandoned download leaves nothing behind.
pub fn fetch(url: &str, dest: &Path, db: Option<&Path>) -> Result<crate::PartClaim, String> {
    crate::github::check_update_url(url)?;
    check_asset_name(url, dest)?;
    let claim = crate::claim_part_at(dest, &part_path(dest), PART_SUFFIX, db)?;
    // Release assets redirect (github.com → release-assets.githubusercontent.com); every hop
    // must pass the same check as the first URL.
    let resp = crate::net::get_guarded(
        crate::net::download(),
        url,
        &|u| crate::github::check_update_url(u.as_str()),
        None,
    )
    .map_err(|e| e.to_string())?;
    write_part(resp.into_reader(), &claim, dest)?;
    Ok(claim)
}

/// Stream `reader` into the claim's part (bounded), then check the result opens as a zip and is
/// the same mod as `dest` (`github::verify_update_identity`).
fn write_part(reader: impl Read, claim: &crate::PartClaim, dest: &Path) -> Result<(), String> {
    let part = claim.path();
    {
        // A new file at the claim's own name: nothing already there is removed or written into.
        let mut out = claim.create().map_err(|e| e.to_string())?;
        let mut limited = reader.take(MAX_BYTES + 1);
        let n = std::io::copy(&mut limited, &mut out).map_err(|e| e.to_string())?;
        if n > MAX_BYTES {
            return Err(format!(
                "The update is larger than {} GB, so Silo stopped the download.",
                MAX_BYTES / (1024 * 1024 * 1024)
            ));
        }
        out.sync_all().map_err(|e| e.to_string())?;
    }
    let f = std::fs::File::open(part).map_err(|e| e.to_string())?;
    zip::ZipArchive::new(f).map_err(|_| {
        "The download isn't a valid .zip (it may be corrupt or cut off). Try again.".to_string()
    })?;
    crate::github::verify_update_identity(part, dest)
}

/// What a finished swap did.
#[derive(Debug, Default)]
pub struct Swapped {
    /// Where the previous version is kept. `None` if moving it there failed (see `problems`);
    /// it then stays beside the mod as `<dest>.silo-update-old` and a later scan retries.
    pub backup: Option<PathBuf>,
    /// The mods-folder entry Silo re-pointed at the new file (an archived mod's projection).
    pub relinked: Option<PathBuf>,
    /// What went wrong after the new version was in place. The update itself stands.
    pub problems: Vec<String>,
}

type Relink<'r> = &'r mut dyn FnMut(&Path, &Path, &mut LinkPolicy) -> std::io::Result<()>;

/// Put the downloaded `part` in place of `dest` (steps a–e above). Call under the library lock.
/// `mods_root` is the flat mods folder (projections and `backups/` live there). Refuses, with
/// nothing changed, unless `dest` is a plain file and no earlier swap of it is unfinished.
pub(crate) fn swap(
    dest: &Path,
    part: &Path,
    mods_root: &Path,
    policy: &mut LinkPolicy,
) -> Result<Swapped, String> {
    swap_with(
        dest,
        part,
        mods_root,
        policy,
        &mut organize::replace_zip_link,
    )
}

/// [`swap`] with step d's link replacement injected (tests make it fail).
fn swap_with(
    dest: &Path,
    part: &Path,
    mods_root: &Path,
    policy: &mut LinkPolicy,
    relink: Relink,
) -> Result<Swapped, String> {
    let name = file_name(dest)?;
    match dest.symlink_metadata() {
        Ok(m) if m.is_file() => {}
        Ok(_) => {
            return Err(format!(
                "{name} isn't a plain file, so Silo won't replace it."
            ))
        }
        Err(_) => {
            return Err(format!(
                "{name} is no longer in your library. Rescan and try again."
            ))
        }
    }
    let old = old_path(dest);
    if old.symlink_metadata().is_ok() {
        return Err(format!(
            "An earlier update of {name} didn't finish: its previous version is still at {}. \
             Move it somewhere else, then try again.",
            old.display()
        ));
    }

    // a. Is the mods-folder entry Silo's projection of this (archived) file?
    let flat = mods_root.join(&name);
    let projected = !in_folder(dest, mods_root) && organize::is_silo_projection(&flat, dest, "zip");

    // b. Set the current version aside.
    std::fs::rename(dest, &old)
        .map_err(|e| format!("Couldn't set {name} aside for the update: {e}"))?;

    // c. Put the new version in place, or put the old one back.
    if let Err(e) = std::fs::rename(part, dest) {
        return Err(match std::fs::rename(&old, dest) {
            Ok(()) => format!(
                "Couldn't put the new {name} in place, so the current version was kept: {e}"
            ),
            Err(back) => format!(
                "Couldn't put the new {name} in place ({e}) or put the current version back \
                 ({back}). It is safe at {}; Silo puts it back on the next scan.",
                old.display()
            ),
        });
    }
    crate::fslog::moved("update", part, dest);

    let mut out = Swapped::default();
    // d. Point the mods folder at the new version, or undo the swap: the game must never be
    //    left loading the old build behind Silo's back.
    if projected {
        if let Err(e) = relink(&flat, dest, policy) {
            return Err(undo_swap(dest, part, &old, &name, &e));
        }
        out.relinked = Some(flat);
    }
    // e. Keep the previous version in backups.
    match keep_backup(&old, dest, mods_root, &name, policy) {
        Ok(b) => out.backup = Some(b),
        Err(e) => out.problems.push(format!(
            "{name} was updated, but {}",
            backup_problem(&name, &old, mods_root, &e)
        )),
    }
    Ok(out)
}

/// Why the previous version `old` of `name` couldn't go to `<mods_root>/backups/`, and what to do.
fn backup_problem(name: &str, old: &Path, mods_root: &Path, e: &std::io::Error) -> String {
    let drive = organize::mount_of(mods_root)
        .map(|(m, _)| m.display().to_string())
        .unwrap_or_else(|| mods_root.display().to_string());
    format!(
        "Silo couldn't move the previous version of {name} to backups: {e}. Free space on {drive} \
         or move it yourself from {}, then try again.",
        old.display()
    )
}

/// Undo steps b–c after step d failed: the new file goes back to `part`, the old one to `dest`.
fn undo_swap(dest: &Path, part: &Path, old: &Path, name: &str, e: &std::io::Error) -> String {
    let why = format!("Couldn't point your mods folder at the new {name} ({e})");
    if let Err(back) = std::fs::rename(dest, part) {
        return format!(
            "{why}, and couldn't undo the update ({back}). Silo finishes it on the next scan."
        );
    }
    match std::fs::rename(old, dest) {
        Ok(()) => {
            crate::fslog::moved("restore", old, dest);
            format!("{why}, so the update was undone and the current version kept.")
        }
        Err(back) => format!(
            "{why}, and couldn't put the current version back ({back}). It is safe at {}; \
             Silo puts it back on the next scan.",
            old.display()
        ),
    }
}

/// Move the set-aside `old` (a previous version of `dest`) into `<mods_root>/backups/`, record
/// it in the policy's journal ([`Role::UpdateBak`], with its size), and prune that mod's older
/// recorded backups. A backup that couldn't be recorded is kept for good (never pruned).
fn keep_backup(
    old: &Path,
    dest: &Path,
    mods_root: &Path,
    name: &str,
    policy: &LinkPolicy,
) -> std::io::Result<PathBuf> {
    let bak = organize::update_backup_path(mods_root, name)?;
    // A cross-drive move is a full copy: asked for room first, like every copy a move makes.
    organize::move_path_checked(old, &bak, "backup", policy.room_check())?;
    let journal = policy.journal();
    if let Some(j) = journal {
        let e = scratch::Entry {
            path: bak.clone(),
            role: Role::UpdateBak,
            from: old.to_path_buf(),
            to: dest.to_path_buf(),
            kind: "zip".into(),
            size: bak.symlink_metadata().ok().map(|m| m.len()),
        };
        if let Err(e) = j.record(&e) {
            log::warn!(
                "couldn't record the backup {} ({e}): it is kept, and never pruned",
                bak.display()
            );
        }
    }
    prune_backups(journal, &mods_root.join("backups"), name, KEEP_BACKUPS);
    Ok(bak)
}

/// Old versions whose re-point or move to backups recovery already tried (and failed) this
/// session. In the background (scans, ordinary library writes) each is tried at most once per
/// run, so a stuck multi-GB cross-drive copy isn't repeated on every scan; an action the user
/// asked for ([`Attempt::UserAsked`]) always tries again.
static TRIED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Who is asking recovery to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Attempt {
    /// A scan or an ordinary library write: costly steps run at most once per session per file.
    Background,
    /// The user started something that needs the leftovers gone (an update, an archive move).
    UserAsked,
}

/// Take a turn at the costly steps for `old`: false in the background once it has had one.
fn take_turn(old: &Path, attempt: Attempt) -> bool {
    let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
    if attempt == Attempt::Background && tried.iter().any(|p| p == old) {
        return false;
    }
    if !tried.iter().any(|p| p == old) {
        tried.push(old.to_path_buf());
    }
    true
}

/// `old` was put away: a later update of the same mod gets its own turns.
fn turn_done(old: &Path) {
    let mut tried = TRIED.lock().unwrap_or_else(|e| e.into_inner());
    tried.retain(|p| p != old);
}

/// What recovery did, and what it couldn't fix.
#[derive(Debug, Default)]
pub struct Recovery {
    /// Updates finished or rolled back, for the user.
    pub notices: Vec<String>,
    /// What couldn't be fixed, for the user (each names the file and why).
    pub problems: Vec<String>,
    /// Set-aside old versions still there afterwards (filled in by [`settle`]).
    pub leftovers: Vec<PathBuf>,
    /// Of those, the ones whose mod is missing from the library: it is short that mod until the
    /// old version can be put back.
    pub stranded: Vec<PathBuf>,
    /// Part-file names no journal records (an older version's, or not Silo's): logged and left
    /// in place.
    pub unrecorded: Vec<PathBuf>,
}

/// Finish or roll back any update in `dir` that stopped partway (non-recursive), in the
/// background ([`Attempt::Background`]) and with no other folders to consult. See
/// [`recover_in`].
#[cfg(test)]
pub(crate) fn recover_interrupted(
    dir: &Path,
    mods_root: &Path,
    policy: &mut LinkPolicy,
) -> Recovery {
    let mut out = Recovery::default();
    recover_in(dir, mods_root, &[], policy, Attempt::Background, &mut out);
    out
}

/// Finish or roll back any update in `dir` that stopped partway (non-recursive):
/// - a lone `X.silo-update-old` goes back to `X`, unless the mod now lives in one of `others`
///   (another archive category, or the archive for a mod in the mods folder): then it goes to
///   backups, so the library never ends up with the mod twice;
/// - one beside a new `X` finishes the swap (re-point the projection, then backup).
///
/// Part files are [`settle`]'s (only recorded ones are ever removed). Call under the library
/// lock.
fn recover_in(
    dir: &Path,
    mods_root: &Path,
    others: &[PathBuf],
    policy: &mut LinkPolicy,
    attempt: Attempt,
    out: &mut Recovery,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().into_owned();
        if let Some(base) = entry_name.strip_suffix(OLD_SUFFIX) {
            let ctx = Leftover {
                dest: dir.join(base),
                old: path,
                name: base,
                mods_root,
                elsewhere: lives_elsewhere(others, base),
            };
            finish_or_roll_back(&ctx, policy, attempt, out);
        }
    }
}

/// One set-aside old version and where its mod belongs.
struct Leftover<'a> {
    dest: PathBuf,
    old: PathBuf,
    name: &'a str,
    mods_root: &'a Path,
    /// The same mod file in another library folder.
    elsewhere: Option<PathBuf>,
}

/// The mod file `name` in one of `others`, if it's there.
fn lives_elsewhere(others: &[PathBuf], name: &str) -> Option<PathBuf> {
    others
        .iter()
        .map(|d| d.join(name))
        .find(|p| p.symlink_metadata().is_ok())
}

fn finish_or_roll_back(
    l: &Leftover,
    policy: &mut LinkPolicy,
    attempt: Attempt,
    out: &mut Recovery,
) {
    let (dest, old, name, mods_root) = (&l.dest, &l.old, l.name, l.mods_root);
    if dest.symlink_metadata().is_err() {
        if let Some(other) = &l.elsewhere {
            // The mod lives somewhere else now: putting this back would file it twice.
            if !take_turn(old, attempt) {
                return;
            }
            match keep_backup(old, dest, mods_root, name, policy) {
                Ok(b) => {
                    turn_done(old);
                    out.notices.push(format!(
                        "An update of {name} was interrupted. The mod is now at {}, so Silo kept \
                         the previous version in {}.",
                        other.display(),
                        b.display()
                    ));
                }
                Err(e) => out.problems.push(backup_problem(name, old, mods_root, &e)),
            }
            return;
        }
        // Stopped before the new version was placed: the old one is the mod again.
        match std::fs::rename(old, dest) {
            Ok(()) => {
                crate::fslog::moved("restore", old, dest);
                out.notices.push(format!(
                    "An update of {name} was interrupted, so Silo put the previous version back."
                ))
            }
            Err(e) => out.problems.push(format!(
                "An update of {name} was interrupted and Silo couldn't put the previous version \
                 back: {e}. It is at {}; close any program using it, then try again.",
                old.display()
            )),
        }
        return;
    }
    // The new version is in place. A projection still on the old file is re-pointed first (a
    // cross-drive copy can be large, hence the turn); if that fails, the old file stays set
    // aside for a later turn.
    if !take_turn(old, attempt) {
        return;
    }
    let flat = mods_root.join(name);
    if !in_folder(dest, mods_root) && organize::is_silo_projection(&flat, old, "zip") {
        if let Err(e) = organize::replace_zip_link(&flat, dest, policy) {
            out.problems.push(format!(
                "Silo couldn't point your mods folder at the new version of {name}: {e}. The \
                 previous version is kept at {} until it can; close the game, then try again.",
                old.display()
            ));
            return;
        }
    }
    match keep_backup(old, dest, mods_root, name, policy) {
        Ok(b) => {
            turn_done(old);
            out.notices.push(format!(
                "Silo finished an interrupted update of {name}. The previous version is kept \
                 in {}.",
                b.display()
            ));
        }
        Err(e) => out.problems.push(backup_problem(name, old, mods_root, &e)),
    }
}

/// Run recovery over `dirs` (the mods folder and archive categories of one layout), then list
/// what is still set aside. Call under the library lock.
///
/// First, with the policy's journal, the recorded part files of downloads that stopped (a
/// crash) are swept (`scratch::sweep_where`: a file, not a link, no download running now
/// claims it). Part-file names in `dirs` that aren't recorded are logged and left in place.
pub(crate) fn settle(
    dirs: &[PathBuf],
    mods_root: &Path,
    policy: &mut LinkPolicy,
    attempt: Attempt,
) -> Recovery {
    let others_of = |dir: &PathBuf| -> Vec<PathBuf> {
        dirs.iter()
            .filter(|d| *d != dir && d.as_path() != mods_root)
            .cloned()
            .collect()
    };
    let mut out = Recovery::default();
    let journal: &dyn Journal = policy.journal().unwrap_or(&scratch::NoJournal);
    scratch::sweep_where(journal, &|e| {
        e.role == Role::Part && !crate::part_in_flight(&e.path)
    });
    for dir in dirs {
        out.unrecorded
            .extend(scratch::log_unrecorded(journal, dir, &|n: &str| {
                n.to_lowercase().ends_with(PART_SUFFIX)
            }));
    }
    for dir in dirs {
        recover_in(dir, mods_root, &others_of(dir), policy, attempt, &mut out);
    }
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let Some(base) = n.strip_suffix(OLD_SUFFIX) else {
                continue;
            };
            let missing = dir.join(base).symlink_metadata().is_err()
                && lives_elsewhere(&others_of(dir), base).is_none();
            if missing {
                out.stranded.push(e.path());
            }
            out.leftovers.push(e.path());
        }
    }
    out
}

/// Why something must wait for `paths` (set-aside old versions): the recorded problems when
/// there are any (they name each file and the cause), else the paths themselves.
pub(crate) fn leftover_message(r: &Recovery, paths: &[PathBuf]) -> String {
    if !r.problems.is_empty() {
        return r.problems.join(" ");
    }
    let list: Vec<String> = paths.iter().map(|p| p.display().to_string()).collect();
    format!(
        "An interrupted mod update left a previous version Silo couldn't put away: {}. Close \
         any program using it, or move it somewhere else, then try again.",
        list.join(", ")
    )
}

/// The folders an update can write in for `layout`: the mods folder and each archive category.
pub(crate) fn layout_dirs(layout: &organize::Layout) -> Vec<PathBuf> {
    let mut dirs = vec![layout.mods.clone()];
    if let Ok(entries) = std::fs::read_dir(&layout.archive) {
        let mut cats: Vec<PathBuf> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        cats.sort();
        dirs.extend(cats);
    }
    dirs
}

/// Keep the `keep` newest update backups of `file_name` in `backups` that Silo recorded making
/// ([`Role::UpdateBak`] in `journal`, in the order it made them); remove older ones (#400).
/// One is removed only while it's still what Silo recorded: a file (never a link), the size it
/// had. One that isn't any more is logged and left, and its record dropped (it no longer
/// counts). A backup-like name nothing records (`<file_name>.<secs>[-n].update.bak` from an
/// earlier version, or the user's own file) is logged and left in place, as is a half-copied
/// `.<file_name>.….silo-tmp` a stopped cross-drive move left (#394). Never touches anything
/// else there, including the `.bak` files adopt/restore keep. Returns the unrecorded
/// backup-like names it logged.
pub(crate) fn prune_backups(
    journal: Option<&dyn Journal>,
    backups: &Path,
    file_name: &str,
    keep: usize,
) -> Vec<PathBuf> {
    let journal = journal.unwrap_or(&scratch::NoJournal);
    let prefix = format!("{file_name}.");
    let tmp_prefix = format!(".{file_name}.");
    let tmp_suffix = format!("{}.silo-tmp", organize::UPDATE_BACKUP_SUFFIX);
    if let Ok(entries) = std::fs::read_dir(backups) {
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let half = n
                .strip_prefix(&tmp_prefix)
                .and_then(|s| s.strip_suffix(&tmp_suffix))
                .and_then(backup_order);
            if half.is_some() {
                log::warn!(
                    "left {} in place: it looks like an older version's half-copied backup",
                    e.path().display()
                );
            }
        }
    }
    let unrecorded = scratch::log_unrecorded(journal, backups, &|n: &str| {
        n.strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(organize::UPDATE_BACKUP_SUFFIX))
            .and_then(backup_order)
            .is_some()
    });
    let mut proven: Vec<PathBuf> = Vec::new();
    for e in journal.entries() {
        let mine = e.role == Role::UpdateBak
            && e.path.parent() == Some(backups)
            && e.to.file_name().is_some_and(|n| n == file_name);
        if !mine {
            continue;
        }
        match e.path.symlink_metadata() {
            Ok(m) if !m.file_type().is_symlink() && m.is_file() && Some(m.len()) == e.size => {
                proven.push(e.path)
            }
            Ok(_) => {
                log::warn!(
                    "left {} in place: it's no longer the backup Silo made there",
                    e.path.display()
                );
                journal.forget(&e.path);
            }
            // Gone (the user removed it): it no longer counts. Kept recorded while its folder
            // can't be reached.
            Err(_) if backups.symlink_metadata().is_ok() => journal.forget(&e.path),
            Err(_) => {}
        }
    }
    let surplus = proven.len().saturating_sub(keep);
    for p in proven.into_iter().take(surplus) {
        match std::fs::remove_file(&p) {
            Ok(()) => {
                journal.forget(&p);
                crate::fslog::changed("prune", &p);
            }
            Err(e) => log::warn!("couldn't remove the old backup {}: {e}", p.display()),
        }
    }
    unrecorded
}

/// `<secs>` or `<secs>-<n>` as a sort key.
fn backup_order(stamp: &str) -> Option<(u64, u64)> {
    match stamp.split_once('-') {
        Some((secs, seq)) => Some((secs.parse().ok()?, seq.parse().ok()?)),
        None => Some((stamp.parse().ok()?, 1)),
    }
}

fn file_name(p: &Path) -> Result<String, String> {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| format!("{} isn't a mod file", p.display()))
}

/// `p` sits directly in `folder` (compared canonically, so spelling differences don't matter).
fn in_folder(p: &Path, folder: &Path) -> bool {
    let Some(parent) = p.parent() else {
        return false;
    };
    match (parent.canonicalize(), folder.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => parent == folder,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_release_named_differently_from_the_installed_file_is_refused() {
        // #391: never written over the installed file under its name.
        let dest = Path::new("mods/FS25_X.zip");
        let same = "https://github.com/o/r/releases/download/v2/FS25_X.zip";
        assert!(super::check_asset_name(same, dest).is_ok());
        assert!(super::check_asset_name(&format!("{same}?raw=1#frag"), dest).is_ok());
        let err = super::check_asset_name(
            "https://github.com/o/r/releases/download/v2/FS25_X_v2.zip",
            dest,
        )
        .unwrap_err();
        assert!(
            err.contains("FS25_X_v2.zip") && err.contains("different mod"),
            "{err}"
        );
        assert!(err.contains("Browse"), "{err}");
        // A different case is a different name to the game.
        assert!(super::check_asset_name(
            "https://github.com/o/r/releases/download/v2/fs25_x.zip",
            dest
        )
        .is_err());
        // Percent-encoding is read as the name it encodes.
        let spaced = Path::new("mods/FS25 X.zip");
        assert!(super::check_asset_name(
            "https://github.com/o/r/releases/download/v2/FS25%20X.zip",
            spaced
        )
        .is_ok());
        assert_eq!(
            super::asset_file_name("https://github.com/o/r/releases/download/v2/a%2"),
            Some("a%2".to_string())
        );
        // No file name at all: refused, never guessed.
        assert!(super::check_asset_name("https://github.com/", dest).is_err());
        assert!(super::check_asset_name("not a url", dest).is_err());
    }

    use super::*;
    use crate::organize::Layout;
    use std::io::Write;

    fn fresh(tag: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("silo_update_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// A minimal mod zip whose modDesc declares `unique_type` and `version`.
    fn make_mod_zip(path: &Path, unique_type: &str, version: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        let f = std::fs::File::create(path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        zw.start_file("modDesc.xml", opts).unwrap();
        write!(
            zw,
            "<modDesc descVersion=\"100\"><author>A</author><title><en>T</en></title>\
             <uniqueType>{unique_type}</uniqueType><version>{version}</version></modDesc>"
        )
        .unwrap();
        zw.finish().unwrap();
    }

    fn version_of(path: &Path) -> Option<String> {
        let xml = crate::scan::read_moddesc_xml(path, "zip").ok()?;
        crate::moddesc::parse(&xml).version
    }

    fn backups_of(mods: &Path) -> Vec<PathBuf> {
        let mut v: Vec<PathBuf> = std::fs::read_dir(mods.join("backups"))
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        v.sort();
        v
    }

    fn is_update_backup_of(p: &Path, file_name: &str) -> bool {
        let n = p.file_name().unwrap().to_string_lossy().into_owned();
        n.strip_prefix(&format!("{file_name}."))
            .and_then(|s| s.strip_suffix(".update.bak"))
            .is_some_and(|stamp| stamp.chars().all(|c| c.is_ascii_digit() || c == '-'))
    }

    #[test]
    fn an_unprojected_update_swaps_the_file_and_keeps_a_backup() {
        let mods = fresh("unprojected");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");

        let done = swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();

        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert!(!part.exists(), "the part became the mod");
        assert!(!old_path(&dest).exists(), "nothing left set aside");
        assert!(
            done.relinked.is_none(),
            "a mod in the mods folder has no projection"
        );
        assert!(done.problems.is_empty(), "{:?}", done.problems);
        let backup = done.backup.expect("the old version is kept");
        assert_eq!(backups_of(&mods), vec![backup.clone()]);
        assert!(is_update_backup_of(&backup, "FS25_A.zip"), "{backup:?}");
        assert_eq!(version_of(&backup).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    /// A second name for the installed file's bytes must still read the old version after the
    /// update: proof nothing was written into the installed file in place.
    #[test]
    fn an_update_never_writes_into_the_installed_file() {
        let mods = fresh("witness");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let witness = mods.join("witness.bin");
        std::fs::hard_link(&dest, &witness).unwrap();
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");

        swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();

        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert_eq!(version_of(&witness).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_projected_update_repoints_the_link_to_the_new_file() {
        let mods = fresh("projected");
        let layout = Layout::default_for(&mods);
        let dest = layout.archive.join("Misc").join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let flat = mods.join("FS25_A.zip");
        std::fs::hard_link(&dest, &flat).unwrap();
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");

        let done = swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();

        assert_eq!(done.relinked.as_deref(), Some(flat.as_path()));
        assert_eq!(version_of(&flat).as_deref(), Some("2.0.0.0"));
        assert!(organize::is_silo_projection(&flat, &dest, "zip"));
        assert!(!mods.join("FS25_A.zip.silo-relink-tmp").exists());
        let backup = done.backup.expect("the old version is kept");
        assert_eq!(version_of(&backup).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_failed_relink_undoes_the_update() {
        let mods = fresh("relink_fails");
        let layout = Layout::default_for(&mods);
        let dest = layout.archive.join("Misc").join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let flat = mods.join("FS25_A.zip");
        std::fs::hard_link(&dest, &flat).unwrap();
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");
        let mut fail = |_: &Path, _: &Path, _: &mut LinkPolicy| -> std::io::Result<()> {
            Err(std::io::Error::other("no links today"))
        };

        let err = swap_with(
            &dest,
            &part,
            &mods,
            &mut LinkPolicy::new(&layout),
            &mut fail,
        )
        .unwrap_err();

        assert!(err.contains("undone"), "{err}");
        assert_eq!(version_of(&dest).as_deref(), Some("1.0.0.0"));
        assert!(
            organize::is_silo_projection(&flat, &dest, "zip"),
            "still Silo's link"
        );
        assert_eq!(
            version_of(&part).as_deref(),
            Some("2.0.0.0"),
            "back at the part"
        );
        assert!(!old_path(&dest).exists());
        assert!(backups_of(&mods).is_empty());
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn an_update_never_replaces_a_users_own_file_in_the_mods_folder() {
        let mods = fresh("users_flat");
        let layout = Layout::default_for(&mods);
        let dest = layout.archive.join("Misc").join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let flat = mods.join("FS25_A.zip");
        make_mod_zip(&flat, "A", "9.9.9.9-mine");
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");

        let done = swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();

        assert!(done.relinked.is_none());
        assert_eq!(version_of(&flat).as_deref(), Some("9.9.9.9-mine"));
        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_failed_swap_keeps_the_current_version() {
        let mods = fresh("failed_swap");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        // No part file: step c fails, and the old version must be put back.
        let err = swap(
            &dest,
            &part_path(&dest),
            &mods,
            &mut LinkPolicy::new(&layout),
        )
        .unwrap_err();
        assert!(err.contains("current version was kept"), "{err}");
        assert_eq!(version_of(&dest).as_deref(), Some("1.0.0.0"));
        assert!(!old_path(&dest).exists());
        assert!(backups_of(&mods).is_empty());
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_failed_backup_keeps_the_old_version_until_recovery_finishes() {
        let mods = fresh("backup_fails");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        // `backups` is a file, so the backups folder can't be made.
        std::fs::write(mods.join("backups"), b"in the way").unwrap();
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "2.0.0.0");

        let done = swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();

        assert!(done.backup.is_none());
        assert_eq!(done.problems.len(), 1, "{:?}", done.problems);
        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert_eq!(version_of(&old_path(&dest)).as_deref(), Some("1.0.0.0"));

        // Once the way is clear, the next recovery finishes the job.
        std::fs::remove_file(mods.join("backups")).unwrap();
        let r = recover_interrupted(&mods, &mods, &mut LinkPolicy::new(&layout));
        assert_eq!(r.notices.len(), 1, "{r:?}");
        assert!(!old_path(&dest).exists());
        let baks = backups_of(&mods);
        assert_eq!(baks.len(), 1);
        assert_eq!(version_of(&baks[0]).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_failing_backup_is_retried_once_per_session() {
        let mods = fresh("backup_retry");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "2.0.0.0");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");
        std::fs::write(mods.join("backups"), b"in the way").unwrap();

        let first = recover_interrupted(&mods, &mods, &mut LinkPolicy::new(&layout));
        assert_eq!(first.problems.len(), 1, "{first:?}");
        let second = recover_interrupted(&mods, &mods, &mut LinkPolicy::new(&layout));
        assert!(
            second.problems.is_empty() && second.notices.is_empty(),
            "{second:?}"
        );
        assert_eq!(version_of(&old_path(&dest)).as_deref(), Some("1.0.0.0"));
        let s = settle(
            std::slice::from_ref(&mods),
            &mods,
            &mut LinkPolicy::new(&layout),
            Attempt::Background,
        );
        assert_eq!(s.leftovers, vec![old_path(&dest)]);
        assert!(s.stranded.is_empty(), "the mod itself is in place");
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_swap_refuses_while_an_earlier_one_is_unfinished() {
        let mods = fresh("unfinished");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "2.0.0.0");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");
        let part = part_path(&dest);
        make_mod_zip(&part, "A", "3.0.0.0");
        assert!(swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).is_err());
        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert_eq!(version_of(&old_path(&dest)).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn recovery_restores_the_old_file_when_the_swap_stopped_halfway() {
        let mods = fresh("recover_back");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");

        let r = recover_interrupted(&mods, &mods, &mut LinkPolicy::new(&layout));

        assert_eq!(r.notices.len(), 1, "{r:?}");
        assert!(r.problems.is_empty());
        assert_eq!(version_of(&dest).as_deref(), Some("1.0.0.0"));
        assert!(!old_path(&dest).exists());
        assert!(backups_of(&mods).is_empty());
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn recovery_finishes_a_swap_that_already_placed_the_new_file() {
        let mods = fresh("recover_forward");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "2.0.0.0");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");

        recover_interrupted(&mods, &mods, &mut LinkPolicy::new(&layout));

        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert!(!old_path(&dest).exists());
        let baks = backups_of(&mods);
        assert_eq!(baks.len(), 1, "{baks:?}");
        assert!(is_update_backup_of(&baks[0], "FS25_A.zip"));
        assert_eq!(version_of(&baks[0]).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn recovery_repoints_a_projection_left_on_the_old_file() {
        let mods = fresh("recover_relink");
        let layout = Layout::default_for(&mods);
        let cat = layout.archive.join("Misc");
        let dest = cat.join("FS25_A.zip");
        // Stopped after step c: the new file is placed, the flat entry still links the old one.
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");
        let flat = mods.join("FS25_A.zip");
        std::fs::hard_link(old_path(&dest), &flat).unwrap();
        make_mod_zip(&dest, "A", "2.0.0.0");

        recover_interrupted(&cat, &mods, &mut LinkPolicy::new(&layout));

        assert_eq!(version_of(&flat).as_deref(), Some("2.0.0.0"));
        assert!(organize::is_silo_projection(&flat, &dest, "zip"));
        assert!(!old_path(&dest).exists());
        assert_eq!(backups_of(&mods).len(), 1);
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn recovery_leaves_a_users_own_file_in_the_mods_folder_alone() {
        let mods = fresh("recover_users_flat");
        let layout = Layout::default_for(&mods);
        let cat = layout.archive.join("Misc");
        let dest = cat.join("FS25_A.zip");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");
        make_mod_zip(&dest, "A", "2.0.0.0");
        let flat = mods.join("FS25_A.zip");
        make_mod_zip(&flat, "A", "9.9.9.9-mine");

        recover_interrupted(&cat, &mods, &mut LinkPolicy::new(&layout));

        assert_eq!(version_of(&flat).as_deref(), Some("9.9.9.9-mine"));
        assert!(!organize::is_silo_projection(&flat, &dest, "zip"));
        assert_eq!(version_of(&dest).as_deref(), Some("2.0.0.0"));
        assert_eq!(backups_of(&mods).len(), 1);
        let _ = std::fs::remove_dir_all(&mods);
    }

    /// A recorded part entry at `path` for `dest`, as a claim records it.
    fn part_entry(path: &Path, dest: &Path) -> scratch::Entry {
        scratch::Entry {
            path: path.to_path_buf(),
            role: Role::Part,
            from: dest.to_path_buf(),
            to: dest.to_path_buf(),
            kind: "zip".into(),
            size: None,
        }
    }

    fn settle_with(mods: &Path, layout: &Layout, journal: &dyn Journal) -> Recovery {
        settle(
            &[mods.to_path_buf()],
            mods,
            &mut LinkPolicy::new(layout).with_journal(journal),
            Attempt::Background,
        )
    }

    #[test]
    fn an_unrecorded_part_is_logged_and_left_in_place() {
        // #400: an older version's fixed-name part, or a look-alike Silo never recorded, is
        // never deleted by name.
        let mods = fresh("recover_part_unrecorded");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let legacy = part_path(&dest);
        std::fs::write(&legacy, b"half a download from an older Silo").unwrap();
        let lookalike = scratch::unique_beside(&dest, PART_SUFFIX);
        std::fs::write(&lookalike, b"not recorded").unwrap();
        let journal = scratch::tests::MemJournal::default();

        let r = settle_with(&mods, &layout, &journal);

        assert!(legacy.exists() && lookalike.exists());
        let mut logged = r.unrecorded.clone();
        logged.sort();
        let mut want = vec![legacy.clone(), lookalike.clone()];
        want.sort();
        assert_eq!(logged, want, "both are logged");
        // Without a journal at all, the same: logged, left.
        let r = recover_and_list(&mods, &layout);
        assert!(legacy.exists() && lookalike.exists());
        assert_eq!(r.unrecorded.len(), 2, "{r:?}");
        assert_eq!(version_of(&dest).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    fn recover_and_list(mods: &Path, layout: &Layout) -> Recovery {
        settle(
            &[mods.to_path_buf()],
            mods,
            &mut LinkPolicy::new(layout),
            Attempt::Background,
        )
    }

    #[test]
    fn a_recorded_part_a_crash_left_is_swept() {
        let mods = fresh("recover_part_recorded");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let part = scratch::unique_beside(&dest, PART_SUFFIX);
        let journal = scratch::tests::MemJournal::default();
        journal.record(&part_entry(&part, &dest)).unwrap();
        std::fs::write(&part, b"half a download").unwrap();

        let r = settle_with(&mods, &layout, &journal);

        assert!(!part.exists(), "a recorded part nothing claims is swept");
        assert!(journal.entries().is_empty(), "and its record goes");
        assert!(r.unrecorded.is_empty(), "{r:?}");
        assert_eq!(version_of(&dest).as_deref(), Some("1.0.0.0"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_recorded_part_that_isnt_a_file_is_kept() {
        let mods = fresh("recover_part_dir");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        let part = scratch::unique_beside(&dest, PART_SUFFIX);
        let journal = scratch::tests::MemJournal::default();
        journal.record(&part_entry(&part, &dest)).unwrap();
        std::fs::create_dir_all(part.join("inside")).unwrap();
        std::fs::write(part.join("inside").join("f"), b"x").unwrap();

        settle_with(&mods, &layout, &journal);

        assert!(
            part.join("inside").join("f").exists(),
            "a folder is never a part"
        );
        assert_eq!(journal.entries().len(), 1, "kept recorded");
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_link_at_a_recorded_part_is_never_followed() {
        let mods = fresh("recover_part_link");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        let victim = mods.join("elsewhere.zip");
        std::fs::write(&victim, b"someone else's file").unwrap();
        let part = scratch::unique_beside(&dest, PART_SUFFIX);
        let journal = scratch::tests::MemJournal::default();
        journal.record(&part_entry(&part, &dest)).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, &part).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&victim, &part).is_err() {
            return; // no symlink privilege on this machine (CI on Unix covers it)
        }

        settle_with(&mods, &layout, &journal);

        assert!(part.symlink_metadata().is_ok(), "the link is kept");
        assert_eq!(std::fs::read(&victim).unwrap(), b"someone else's file");
        assert_eq!(journal.entries().len(), 1, "kept recorded");
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn recovery_leaves_a_part_that_a_download_is_writing() {
        let mods = fresh("recover_claimed");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        let db = mods.join("silo.db");
        let claim = crate::claim_part_at(&dest, &part_path(&dest), PART_SUFFIX, Some(&db)).unwrap();
        std::io::Write::write_all(&mut claim.create().unwrap(), b"downloading").unwrap();
        let conn = crate::db::open(&db).unwrap();
        let journal = scratch::DbJournal(&conn);
        assert_eq!(
            journal.entries(),
            vec![part_entry(claim.path(), &dest)],
            "recorded before it was made"
        );

        let r = settle_with(&mods, &layout, &journal);

        assert!(
            claim.path().exists(),
            "an in-flight download is never removed"
        );
        assert!(r.unrecorded.is_empty(), "{r:?}");
        let part = claim.path().to_path_buf();
        drop(claim);
        assert!(!part.exists(), "the claim cleans up the part it made");
        assert!(journal.entries().is_empty(), "and drops its record");
        drop(conn);
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_leftover_message_names_the_full_path_or_the_cause() {
        let old = PathBuf::from("mods").join("FS25_A.zip.silo-update-old");
        let msg = leftover_message(&Recovery::default(), std::slice::from_ref(&old));
        assert!(msg.contains(&old.display().to_string()), "{msg}");
        let r = Recovery {
            problems: vec!["the disk is full".into()],
            ..Default::default()
        };
        assert_eq!(leftover_message(&r, &[old]), "the disk is full");
    }

    #[test]
    fn a_backup_problem_names_the_cause_and_where_the_old_version_is() {
        let mods = fresh("backup_problem_msg");
        let old = mods.join("FS25_A.zip.silo-update-old");
        let msg = backup_problem(
            "FS25_A.zip",
            &old,
            &mods,
            &std::io::Error::other("no space left"),
        );
        assert!(msg.contains("no space left"), "{msg}");
        assert!(msg.contains(&old.display().to_string()), "{msg}");
        assert!(msg.contains("Free space on"), "{msg}");
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_user_asked_settle_retries_what_the_background_gave_up_on() {
        let mods = fresh("user_retry");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "2.0.0.0");
        make_mod_zip(&old_path(&dest), "A", "1.0.0.0");
        std::fs::write(mods.join("backups"), b"in the way").unwrap();
        let dirs = vec![mods.clone()];
        let bg = |l: &Layout| settle(&dirs, &mods, &mut LinkPolicy::new(l), Attempt::Background);

        assert_eq!(bg(&layout).problems.len(), 1, "first background turn");
        assert!(bg(&layout).problems.is_empty(), "no second background turn");
        let asked = settle(
            &dirs,
            &mods,
            &mut LinkPolicy::new(&layout),
            Attempt::UserAsked,
        );
        assert_eq!(asked.problems.len(), 1, "the user's action tries again");
        assert_eq!(asked.leftovers, vec![old_path(&dest)]);

        std::fs::remove_file(mods.join("backups")).unwrap();
        let asked = settle(
            &dirs,
            &mods,
            &mut LinkPolicy::new(&layout),
            Attempt::UserAsked,
        );
        assert!(asked.leftovers.is_empty(), "{asked:?}");
        assert_eq!(backups_of(&mods).len(), 1);
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_lone_old_version_goes_to_backups_when_the_mod_lives_elsewhere() {
        let mods = fresh("lives_elsewhere");
        let layout = Layout::default_for(&mods);
        let tools = layout.archive.join("Tools");
        let misc = layout.archive.join("Misc");
        // The mod was refiled under Misc; Tools still holds a set-aside old version.
        make_mod_zip(&misc.join("FS25_A.zip"), "A", "2.0.0.0");
        make_mod_zip(&old_path(&tools.join("FS25_A.zip")), "A", "1.0.0.0");

        let s = settle(
            &layout_dirs(&layout),
            &mods,
            &mut LinkPolicy::new(&layout),
            Attempt::Background,
        );

        assert!(!tools.join("FS25_A.zip").exists(), "not filed twice");
        assert!(s.leftovers.is_empty() && s.stranded.is_empty(), "{s:?}");
        let baks = backups_of(&mods);
        assert_eq!(baks.len(), 1);
        assert_eq!(version_of(&baks[0]).as_deref(), Some("1.0.0.0"));
        assert_eq!(
            version_of(&misc.join("FS25_A.zip")).as_deref(),
            Some("2.0.0.0")
        );
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn layout_dirs_are_the_mods_folder_and_each_archive_category() {
        let mods = fresh("layout_dirs");
        let layout = Layout::default_for(&mods);
        std::fs::create_dir_all(layout.archive.join("Tools")).unwrap();
        std::fs::create_dir_all(layout.archive.join("Maps")).unwrap();
        std::fs::write(layout.archive.join("stray.txt"), b"x").unwrap();
        assert_eq!(
            layout_dirs(&layout),
            vec![
                mods.clone(),
                layout.archive.join("Maps"),
                layout.archive.join("Tools")
            ]
        );
        let _ = std::fs::remove_dir_all(&mods);
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut v: Vec<String> = std::fs::read_dir(dir)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        v.sort();
        v
    }

    /// Update `dest` (in the mods folder) to `version` through a swap recorded in `journal`.
    fn update_to(dest: &Path, mods: &Path, version: &str, journal: &dyn Journal) -> PathBuf {
        let layout = Layout::default_for(mods);
        let part = part_path(dest);
        make_mod_zip(&part, "A", version);
        let mut policy = LinkPolicy::new(&layout).with_journal(journal);
        swap(dest, &part, mods, &mut policy)
            .unwrap()
            .backup
            .expect("kept")
    }

    #[test]
    fn only_the_newest_three_recorded_backups_are_kept() {
        let mods = fresh("prune");
        let backups = mods.join("backups");
        std::fs::create_dir_all(&backups).unwrap();
        // Backup-like names nothing records (an older version's backups, the user's own):
        // logged and left in place (#400), even the oldest.
        for t in 1..=3 {
            std::fs::write(backups.join(format!("FS25_A.zip.{t}.update.bak")), b"x").unwrap();
        }
        std::fs::write(backups.join("FS25_A.zip.3-2.update.bak"), b"x").unwrap();
        // An older version's half-copied backup (a fixed name): left in place (#394).
        std::fs::write(backups.join(".FS25_A.zip.6.update.bak.silo-tmp"), b"half").unwrap();
        // Not update backups of this mod: never touched.
        std::fs::write(backups.join("FS25_A.zip.1.bak"), b"user's own").unwrap();
        std::fs::write(backups.join("FS25_B.zip.1.update.bak"), b"other mod").unwrap();
        let before = names_in(&backups);

        let journal = scratch::tests::MemJournal::default();
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let made: Vec<PathBuf> = (2..=5)
            .map(|v| update_to(&dest, &mods, &format!("{v}.0.0.0"), &journal))
            .collect();

        // The first of the four recorded backups is gone; the newest three stay.
        assert!(!made[0].exists(), "{made:?}");
        for b in &made[1..] {
            assert!(b.exists(), "{b:?}");
        }
        assert_eq!(version_of(&made[3]).as_deref(), Some("4.0.0.0"));
        let recorded: Vec<PathBuf> = journal
            .entries()
            .into_iter()
            .filter(|e| e.role == Role::UpdateBak)
            .map(|e| e.path)
            .collect();
        assert_eq!(recorded, made[1..].to_vec(), "only what's left is recorded");
        // Everything that was there before is still there.
        let after = names_in(&backups);
        for n in &before {
            assert!(after.contains(n), "{n} was removed: {after:?}");
        }
        assert_eq!(after.len(), before.len() + 3, "{after:?}");
        // A direct prune lists the unrecorded look-alikes it logged.
        let mut logged = prune_backups(Some(&journal), &backups, "FS25_A.zip", KEEP_BACKUPS);
        logged.sort();
        let mut want: Vec<PathBuf> = ["FS25_A.zip.1.update.bak", "FS25_A.zip.2.update.bak"]
            .iter()
            .chain(["FS25_A.zip.3-2.update.bak", "FS25_A.zip.3.update.bak"].iter())
            .map(|n| backups.join(n))
            .collect();
        want.sort();
        assert_eq!(logged, want);
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_recorded_backup_that_changed_is_left_and_no_longer_counts() {
        let mods = fresh("prune_changed");
        let journal = scratch::tests::MemJournal::default();
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let first = update_to(&dest, &mods, "2.0.0.0", &journal);
        // The user put something else at the oldest backup's name (another size).
        std::fs::write(&first, b"the user's own file now").unwrap();
        let rest: Vec<PathBuf> = (3..=5)
            .map(|v| update_to(&dest, &mods, &format!("{v}.0.0.0"), &journal))
            .collect();

        assert_eq!(std::fs::read(&first).unwrap(), b"the user's own file now");
        assert!(
            rest.iter().all(|b| b.exists()),
            "the three proven ones stay"
        );
        assert!(
            !journal.entries().iter().any(|e| e.path == first),
            "its record goes"
        );
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_link_at_a_recorded_backup_is_never_followed_or_removed() {
        let mods = fresh("prune_link");
        let journal = scratch::tests::MemJournal::default();
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let first = update_to(&dest, &mods, "2.0.0.0", &journal);
        let victim = mods.join("elsewhere.zip");
        make_mod_zip(&victim, "A", "9.9.9.9");
        std::fs::remove_file(&first).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&victim, &first).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&victim, &first).is_err() {
            return; // no symlink privilege on this machine (CI on Unix covers it)
        }
        for v in 3..=5 {
            update_to(&dest, &mods, &format!("{v}.0.0.0"), &journal);
        }

        assert!(first
            .symlink_metadata()
            .is_ok_and(|m| m.file_type().is_symlink()));
        assert_eq!(version_of(&victim).as_deref(), Some("9.9.9.9"));
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn backups_without_a_journal_are_never_pruned() {
        let mods = fresh("prune_no_journal");
        let layout = Layout::default_for(&mods);
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        for v in 2..=5 {
            let part = part_path(&dest);
            make_mod_zip(&part, "A", &format!("{v}.0.0.0"));
            swap(&dest, &part, &mods, &mut LinkPolicy::new(&layout)).unwrap();
        }
        assert_eq!(
            backups_of(&mods).len(),
            4,
            "nothing recorded, nothing removed"
        );
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn a_download_that_isnt_a_whole_zip_of_the_same_mod_is_refused() {
        let mods = fresh("write_part");
        let dest = mods.join("FS25_A.zip");
        make_mod_zip(&dest, "A", "1.0.0.0");
        let claim = || crate::claim_part_at(&dest, &part_path(&dest), PART_SUFFIX, None).unwrap();

        let err = write_part(&b"not a zip"[..], &claim(), &dest).unwrap_err();
        assert!(err.contains("valid .zip"), "{err}");

        let other = mods.join("other.zip");
        make_mod_zip(&other, "B", "1.0.0.0");
        let bytes = std::fs::read(&other).unwrap();
        assert!(
            write_part(&bytes[..], &claim(), &dest).is_err(),
            "a different mod"
        );

        let same = mods.join("same.zip");
        make_mod_zip(&same, "A", "2.0.0.0");
        let bytes = std::fs::read(&same).unwrap();
        assert!(write_part(&bytes[..], &claim(), &dest).is_ok());
        assert_eq!(
            version_of(&dest).as_deref(),
            Some("1.0.0.0"),
            "dest untouched"
        );
        let _ = std::fs::remove_dir_all(&mods);
    }

    #[test]
    fn fetch_refuses_non_github_urls_before_downloading() {
        let mods = fresh("fetch_refuse");
        let dest = mods.join("FS25_A.zip");
        assert!(fetch("https://evil.example/x.zip", &dest, None).is_err());
        assert!(!part_path(&dest).exists());
        let _ = std::fs::remove_dir_all(&mods);
    }
}
