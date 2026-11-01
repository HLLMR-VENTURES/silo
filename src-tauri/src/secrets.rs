//! Secret storage for API tokens. Prefers the OS credential store (Windows Credential
//! Manager, macOS Keychain, Linux Secret Service) so tokens never sit in plaintext
//! beside ordinary preferences. Two safety nets keep it robust:
//!
//! * **Migration** — a token written by an older build (plaintext in the `app_setting`
//!   table) is moved into the keychain the first time it's read, then cleared from SQLite.
//! * **Fallback** — on a machine with no working keychain (e.g. a headless Linux box),
//!   we fall back to the SQLite setting so the feature still works. That's no worse than
//!   before; everywhere with a keychain (all Windows/macOS, most Linux) is now secure.
//!   Only when every keychain call fails at the platform level (nothing answers) does it
//!   lead there; any other keychain failure on save or disconnect is returned to the
//!   caller, never swallowed.
//!
//! Only genuine secrets (e.g. `gh_token`) go through here. Non-secret flags like
//! `gh_write` stay in `app_setting`.

use crate::db;
use rusqlite::Connection;

const SERVICE: &str = "com.hllmr.silo";

/// Every key stored through this module. Database snapshots drop these (`db::snapshot`).
pub(crate) const SECRET_KEYS: &[&str] = &["gh_token"];

/// The keychain entry for `key`. None in a demo profile, which keeps its (fake) tokens in its
/// own database and never reads or writes the real keychain.
fn entry(key: &str) -> Option<keyring::Entry> {
    if crate::profile::active() {
        return None;
    }
    keyring::Entry::new(SERVICE, key).ok()
}

/// Whether this machine has a working OS keychain. When false, the UI warns before connecting:
/// with no keychain service at all the token goes to the local SQLite DB (not OS-encrypted),
/// and with a locked or refusing one connecting fails until the user fixes it.
/// Does a full write→read→delete round-trip on a throwaway probe entry, since some platforms
/// create an `Entry` fine but fail on the actual store.
pub fn keychain_available() -> bool {
    if crate::profile::active() {
        return false;
    }
    match keyring::Entry::new(SERVICE, "silo-keychain-probe") {
        Ok(e) => {
            if e.set_password("probe").is_ok() {
                let ok = e.get_password().is_ok();
                let _ = e.delete_credential();
                ok
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

/// Held by every read that may move a row into the keychain and by every write or clear, so a
/// read that saw a row can't put it back into the keychain after a disconnect removed it.
static STORE: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn store_lock() -> std::sync::MutexGuard<'static, ()> {
    STORE.lock().unwrap_or_else(|e| e.into_inner())
}

/// The (non-secret) setting that says the keychain may hold a copy of `key` that Silo must not
/// use: one a disconnect couldn't confirm gone. (While a fallback row exists the row wins
/// anyway.) Cleared once the keychain accepts a write or a delete again.
fn distrust_key(key: &str) -> String {
    format!("{key}:keychain_distrusted")
}

/// Whether the keychain copy is distrusted. A marker Silo can't read counts as set, so a
/// database read error never brings a distrusted copy back.
fn keychain_distrusted(conn: &Connection, key: &str) -> bool {
    setting(conn, &distrust_key(key)).map_or(true, |v| v.is_some())
}

/// Read a setting, telling "no row" apart from "couldn't read" (`db::get_app_setting` returns
/// None for both). For the reads that decide whether a token may be used or counts as gone.
fn setting(conn: &Connection, key: &str) -> Result<Option<String>, String> {
    use rusqlite::OptionalExtension;
    conn.query_row("SELECT value FROM app_setting WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .optional()
    .map_err(|e| e.to_string())
}

/// Set or clear the marker. Clearing one that isn't set writes nothing, so it can't fail.
fn set_distrust(conn: &Connection, key: &str, on: bool) -> Result<(), String> {
    if !on && !keychain_distrusted(conn, key) {
        return Ok(());
    }
    db::set_app_setting(conn, &distrust_key(key), on.then_some("1"))
}

/// The (non-secret) setting that names a SQLite row Silo must not use: a fingerprint of the
/// token a disconnect couldn't delete from the database. Only that exact value is refused, so a
/// later reconnect through the fallback (a new row) is used as normal.
fn revoked_key(key: &str) -> String {
    format!("{key}:revoked_row")
}

/// A one-way fingerprint of a secret, for `revoked_key`.
fn fingerprint(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"silo-revoked-row\0");
    h.update(value.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Whether `row` is the value a disconnect revoked: Ok(true) or Ok(false), or Err when the
/// marker can't be read (then the row may be neither used nor deleted).
fn row_revoked(conn: &Connection, key: &str, row: &str) -> Result<bool, String> {
    Ok(setting(conn, &revoked_key(key))?.is_some_and(|f| f == fingerprint(row)))
}

/// Revoke `row` (`Some`) or lift the revocation (`None`). Lifting one that isn't set writes
/// nothing; one Silo can't read is deleted.
fn set_revoked(conn: &Connection, key: &str, row: Option<&str>) -> Result<(), String> {
    match row {
        Some(r) => db::set_app_setting(conn, &revoked_key(key), Some(&fingerprint(r))),
        None if setting(conn, &revoked_key(key)) == Ok(None) => Ok(()),
        None => db::set_app_setting(conn, &revoked_key(key), None),
    }
}

/// Store `v` in the SQLite fallback. A revocation naming this very value (the user reconnects
/// with the token a failed disconnect left behind), or one Silo can't read, is lifted, or the
/// save fails.
fn write_fallback(conn: &Connection, key: &str, v: &str) -> Result<(), SaveError> {
    db::set_app_setting(conn, key, Some(v)).map_err(SaveError::unchanged)?;
    if row_revoked(conn, key, v) != Ok(false) {
        set_revoked(conn, key, None).map_err(SaveError::changed)?;
    }
    Ok(())
}

/// Whether this machine has no keychain service at all, as far as keyring can tell: a write to a
/// throwaway entry fails at the platform level. Any other answer (it works, it's locked, access
/// is denied) means a store exists, so a failure there is the user's to fix, not a reason to
/// fall back to SQLite.
fn keychain_missing() -> bool {
    // macOS and Windows always have their store (Keychain, Credential Manager). There, a
    // platform-level failure is a refusal (keyring maps a cancelled unlock prompt on macOS to
    // `PlatformFailure`) or a broken call, never a reason to put the token in SQLite.
    if cfg!(any(target_os = "macos", target_os = "windows")) || crate::profile::active() {
        return false;
    }
    let Ok(e) = keyring::Entry::new(SERVICE, "silo-keychain-probe") else {
        return false;
    };
    match e.set_password("probe") {
        Ok(()) => {
            let _ = e.delete_credential();
            false
        }
        Err(err) => no_keychain_service(&err),
    }
}

/// Read a secret. A SQLite row wins over the keychain: every keychain write clears the row, and
/// the row is only written when the keychain can't be used, so it's never the older of the two.
/// A row is moved into the keychain once the keychain works (this is also how a token written
/// by a pre-keychain build is migrated). A distrusted keychain copy and a revoked row are never
/// returned; Silo deletes them as soon as the store allows.
pub fn get(conn: &Connection, key: &str) -> Option<String> {
    get_with(conn, key, entry(key).as_ref())
}

fn get_with(conn: &Connection, key: &str, keychain: Option<&keyring::Entry>) -> Option<String> {
    let _held = store_lock();
    // Strict: a row Silo can't read may be the newest token (a fallback sign-in), so it's never
    // taken as "no row", which would return an older keychain copy instead.
    let Ok(row) = setting(conn, key) else {
        return None;
    };
    if let Some(row) = row {
        match row_revoked(conn, key, &row) {
            // Can't tell whether this row is the current token or a revoked one: use nothing,
            // delete nothing, and don't fall back to an older keychain copy either.
            Err(_) => return None,
            // A disconnect couldn't delete this row. Never use it; remove it once the database
            // allows, and go on to the keychain (whose copy, if any, is newer, or distrusted).
            Ok(true) => {
                let _ = clear_fallback(conn, key);
            }
            Ok(false) => {
                // Best-effort promotion into the keychain, then drop the plaintext copy AND VACUUM
                // so the token isn't left carve-able in the DB's free pages (SQLite doesn't zero
                // deletes). The marker goes first: a row dropped while it stayed would make the
                // next read delete the keychain copy just written.
                if let Some(e) = keychain {
                    if e.set_password(&row).is_ok() && set_distrust(conn, key, false).is_ok() {
                        let _ = db::set_app_setting(conn, key, None);
                        db::vacuum(conn);
                    }
                }
                return Some(row);
            }
        }
    }
    let e = keychain?;
    match setting(conn, &distrust_key(key)) {
        Ok(None) => e.get_password().ok(),
        Ok(Some(_)) => {
            if clear_keychain(e) == Cleared::Gone {
                let _ = set_distrust(conn, key, false);
            }
            None
        }
        // Can't tell: don't use it (and don't delete what may be a trusted copy).
        Err(_) => None,
    }
}

/// Shown when the keychain refused to store a token on a machine that has one.
const SAVE_FAILED: &str = "Silo couldn't save the token in your system's credential store. \
     If it's locked, unlock it and try again.";
/// Shown when disconnecting couldn't delete the keychain copy (Silo has stopped using it).
pub(crate) const REMOVE_FAILED: &str = "Silo couldn't remove the token from your system's \
     credential store. Silo won't use it, and removes it once the store allows (for example \
     after you unlock it). You can also delete the com.hllmr.silo entry there yourself.";

/// Shown when disconnecting couldn't delete the database copy, but revoked it.
pub(crate) const REMOVE_ROW_FAILED: &str = "Silo couldn't remove the token from its database. \
     Silo won't use it, and removes it the next time it can.";

/// Why a secret couldn't be saved, and whether the stored value may have changed anyway (it was
/// written, but a step after it failed). When `changed` is false nothing was written, so the
/// value stored before (the account connected now) is still the one in use.
#[derive(Debug, PartialEq)]
pub struct SaveError {
    pub message: String,
    pub changed: bool,
}

impl SaveError {
    fn unchanged(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            changed: false,
        }
    }
    fn changed(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            changed: true,
        }
    }
}

/// What a disconnect left behind. Silo uses neither: a kept keychain copy is distrusted and a
/// kept row is revoked, and both are deleted once the stores allow.
#[derive(Debug, Default, PartialEq)]
pub struct Removal {
    /// The keychain may still hold a copy.
    pub keychain_kept: bool,
    /// The database still holds its row.
    pub row_kept: bool,
}

impl Removal {
    /// What to tell the user, when something was left behind.
    pub fn message(&self) -> Option<&'static str> {
        if self.keychain_kept {
            Some(REMOVE_FAILED)
        } else if self.row_kept {
            Some(REMOVE_ROW_FAILED)
        } else {
            None
        }
    }
}

/// Store (`Some`) or clear (`None`) a secret. Keychain-primary; SQLite only when this machine
/// has no usable keychain at all. Any other keychain failure is returned, never swallowed.
pub fn set(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    set_with(conn, key, value, entry(key).as_ref(), keychain_missing)
}

/// Save a secret (connect), saying whether a failure may still have changed it.
pub fn store(conn: &Connection, key: &str, value: &str) -> Result<(), SaveError> {
    let _held = store_lock();
    store_with(conn, key, value, entry(key).as_ref(), keychain_missing)
}

/// Forget a secret (disconnect). Silo stops using it either way; the `Removal` says what was
/// left behind. Err when even that couldn't be recorded in the database, so a copy might be
/// read back later (in a later run of Silo).
pub fn clear(conn: &Connection, key: &str) -> Result<Removal, String> {
    let _held = store_lock();
    clear_with(conn, key, entry(key).as_ref(), keychain_missing)
}

/// Delete what a disconnect left behind (a revoked row, a distrusted keychain copy) once the
/// stores allow, without reading a token out. For while the account is signed out, when no
/// `get` runs to do it. It never touches a token a reconnect saved: that save lifts both marks,
/// under the same lock.
pub fn sweep(conn: &Connection, key: &str) {
    let _held = store_lock();
    sweep_with(conn, key, entry(key).as_ref());
}

fn sweep_with(conn: &Connection, key: &str, keychain: Option<&keyring::Entry>) {
    if let Ok(Some(row)) = setting(conn, key) {
        if row_revoked(conn, key, &row) == Ok(true) {
            let _ = clear_fallback(conn, key);
        }
    }
    if let (Some(e), Ok(Some(_))) = (keychain, setting(conn, &distrust_key(key))) {
        if clear_keychain(e) == Cleared::Gone {
            let _ = set_distrust(conn, key, false);
        }
    }
}

/// Disconnect when the database can't be opened: delete the keychain copy at least. True when
/// it's gone (or there's no keychain entry to delete).
pub fn forget_keychain(key: &str) -> bool {
    let _held = store_lock();
    entry(key).map_or(true, |e| clear_keychain(&e) == Cleared::Gone)
}

/// `set`, with the keychain entry and the "is there no keychain service at all?" probe passed
/// in so tests can use fakes instead of the real store.
fn set_with(
    conn: &Connection,
    key: &str,
    value: Option<&str>,
    keychain: Option<&keyring::Entry>,
    missing: impl Fn() -> bool,
) -> Result<(), String> {
    let _held = store_lock();
    match value {
        Some(v) => store_with(conn, key, v, keychain, missing).map_err(|e| e.message),
        None => match clear_with(conn, key, keychain, missing)?.message() {
            None => Ok(()),
            Some(m) => Err(m.to_string()),
        },
    }
}

/// `store` without the lock, with the keychain entry and probe passed in.
fn store_with(
    conn: &Connection,
    key: &str,
    v: &str,
    keychain: Option<&keyring::Entry>,
    missing: impl Fn() -> bool,
) -> Result<(), SaveError> {
    match keychain {
        // Demo profile, or no keychain backend at all.
        None => write_fallback(conn, key, v),
        Some(e) => save(conn, key, v, e, missing),
    }
}

/// Save to the keychain; SQLite only when there's no keychain service at all.
fn save(
    conn: &Connection,
    key: &str,
    v: &str,
    e: &keyring::Entry,
    missing: impl Fn() -> bool,
) -> Result<(), SaveError> {
    match e.set_password(v) {
        Ok(()) => saved_to_keychain(conn, key).map_err(SaveError::changed),
        // No usable keychain (e.g. a Linux box without Secret Service): keep the feature working
        // via SQLite; the keychain_available warning covers this. Only when the write, a read,
        // the probe (a throwaway entry) and one more write all fail at the platform level: a
        // store that answers at all (an entry, "no entry", locked, access denied) exists, so its
        // failure is an error. The existing entry is never deleted here, so a failed save
        // always leaves the account that's connected now intact. The SQLite row then wins over
        // any keychain copy (`get`), and a disconnect that can't confirm that copy gone
        // distrusts it.
        // A platform-level failure doesn't say the write didn't happen (a reply lost on the
        // way back still leaves the new secret stored), so it counts as a possible change. Only
        // a refusal (locked, access denied, a bad entry) leaves the old token for certain.
        Err(err)
            if no_keychain_service(&err)
                && keychain_copy(e) == KeychainCopy::NoService
                && missing() =>
        {
            // The store may have come back meanwhile: try the keychain once more, as an
            // overwrite (never delete-then-write).
            match e.set_password(v) {
                Ok(()) => saved_to_keychain(conn, key).map_err(SaveError::changed),
                Err(again) if no_keychain_service(&again) => {
                    log::warn!("{key}: no usable keychain ({err}); storing it in the database");
                    write_fallback(conn, key, v).map_err(|e| SaveError::changed(e.message))
                }
                Err(again) => {
                    log::warn!("{key}: the keychain answered, but refused the write: {again}");
                    Err(SaveError::changed(SAVE_FAILED))
                }
            }
        }
        Err(err) => {
            log::warn!("{key}: couldn't save to the keychain: {err}");
            // Whether to fall back and whether the write may have landed are separate
            // questions: any platform-level failure leaves the outcome unknown.
            Err(SaveError {
                message: SAVE_FAILED.into(),
                changed: matches!(err, keyring::Error::PlatformFailure(_)),
            })
        }
    }
}

/// After a keychain write: that copy is trusted, and no plaintext copy stays behind (e.g. from
/// a prior build).
fn saved_to_keychain(conn: &Connection, key: &str) -> Result<(), String> {
    set_distrust(conn, key, false)?;
    clear_fallback(conn, key)
}

#[derive(Debug, PartialEq)]
enum Cleared {
    /// The keychain holds no copy.
    Gone,
    /// The store's service fails on both the delete and a read: no keychain to hold one.
    NoService,
    /// A copy may remain (readable, locked, or refused).
    Kept,
}

/// Delete the keychain copy and say what's left. A locked store is never "cleared", and a
/// delete that reports success only counts once a read confirms the entry is gone: on macOS,
/// keyring 3.6 ignores the result of the native delete and always reports success.
fn clear_keychain(e: &keyring::Entry) -> Cleared {
    let deleted = e.delete_credential();
    match (keychain_copy(e), deleted) {
        (KeychainCopy::Gone, _) => Cleared::Gone,
        (KeychainCopy::NoService, Err(err)) if no_keychain_service(&err) => Cleared::NoService,
        _ => Cleared::Kept,
    }
}

/// Disconnect: remove the keychain copy and the SQLite row. Only a copy confirmed gone counts
/// as removed, except where there's no keychain service and the token lived in SQLite. When
/// the keychain may keep a copy, it's distrusted first, and the row is revoked before it's
/// deleted, so Silo stops using either even when a delete fails.
fn clear_with(
    conn: &Connection,
    key: &str,
    keychain: Option<&keyring::Entry>,
    missing: impl Fn() -> bool,
) -> Result<Removal, String> {
    // A row Silo can't read can be neither revoked nor counted as gone, so then the result is
    // an error (the caller keeps the account signed out and retries). The keychain copy is
    // still deleted first: a database problem mustn't keep a healthy keychain's token.
    let row = setting(conn, key);
    let Some(e) = keychain else {
        let row = row?;
        return Ok(Removal {
            keychain_kept: false,
            row_kept: drop_row(conn, key, row.as_deref())?,
        });
    };
    let cleared = clear_keychain(e);
    let row = row?;
    let removed = match cleared {
        Cleared::Gone => {
            set_distrust(conn, key, false)?;
            true
        }
        cleared => {
            // Distrust before dropping the row, so a failure here leaves everything as it was.
            set_distrust(conn, key, true)?;
            cleared == Cleared::NoService && row.is_some() && missing()
        }
    };
    if !removed {
        log::warn!("{key}: couldn't remove it from the keychain; it won't be used");
    }
    // The plaintext copy goes either way.
    Ok(Removal {
        keychain_kept: !removed,
        row_kept: drop_row(conn, key, row.as_deref())?,
    })
}

/// Delete the SQLite row (`row`, as read just before). It's revoked first, so a row the delete
/// leaves behind is never read back; if even that write fails, the delete may still work.
/// Ok(true) when the row stayed (revoked); Err when it stayed unrevoked.
fn drop_row(conn: &Connection, key: &str, row: Option<&str>) -> Result<bool, String> {
    let revoked = match row {
        Some(r) => set_revoked(conn, key, Some(r)),
        None => Ok(()),
    };
    match clear_fallback(conn, key) {
        Ok(()) => Ok(false),
        Err(e) => {
            log::warn!("{key}: couldn't delete it from the database: {e}");
            match (row, revoked) {
                (Some(_), Ok(())) => Ok(true),
                _ => Err(e),
            }
        }
    }
}

/// A keychain error that positively shows there's no keychain service to use: on Linux, no
/// Secret Service on the session bus, or no session bus at all. Only this lets a token go to
/// the SQLite fallback. keyring 3.6 reports every other Secret Service failure as
/// `PlatformFailure` too (a D-Bus `AccessDenied`, a broken reply), and those are refusals or
/// faults of a store that exists, so they're errors, never a plaintext fallback.
/// `NoStorageAccess` (locked, refused) and the entry-level variants never count. On macOS and
/// Windows the store always exists, so nothing counts there.
fn no_keychain_service(err: &keyring::Error) -> bool {
    match err {
        keyring::Error::PlatformFailure(inner) => service_absent(inner.as_ref()),
        _ => false,
    }
}

/// D-Bus error names that mean nobody provides the Secret Service, or there's no session bus to
/// ask (`NoServer`, `FileNotFound`, `BadAddress`: no bus at that address). Not `NotSupported`
/// in general: a running Secret Service answers that when it rejects the session algorithm.
#[cfg(target_os = "linux")]
const SERVICE_ABSENT_NAMES: &[&str] = &[
    "org.freedesktop.DBus.Error.ServiceUnknown",
    "org.freedesktop.DBus.Error.NameHasNoOwner",
    "org.freedesktop.DBus.Error.NoServer",
    "org.freedesktop.DBus.Error.FileNotFound",
    "org.freedesktop.DBus.Error.BadAddress",
];

/// Whether a D-Bus error says there's no session bus to connect to: one of the names above,
/// or libdbus's `NotSupported` for "Unable to autolaunch a dbus-daemon" (a headless box with no
/// session bus), which happens while connecting, before any Secret Service could answer.
#[cfg(target_os = "linux")]
fn dbus_says_absent(name: Option<&str>, message: Option<&str>) -> bool {
    match name {
        Some("org.freedesktop.DBus.Error.NotSupported") => {
            message.is_some_and(|m| m.contains("autolaunch"))
        }
        Some(n) => SERVICE_ABSENT_NAMES.contains(&n),
        None => false,
    }
}

/// Whether the error keyring wrapped in a `PlatformFailure` says there's no Secret Service.
fn service_absent(inner: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    #[cfg(test)]
    if inner.is::<tests::NoServiceForTest>() {
        return true;
    }
    #[cfg(target_os = "linux")]
    match inner.downcast_ref::<dbus_secret_service::Error>() {
        Some(dbus_secret_service::Error::Dbus(e)) => {
            return dbus_says_absent(e.name(), e.message());
        }
        // "No D-Bus session or Secret Service provider found" (unused in 4.1, but that's
        // what it means).
        Some(dbus_secret_service::Error::Unavailable) => return true,
        _ => {}
    }
    let _ = inner;
    false
}

#[derive(Debug, PartialEq)]
enum KeychainCopy {
    /// No entry.
    Gone,
    /// The store's service itself fails, so it can't hold one Silo could read.
    NoService,
    /// There may be one: it's readable, or the store is locked or the entry malformed.
    Unknown,
}

/// Whether the keychain may still hold a copy of this entry.
fn keychain_copy(e: &keyring::Entry) -> KeychainCopy {
    match e.get_password() {
        Err(keyring::Error::NoEntry) => KeychainCopy::Gone,
        Err(err) if no_keychain_service(&err) => KeychainCopy::NoService,
        _ => KeychainCopy::Unknown,
    }
}

/// Delete `key`'s plaintext SQLite fallback row, and when there was one, VACUUM so the old
/// value isn't left carve-able in the database's free pages (as the keychain migration in
/// `get` does). No row, no VACUUM: saving a token doesn't rewrite the database every time.
/// With the row gone, a revocation naming it has nothing left to guard (best effort).
fn clear_fallback(conn: &Connection, key: &str) -> Result<(), String> {
    let deleted = conn
        .execute("DELETE FROM app_setting WHERE key = ?1", [key])
        .map_err(|e| e.to_string())?;
    if deleted > 0 {
        let _ = set_revoked(conn, key, None);
        db::vacuum(conn);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the database and its WAL, where a deleted row can linger.
    fn file_bytes(db: &std::path::Path) -> Vec<u8> {
        let mut b = std::fs::read(db).unwrap_or_default();
        let wal = db.with_file_name("silo.db-wal");
        b.extend(std::fs::read(wal).unwrap_or_default());
        b
    }

    fn contains(hay: &[u8], needle: &[u8]) -> bool {
        hay.windows(needle.len()).any(|w| w == needle)
    }

    /// Clearing the plaintext fallback leaves no copy of the token in the file. Uses its own
    /// key and never the keychain (`clear_fallback` is the SQLite half only).
    #[test]
    fn clearing_the_fallback_leaves_no_copy_in_the_file() {
        let dir = std::env::temp_dir().join(format!("silo_secret_vacuum_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("silo.db");
        let conn = db::open(&path).unwrap();
        let token = "gho_SiloTestTokenThatMustNotLinger0123456789";
        db::set_app_setting(&conn, "silo_test_secret", Some(token)).unwrap();
        assert!(contains(&file_bytes(&path), token.as_bytes()));

        clear_fallback(&conn, "silo_test_secret").unwrap();
        assert_eq!(db::get_app_setting(&conn, "silo_test_secret"), None);
        assert!(
            !contains(&file_bytes(&path), token.as_bytes()),
            "the deleted token is still in the database file"
        );
        // Nothing to clear is fine (and doesn't VACUUM).
        clear_fallback(&conn, "silo_test_secret").unwrap();
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── Keychain error handling, against a scripted fake store (never the real keychain) ──

    use keyring::credential::CredentialApi;
    use std::sync::{Arc, Mutex};

    #[derive(Clone, Copy)]
    enum Fail {
        /// A platform failure that shows there's no keychain service.
        Platform,
        /// A platform failure from a service that exists (denied by policy, a broken reply).
        Refused,
        /// Locked, or access denied.
        NoAccess,
        /// A problem with this one entry.
        Invalid,
    }

    /// Stands in, in tests on every platform, for the D-Bus "no Secret Service" errors that
    /// `service_absent` recognizes on Linux.
    #[derive(Debug)]
    pub(super) struct NoServiceForTest;

    impl std::fmt::Display for NoServiceForTest {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("fake: no Secret Service")
        }
    }

    impl std::error::Error for NoServiceForTest {}

    fn err(f: Fail) -> keyring::Error {
        match f {
            Fail::Platform => keyring::Error::PlatformFailure(Box::new(NoServiceForTest)),
            Fail::Refused => keyring::Error::PlatformFailure("fake: access denied".into()),
            Fail::NoAccess => keyring::Error::NoStorageAccess("fake locked".into()),
            Fail::Invalid => keyring::Error::Invalid("fake".into(), "invalid".into()),
        }
    }

    #[derive(Default)]
    struct FakeState {
        stored: Option<String>,
        set_fails: Option<Fail>,
        get_fails: Option<Fail>,
        delete_fails: Option<Fail>,
        /// Writes store the secret, then report this failure anyway (a reply lost on the way).
        set_stores_then_fails: Option<Fail>,
        /// Deletes report success but keep the secret (keyring 3.6 on macOS).
        delete_lies: bool,
        /// The next write says so on the first channel, then waits for the second.
        pause: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
    }

    /// A keychain entry whose failures stay until the test changes them.
    #[derive(Clone, Default)]
    struct FakeKeychain(Arc<Mutex<FakeState>>);

    impl CredentialApi for FakeKeychain {
        fn set_secret(&self, secret: &[u8]) -> keyring::Result<()> {
            let pause = self.0.lock().unwrap().pause.take();
            if let Some((entered, go)) = pause {
                entered.send(()).unwrap();
                go.recv().unwrap();
            }
            let mut s = self.0.lock().unwrap();
            if let Some(f) = s.set_fails {
                return Err(err(f));
            }
            s.stored = Some(String::from_utf8(secret.to_vec()).unwrap());
            match s.set_stores_then_fails {
                Some(f) => Err(err(f)),
                None => Ok(()),
            }
        }
        fn get_secret(&self) -> keyring::Result<Vec<u8>> {
            let s = self.0.lock().unwrap();
            if let Some(f) = s.get_fails {
                return Err(err(f));
            }
            s.stored
                .clone()
                .map(String::into_bytes)
                .ok_or(keyring::Error::NoEntry)
        }
        fn delete_credential(&self) -> keyring::Result<()> {
            let mut s = self.0.lock().unwrap();
            if let Some(f) = s.delete_fails {
                return Err(err(f));
            }
            if s.delete_lies {
                return Ok(());
            }
            s.stored.take().map(|_| ()).ok_or(keyring::Error::NoEntry)
        }
        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    impl FakeKeychain {
        fn with(f: impl FnOnce(&mut FakeState)) -> Self {
            let k = Self::default();
            f(&mut k.0.lock().unwrap());
            k
        }
        fn entry(&self) -> keyring::Entry {
            keyring::Entry::new_with_credential(Box::new(self.clone()))
        }
        fn stored(&self) -> Option<String> {
            self.0.lock().unwrap().stored.clone()
        }
    }

    const KEY: &str = "silo_test_token";
    /// Stand-ins for `keychain_missing`.
    const KEYCHAIN_WORKS: fn() -> bool = || false;
    const NO_KEYCHAIN: fn() -> bool = || true;

    fn mem_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE app_setting (key TEXT PRIMARY KEY, value TEXT NOT NULL)")
            .unwrap();
        conn
    }

    fn row(conn: &Connection) -> Option<String> {
        db::get_app_setting(conn, KEY)
    }

    #[test]
    fn saving_goes_to_the_keychain_and_clears_an_old_row() {
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("old")).unwrap();
        let k = FakeKeychain::default();
        set_with(&conn, KEY, Some("new"), Some(&k.entry()), KEYCHAIN_WORKS).unwrap();
        assert_eq!(k.stored().as_deref(), Some("new"));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn a_failed_save_on_a_working_keychain_is_returned_not_stored_in_sqlite() {
        for f in [Fail::Platform, Fail::NoAccess] {
            let conn = mem_db();
            let k = FakeKeychain::with(|s| {
                s.stored = Some("other-account".into());
                s.set_fails = Some(f);
            });
            let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), KEYCHAIN_WORKS);
            assert_eq!(e, Err(SAVE_FAILED.to_string()));
            assert_eq!(row(&conn), None, "no silent plaintext fallback");
            assert_eq!(k.stored().as_deref(), Some("other-account"));
        }
    }

    #[test]
    fn an_entry_error_is_returned_even_without_a_working_keychain() {
        let conn = mem_db();
        let k = FakeKeychain::with(|s| s.set_fails = Some(Fail::Invalid));
        let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(SAVE_FAILED.to_string()));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn no_keychain_at_all_falls_back_to_sqlite() {
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::Platform);
        });
        set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN).unwrap();
        assert_eq!(row(&conn).as_deref(), Some("new"));
        // No keychain entry at all (demo profile) stores the row too.
        let conn = mem_db();
        set_with(&conn, KEY, Some("demo"), None, NO_KEYCHAIN).unwrap();
        assert_eq!(row(&conn).as_deref(), Some("demo"));
    }

    #[test]
    fn a_locked_keychain_is_an_error_never_a_fallback() {
        // Locked: every call fails with NoStorageAccess, the probe too. The user can unlock it,
        // so the token must not go to SQLite.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::NoAccess);
            s.get_fails = Some(Fail::NoAccess);
            s.delete_fails = Some(Fail::NoAccess);
        });
        let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(SAVE_FAILED.to_string()));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn a_store_that_answers_is_an_error_not_a_fallback() {
        // The write fails at the platform level, but a read answers (a token, or no entry): the
        // store exists. The older copy stays, so the account still connected keeps working.
        for stored in [Some("other-account"), None] {
            let conn = mem_db();
            let k = FakeKeychain::with(|s| {
                s.stored = stored.map(String::from);
                s.set_fails = Some(Fail::Platform);
            });
            let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN);
            assert_eq!(e, Err(SAVE_FAILED.to_string()));
            assert_eq!(row(&conn), None);
            assert_eq!(k.stored().as_deref(), stored);
        }
        // A store the probe finds (working, or locked) isn't missing either.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("other-account".into());
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::Platform);
        });
        let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), KEYCHAIN_WORKS);
        assert_eq!(e, Err(SAVE_FAILED.to_string()));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn a_store_that_comes_back_mid_save_gets_the_token() {
        // Write and read fail at the platform level, then the store is back: the retried
        // write overwrites the old token in place, nothing goes into SQLite.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
        });
        let entry = k.entry();
        let recover = || {
            let mut s = k.0.lock().unwrap();
            s.set_fails = None;
            s.get_fails = None;
            true // the probe ran while it was still down
        };
        set_with(&conn, KEY, Some("new"), Some(&entry), recover).unwrap();
        assert_eq!(k.stored().as_deref(), Some("new"));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn a_rejected_fallback_write_leaves_the_old_keychain_token_usable() {
        // Account A in the keychain, then nothing answers; storing B in SQLite fails.
        let conn = mem_db();
        conn.execute_batch(
            "CREATE TRIGGER no_insert BEFORE INSERT ON app_setting \
             BEGIN SELECT RAISE(FAIL, 'read-only'); END",
        )
        .unwrap();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::Platform);
        });
        assert!(set_with(&conn, KEY, Some("account-b"), Some(&k.entry()), NO_KEYCHAIN).is_err());
        // The service comes back: A is still there and still used.
        *k.0.lock().unwrap() = FakeState {
            stored: Some("account-a".into()),
            ..FakeState::default()
        };
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("account-a")
        );
    }

    #[test]
    fn a_failed_replacement_after_recovery_leaves_the_connected_token() {
        // Account A in the keychain. Connecting B: write, read and probe fail at the platform
        // level, then the store recovers but refuses B. A must still be there and in use.
        for retry in [Fail::NoAccess, Fail::Invalid] {
            let conn = mem_db();
            let k = FakeKeychain::with(|s| {
                s.stored = Some("account-a".into());
                s.set_fails = Some(Fail::Platform);
                s.get_fails = Some(Fail::Platform);
            });
            let entry = k.entry();
            let recover = || {
                let mut s = k.0.lock().unwrap();
                s.get_fails = None;
                s.set_fails = Some(retry);
                true // the probe ran while it was still down
            };
            let e = set_with(&conn, KEY, Some("account-b"), Some(&entry), recover);
            assert_eq!(e, Err(SAVE_FAILED.to_string()));
            assert_eq!(k.stored().as_deref(), Some("account-a"));
            assert_eq!(row(&conn), None);
            assert_eq!(
                get_with(&conn, KEY, Some(&entry)).as_deref(),
                Some("account-a")
            );
        }
        // Still down on the retry: B goes to SQLite (and wins), A is left untouched.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
        });
        set_with(&conn, KEY, Some("account-b"), Some(&k.entry()), NO_KEYCHAIN).unwrap();
        assert_eq!(k.stored().as_deref(), Some("account-a"));
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("account-b")
        );
    }

    #[test]
    fn disconnect_succeeds_when_the_keychain_copy_is_gone() {
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("legacy")).unwrap();
        let k = FakeKeychain::with(|s| s.stored = Some("tok".into()));
        set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS).unwrap();
        assert_eq!(k.stored(), None);
        assert_eq!(row(&conn), None);
        // Nothing stored (NoEntry) is success too.
        set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS).unwrap();
    }

    #[test]
    fn disconnect_reports_a_token_the_keychain_kept() {
        for (delete, get) in [
            (Fail::Platform, None),
            (Fail::NoAccess, Some(Fail::NoAccess)),
            (Fail::Invalid, None),
        ] {
            let conn = mem_db();
            db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
            let k = FakeKeychain::with(|s| {
                s.stored = Some("tok".into());
                s.delete_fails = Some(delete);
                s.get_fails = get;
            });
            let e = set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS);
            assert_eq!(e, Err(REMOVE_FAILED.to_string()));
            assert_eq!(row(&conn), None, "the SQLite copy goes either way");
            assert_eq!(k.stored().as_deref(), Some("tok"));
            // Silo stops using the copy it couldn't delete...
            k.0.lock().unwrap().get_fails = None;
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            assert_eq!(k.stored().as_deref(), Some("tok"));
            // ...and removes it once the store allows.
            k.0.lock().unwrap().delete_fails = None;
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            assert_eq!(k.stored(), None);
            assert!(!keychain_distrusted(&conn, KEY));
        }
    }

    #[test]
    fn a_failed_disconnect_never_switches_to_another_accounts_token() {
        // Left by an older build: account A in the keychain, account B's token in SQLite after
        // a silent fallback. The keychain won't write or delete, and is locked or reads fine.
        for get in [None, Some(Fail::NoAccess)] {
            let conn = mem_db();
            db::set_app_setting(&conn, KEY, Some("account-b")).unwrap();
            let k = FakeKeychain::with(|s| {
                s.stored = Some("account-a".into());
                s.set_fails = Some(Fail::Platform);
                s.delete_fails = Some(Fail::NoAccess);
                s.get_fails = get;
            });
            assert_eq!(
                get_with(&conn, KEY, Some(&k.entry())).as_deref(),
                Some("account-b")
            );
            let e = set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN);
            assert_eq!(e, Err(REMOVE_FAILED.to_string()));
            // The store unlocks: account A's token still isn't used.
            k.0.lock().unwrap().get_fails = None;
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
        }
    }

    #[test]
    fn an_unseen_keychain_copy_never_comes_back_after_the_service_returns() {
        // Account A in the keychain; then its service fails on every call. Connecting B falls
        // back to SQLite, and disconnecting succeeds (B lived in SQLite).
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::Platform);
        });
        set_with(&conn, KEY, Some("account-b"), Some(&k.entry()), NO_KEYCHAIN).unwrap();
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("account-b")
        );
        set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN).unwrap();
        // The service comes back: A isn't used, and is removed.
        *k.0.lock().unwrap() = FakeState {
            stored: Some("account-a".into()),
            ..FakeState::default()
        };
        assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
        assert_eq!(k.stored(), None);
        // A later connect through the keychain is trusted again.
        set_with(
            &conn,
            KEY,
            Some("account-c"),
            Some(&k.entry()),
            KEYCHAIN_WORKS,
        )
        .unwrap();
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("account-c")
        );
    }

    #[test]
    fn a_failed_marker_clear_never_costs_the_promoted_token() {
        // A fallback row with the marker set; the keychain works again, but the database won't
        // take the marker change. The row must stay, or the next read would delete the token.
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        set_distrust(&conn, KEY, true).unwrap();
        let sql = format!(
            "CREATE TRIGGER no_unmark BEFORE DELETE ON app_setting WHEN old.key = '{}' BEGIN SELECT RAISE(FAIL, 'busy'); END",
            distrust_key(KEY)
        );
        conn.execute_batch(&sql).unwrap();
        let k = FakeKeychain::default();
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("row")
        );
        assert_eq!(row(&conn).as_deref(), Some("row"));
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("row")
        );
    }

    #[test]
    fn clear_says_whether_the_keychain_copy_went() {
        let conn = mem_db();
        let k = FakeKeychain::with(|s| s.stored = Some("tok".into()));
        assert_eq!(
            clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS),
            Ok(Removal::default())
        );
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_fails = Some(Fail::NoAccess);
        });
        assert_eq!(
            clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS),
            Ok(Removal {
                keychain_kept: true,
                row_kept: false
            })
        );
        assert_eq!(
            clear_with(&conn, KEY, None, KEYCHAIN_WORKS),
            Ok(Removal::default())
        );
        // A row the database won't drop is revoked and reported; one it won't even revoke is
        // an error.
        db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
        refuse_row_delete(&conn);
        assert_eq!(
            clear_with(&conn, KEY, None, KEYCHAIN_WORKS),
            Ok(Removal {
                keychain_kept: false,
                row_kept: true
            })
        );
        let sql = format!(
            "CREATE TRIGGER no_revoke BEFORE INSERT ON app_setting WHEN new.key = '{}' \
             BEGIN SELECT RAISE(FAIL, 'busy'); END",
            revoked_key(KEY)
        );
        conn.execute_batch(&sql).unwrap();
        set_revoked(&conn, KEY, None).unwrap();
        assert!(clear_with(&conn, KEY, None, KEYCHAIN_WORKS).is_err());
    }

    #[test]
    fn a_read_moving_a_row_into_the_keychain_cant_outlast_a_disconnect() {
        // The read pauses inside its keychain write; a disconnect starts meanwhile. Whatever
        // the timing, nothing is left once both are done.
        let dir = std::env::temp_dir().join(format!("silo_secret_race_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("silo.db");
        let conn = db::open(&path).unwrap();
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel::<()>();
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();
        let k = FakeKeychain::default();
        k.0.lock().unwrap().pause = Some((entered_tx, go_rx));

        let reader = {
            let (k, path) = (k.clone(), path.clone());
            std::thread::spawn(move || {
                let conn = db::open(&path).unwrap();
                get_with(&conn, KEY, Some(&k.entry()))
            })
        };
        entered_rx.recv().unwrap(); // the read is inside set_password now
        let remover = {
            let (k, path) = (k.clone(), path.clone());
            std::thread::spawn(move || {
                let conn = db::open(&path).unwrap();
                set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS)
            })
        };
        std::thread::sleep(std::time::Duration::from_millis(50));
        go_tx.send(()).unwrap();
        assert_eq!(reader.join().unwrap().as_deref(), Some("row"));
        remover.join().unwrap().unwrap();
        assert_eq!(k.stored(), None);
        assert_eq!(row(&conn), None);
        drop(conn);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disconnect_on_a_locked_keychain_without_a_row_is_reported() {
        // The token was in the keychain (no row) and the store is locked now, so it can't be
        // confirmed gone.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_fails = Some(Fail::NoAccess);
            s.get_fails = Some(Fail::NoAccess);
        });
        let e = set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(REMOVE_FAILED.to_string()));
        // A SQLite row beside it doesn't make a locked store count as "no keychain".
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        let e = set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(REMOVE_FAILED.to_string()));
        // Nor does a failing service when the probe says the keychain works.
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
        });
        let e = set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS);
        assert_eq!(e, Err(REMOVE_FAILED.to_string()));
    }

    #[test]
    fn disconnect_without_any_keychain_clears_the_sqlite_token() {
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::Platform);
        });
        set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN).unwrap();
        assert_eq!(row(&conn), None);
        // Demo profile: no keychain entry.
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        set_with(&conn, KEY, None, None, NO_KEYCHAIN).unwrap();
        assert_eq!(row(&conn), None);
    }

    /// Make deleting the test key's row fail, as a busy or read-only database would.
    fn refuse_row_delete(conn: &Connection) {
        let sql = format!(
            "CREATE TRIGGER no_row_delete BEFORE DELETE ON app_setting WHEN old.key = '{KEY}' \
             BEGIN SELECT RAISE(FAIL, 'busy'); END"
        );
        conn.execute_batch(&sql).unwrap();
    }

    fn allow_row_delete(conn: &Connection) {
        conn.execute_batch("DROP TRIGGER no_row_delete").unwrap();
    }

    #[test]
    fn a_row_a_failed_disconnect_left_behind_is_never_read_back() {
        // Both deletes fail: the keychain is locked and the database won't drop the row.
        for get in [None, Some(Fail::NoAccess)] {
            let conn = mem_db();
            db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
            refuse_row_delete(&conn);
            let k = FakeKeychain::with(|s| {
                s.stored = Some("tok".into());
                s.set_fails = Some(Fail::NoAccess);
                s.delete_fails = Some(Fail::NoAccess);
                s.get_fails = get;
            });
            assert_eq!(
                clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS),
                Ok(Removal {
                    keychain_kept: true,
                    row_kept: true
                })
            );
            assert_eq!(
                row(&conn).as_deref(),
                Some("tok"),
                "the delete really failed"
            );
            // Neither copy is used, before or after the store unlocks.
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            k.0.lock().unwrap().get_fails = None;
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            assert_eq!(get_with(&conn, KEY, None), None, "nor without a keychain");
            // Both are removed once the stores allow.
            allow_row_delete(&conn);
            k.0.lock().unwrap().delete_fails = None;
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            assert_eq!(row(&conn), None);
            assert_eq!(k.stored(), None);
            assert_eq!(db::get_app_setting(&conn, &revoked_key(KEY)), None);
        }
    }

    #[test]
    fn a_reconnect_through_the_fallback_works_after_a_failed_disconnect() {
        // No keychain at all; the token lives in SQLite and the row can't be deleted.
        let no_service = || {
            FakeKeychain::with(|s| {
                s.set_fails = Some(Fail::Platform);
                s.get_fails = Some(Fail::Platform);
                s.delete_fails = Some(Fail::Platform);
            })
        };
        for reconnect in ["new-token", "tok"] {
            let conn = mem_db();
            let k = no_service();
            set_with(&conn, KEY, Some("tok"), Some(&k.entry()), NO_KEYCHAIN).unwrap();
            refuse_row_delete(&conn);
            let e = set_with(&conn, KEY, None, Some(&k.entry()), NO_KEYCHAIN);
            assert_eq!(e, Err(REMOVE_ROW_FAILED.to_string()));
            assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None);
            // Connecting again stores a row (a new token, or the same one pasted again): used.
            set_with(&conn, KEY, Some(reconnect), Some(&k.entry()), NO_KEYCHAIN).unwrap();
            assert_eq!(
                get_with(&conn, KEY, Some(&k.entry())).as_deref(),
                Some(reconnect)
            );
            // Demo profile (no keychain entry) too.
            set_with(&conn, KEY, None, None, NO_KEYCHAIN).unwrap_err();
            assert_eq!(get_with(&conn, KEY, None), None);
            set_with(&conn, KEY, Some(reconnect), None, NO_KEYCHAIN).unwrap();
            assert_eq!(get_with(&conn, KEY, None).as_deref(), Some(reconnect));
        }
    }

    #[test]
    fn a_keychain_reconnect_wins_over_a_revoked_row() {
        // A failed disconnect left a revoked row; the next connect goes to the keychain, whose
        // own row clear fails too. The new keychain token is used, the old row never.
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("old")).unwrap();
        refuse_row_delete(&conn);
        let k = FakeKeychain::default();
        assert_eq!(
            set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS),
            Err(REMOVE_ROW_FAILED.to_string())
        );
        assert!(set_with(&conn, KEY, Some("new"), Some(&k.entry()), KEYCHAIN_WORKS).is_err());
        assert_eq!(k.stored().as_deref(), Some("new"));
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("new")
        );
    }

    #[test]
    fn a_failed_save_says_whether_the_token_changed() {
        // Refused before anything was written: unchanged, the old token is still stored.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_fails = Some(Fail::NoAccess);
        });
        let e = store_with(&conn, KEY, "account-b", Some(&k.entry()), KEYCHAIN_WORKS);
        assert_eq!(e, Err(SaveError::unchanged(SAVE_FAILED)));
        assert_eq!(k.stored().as_deref(), Some("account-a"));
        // The keychain took it, but the database step after it failed: changed.
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("old-row")).unwrap();
        refuse_row_delete(&conn);
        let k = FakeKeychain::default();
        let e = store_with(&conn, KEY, "account-b", Some(&k.entry()), KEYCHAIN_WORKS);
        assert!(matches!(e, Err(SaveError { changed: true, .. })));
        assert_eq!(k.stored().as_deref(), Some("account-b"));
        // A write that reports a platform failure but stored it anyway: counted as changed, so
        // the session signs out instead of using B under A's name.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_stores_then_fails = Some(Fail::Platform);
        });
        let e = store_with(&conn, KEY, "account-b", Some(&k.entry()), KEYCHAIN_WORKS);
        assert!(matches!(e, Err(SaveError { changed: true, .. })));
        assert_eq!(k.stored().as_deref(), Some("account-b"));
        // The same with a platform failure that isn't "no service" (a lost D-Bus reply): still
        // a possible change, never "unchanged".
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("account-a".into());
            s.set_stores_then_fails = Some(Fail::Refused);
        });
        for missing in [KEYCHAIN_WORKS, NO_KEYCHAIN] {
            let e = store_with(&conn, KEY, "account-b", Some(&k.entry()), missing);
            assert!(matches!(e, Err(SaveError { changed: true, .. })));
            assert_eq!(row(&conn), None, "and never in plaintext");
        }
        // The fallback write itself refused: unchanged.
        let conn = mem_db();
        conn.execute_batch(
            "CREATE TRIGGER no_insert BEFORE INSERT ON app_setting \
             BEGIN SELECT RAISE(FAIL, 'read-only'); END",
        )
        .unwrap();
        let e = store_with(&conn, KEY, "account-b", None, NO_KEYCHAIN);
        assert!(matches!(e, Err(SaveError { changed: false, .. })));
    }

    #[test]
    fn a_disconnect_that_cant_read_the_row_claims_nothing() {
        // A database whose app_setting table can't be read: the disconnect errs (the caller
        // stays signed out) instead of reporting the row gone or revoked.
        let broken = Connection::open_in_memory().unwrap();
        let k = FakeKeychain::with(|s| s.stored = Some("tok".into()));
        assert!(clear_with(&broken, KEY, Some(&k.entry()), KEYCHAIN_WORKS).is_err());
        assert!(clear_with(&broken, KEY, None, KEYCHAIN_WORKS).is_err());
        // A row that wasn't revoked and won't delete is an error, never "kept, revoked".
        let conn = mem_db();
        refuse_row_delete(&conn);
        db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
        assert!(drop_row(&conn, KEY, None).is_err());
        assert_eq!(drop_row(&conn, KEY, Some("tok")), Ok(true));
    }

    #[test]
    fn a_sweep_removes_leftovers_but_never_a_current_token() {
        // A disconnect left a distrusted keychain copy (locked) and a revoked row (busy).
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
        refuse_row_delete(&conn);
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_fails = Some(Fail::NoAccess);
            s.get_fails = Some(Fail::NoAccess);
        });
        assert!(clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS).is_ok());
        // Still locked and busy: nothing changes.
        sweep_with(&conn, KEY, Some(&k.entry()));
        assert_eq!(k.stored().as_deref(), Some("tok"));
        assert_eq!(row(&conn).as_deref(), Some("tok"));
        // Both stores allow it again: both copies go, and the marks with them.
        allow_row_delete(&conn);
        {
            let mut s = k.0.lock().unwrap();
            s.delete_fails = None;
            s.get_fails = None;
        }
        sweep_with(&conn, KEY, Some(&k.entry()));
        assert_eq!(k.stored(), None);
        assert_eq!(row(&conn), None);
        assert!(!keychain_distrusted(&conn, KEY));
        assert_eq!(db::get_app_setting(&conn, &revoked_key(KEY)), None);
        // A token saved normally is neither marked, so a sweep leaves it alone.
        set_with(&conn, KEY, Some("new"), Some(&k.entry()), KEYCHAIN_WORKS).unwrap();
        sweep_with(&conn, KEY, Some(&k.entry()));
        assert_eq!(k.stored().as_deref(), Some("new"));
        let conn = mem_db();
        set_with(&conn, KEY, Some("row"), None, NO_KEYCHAIN).unwrap();
        sweep_with(&conn, KEY, None);
        assert_eq!(row(&conn).as_deref(), Some("row"));
    }

    #[test]
    fn a_delete_that_reports_success_but_keeps_the_token_isnt_trusted() {
        // macOS: keyring says the delete worked, but the entry is still there.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_lies = true;
        });
        let e = set_with(&conn, KEY, None, Some(&k.entry()), KEYCHAIN_WORKS);
        assert_eq!(e, Err(REMOVE_FAILED.to_string()));
        assert!(keychain_distrusted(&conn, KEY));
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())),
            None,
            "never read back"
        );
        sweep_with(&conn, KEY, Some(&k.entry()));
        assert!(
            keychain_distrusted(&conn, KEY),
            "a sweep doesn't believe it either"
        );
        // Once a delete really works, the copy and its mark go.
        k.0.lock().unwrap().delete_lies = false;
        sweep_with(&conn, KEY, Some(&k.entry()));
        assert_eq!(k.stored(), None);
        assert!(!keychain_distrusted(&conn, KEY));
    }

    #[test]
    fn macos_and_windows_never_count_as_having_no_keychain() {
        if cfg!(any(target_os = "macos", target_os = "windows")) {
            assert!(!keychain_missing());
        }
    }

    #[test]
    fn a_service_that_refuses_at_the_platform_level_never_gets_a_plaintext_fallback() {
        // Every call fails with a PlatformFailure that isn't "no service" (e.g. D-Bus
        // AccessDenied), and the probe agrees the store is missing: still an error.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::Refused);
            s.get_fails = Some(Fail::Refused);
            s.delete_fails = Some(Fail::Refused);
        });
        let e = store_with(&conn, KEY, "tok", Some(&k.entry()), NO_KEYCHAIN);
        assert!(matches!(e, Err(SaveError { ref message, .. }) if message == SAVE_FAILED));
        assert_eq!(row(&conn), None, "never in plaintext");
        // A Disconnect there can't count the keychain copy as gone either.
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        assert_eq!(
            clear_with(&conn, KEY, Some(&k.entry()), NO_KEYCHAIN),
            Ok(Removal {
                keychain_kept: true,
                row_kept: false
            })
        );
    }

    #[test]
    fn only_a_missing_service_counts_as_no_keychain() {
        let absent = keyring::Error::PlatformFailure(Box::new(NoServiceForTest));
        assert!(no_keychain_service(&absent));
        assert!(!no_keychain_service(&keyring::Error::PlatformFailure(
            "some other backend failure".into()
        )));
        assert!(!no_keychain_service(&keyring::Error::NoStorageAccess(
            "locked".into()
        )));
        assert!(!no_keychain_service(&keyring::Error::NoEntry));
    }

    /// The real error types keyring's Linux backend wraps (no D-Bus connection is made).
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_dbus_errors_are_classified_by_name() {
        fn dbus_err(name: &str) -> keyring::Error {
            dbus_err_msg(name, "test")
        }
        fn dbus_err_msg(name: &str, message: &str) -> keyring::Error {
            keyring::Error::PlatformFailure(Box::new(dbus_secret_service::Error::Dbus(
                dbus::Error::new_custom(name, message),
            )))
        }
        // NotSupported: only libdbus's "can't autolaunch a session bus", never a Secret
        // Service rejecting the session algorithm.
        assert!(no_keychain_service(&dbus_err_msg(
            "org.freedesktop.DBus.Error.NotSupported",
            "Unable to autolaunch a dbus-daemon without a $DISPLAY for X11"
        )));
        assert!(!no_keychain_service(&dbus_err_msg(
            "org.freedesktop.DBus.Error.NotSupported",
            "Algorithm dh-ietf1024-sha256-aes128-cbc-pkcs7 is not supported"
        )));
        for name in SERVICE_ABSENT_NAMES {
            assert!(
                no_keychain_service(&dbus_err(name)),
                "{name} means no service"
            );
        }
        assert!(no_keychain_service(&keyring::Error::PlatformFailure(
            Box::new(dbus_secret_service::Error::Unavailable)
        )));
        for name in [
            "org.freedesktop.DBus.Error.AccessDenied",
            "org.freedesktop.DBus.Error.AuthFailed",
            "org.freedesktop.DBus.Error.NoReply",
            "org.freedesktop.DBus.Error.Timeout",
            "org.freedesktop.DBus.Error.Failed",
            "org.freedesktop.Secret.Error.IsLocked",
        ] {
            assert!(!no_keychain_service(&dbus_err(name)), "{name} is a refusal");
        }
        // The crate's other errors aren't evidence of absence either.
        for other in [
            dbus_secret_service::Error::Parse,
            dbus_secret_service::Error::Locked,
            dbus_secret_service::Error::Path("bad".into()),
        ] {
            let e = keyring::Error::PlatformFailure(Box::new(other));
            assert!(!no_keychain_service(&e));
        }
    }

    /// Put a value in the test key's row that can't be read as text, as a read error would.
    fn unreadable_row(conn: &Connection) {
        conn.execute(
            "INSERT INTO app_setting(key, value) VALUES (?1, x'00ff') \
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [KEY],
        )
        .unwrap();
    }

    #[test]
    fn an_unreadable_row_never_lets_an_older_keychain_token_through() {
        // B signed in through the fallback (its row can't be read just now); the keychain
        // still holds A, with no distrust marker.
        let conn = mem_db();
        unreadable_row(&conn);
        let k = FakeKeychain::with(|s| s.stored = Some("account-a".into()));
        assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None, "never A");
        assert_eq!(k.stored().as_deref(), Some("account-a"), "nothing deleted");
    }

    #[test]
    fn a_disconnect_deletes_the_keychain_copy_even_when_the_row_cant_be_read() {
        let conn = mem_db();
        unreadable_row(&conn);
        let k = FakeKeychain::with(|s| s.stored = Some("tok".into()));
        assert!(clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS).is_err());
        assert_eq!(k.stored(), None, "the healthy keychain's copy is gone");
    }

    #[test]
    fn an_unreadable_distrust_marker_keeps_the_keychain_copy_unused() {
        let broken = Connection::open_in_memory().unwrap();
        let k = FakeKeychain::with(|s| s.stored = Some("tok".into()));
        assert!(keychain_distrusted(&broken, KEY));
        assert_eq!(get_with(&broken, KEY, Some(&k.entry())), None);
        assert_eq!(k.stored().as_deref(), Some("tok"), "not deleted on a guess");
    }

    #[test]
    fn an_unreadable_revocation_counts_as_revoked() {
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("tok")).unwrap();
        assert_eq!(row_revoked(&conn, KEY, "tok"), Ok(false));
        set_revoked(&conn, KEY, Some("tok")).unwrap();
        assert_eq!(row_revoked(&conn, KEY, "tok"), Ok(true));
        assert_eq!(row_revoked(&conn, KEY, "other"), Ok(false));
        // The marker holds a fingerprint, never the token.
        let marker = db::get_app_setting(&conn, &revoked_key(KEY)).unwrap();
        assert!(!marker.contains("tok"));
        // A database Silo can't read the marker from: unknown, not "not revoked".
        let broken = Connection::open_in_memory().unwrap();
        assert!(row_revoked(&broken, KEY, "tok").is_err());
    }

    #[test]
    fn an_unreadable_revocation_neither_uses_nor_deletes_the_row() {
        // Account A's token in the keychain, B's (newer) in the SQLite fallback. The revocation
        // marker holds something that can't be read as text.
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("account-b")).unwrap();
        conn.execute(
            "INSERT INTO app_setting(key, value) VALUES (?1, x'00ff')",
            [revoked_key(KEY)],
        )
        .unwrap();
        let k = FakeKeychain::with(|s| s.stored = Some("account-a".into()));
        assert_eq!(get_with(&conn, KEY, Some(&k.entry())), None, "never A");
        assert_eq!(row(&conn).as_deref(), Some("account-b"), "B isn't deleted");
        assert_eq!(k.stored().as_deref(), Some("account-a"));
        // Without a keychain too.
        assert_eq!(get_with(&conn, KEY, None), None);
        assert_eq!(row(&conn).as_deref(), Some("account-b"));
        // Connecting again through the fallback lifts it (replacing the unreadable value).
        set_with(&conn, KEY, Some("account-b"), None, NO_KEYCHAIN).unwrap();
        assert_eq!(get_with(&conn, KEY, None).as_deref(), Some("account-b"));
    }

    #[test]
    fn reading_prefers_the_newer_sqlite_row_and_moves_it_into_the_keychain() {
        let conn = mem_db();
        db::set_app_setting(&conn, KEY, Some("row")).unwrap();
        // The keychain can't take it yet: the row is returned and kept.
        let k = FakeKeychain::with(|s| {
            s.stored = Some("stale".into());
            s.set_fails = Some(Fail::NoAccess);
        });
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("row")
        );
        assert_eq!(row(&conn).as_deref(), Some("row"));
        // Once it works, the row moves into the keychain, replacing the stale copy.
        k.0.lock().unwrap().set_fails = None;
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("row")
        );
        assert_eq!(row(&conn), None);
        assert_eq!(k.stored().as_deref(), Some("row"));
        // No row: the keychain answers.
        assert_eq!(
            get_with(&conn, KEY, Some(&k.entry())).as_deref(),
            Some("row")
        );
        assert_eq!(get_with(&conn, KEY, None), None);
    }
}
