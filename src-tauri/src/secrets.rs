//! Secret storage for API tokens. Prefers the OS credential store (Windows Credential
//! Manager, macOS Keychain, Linux Secret Service) so tokens never sit in plaintext
//! beside ordinary preferences. Two safety nets keep it robust:
//!
//! * **Migration** — a token written by an older build (plaintext in the `app_setting`
//!   table) is moved into the keychain the first time it's read, then cleared from SQLite.
//! * **Fallback** — on a machine with no working keychain (e.g. a headless Linux box),
//!   we fall back to the SQLite setting so the feature still works. That's no worse than
//!   before; everywhere with a keychain (all Windows/macOS, most Linux) is now secure.
//!   Only "no usable store" errors that the probe confirms lead there; any other keychain
//!   failure on save or disconnect is returned to the caller, never swallowed.
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

/// Whether this machine has a working OS keychain. When false, connecting an account falls
/// back to storing the token in the local SQLite DB (not OS-encrypted) — the UI surfaces a
/// warning so the user can decide before connecting, rather than falling back silently.
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
/// use: one a disconnect couldn't delete, or one Silo couldn't see when it fell back to SQLite.
/// Cleared once the keychain accepts a write or a delete again.
fn distrust_key(key: &str) -> String {
    format!("{key}:keychain_distrusted")
}

fn keychain_distrusted(conn: &Connection, key: &str) -> bool {
    db::get_app_setting(conn, &distrust_key(key)).is_some()
}

/// Set or clear the marker. Clearing one that isn't set writes nothing, so it can't fail.
fn set_distrust(conn: &Connection, key: &str, on: bool) -> Result<(), String> {
    if !on && !keychain_distrusted(conn, key) {
        return Ok(());
    }
    db::set_app_setting(conn, &distrust_key(key), on.then_some("1"))
}

/// Read a secret. A SQLite row wins over the keychain: every keychain write clears the row, and
/// the row is only written when the keychain can't be used, so it's never the older of the two.
/// A row is moved into the keychain once the keychain works (this is also how a token written
/// by a pre-keychain build is migrated). A distrusted keychain copy is never returned; Silo
/// deletes it as soon as the store allows.
pub fn get(conn: &Connection, key: &str) -> Option<String> {
    get_with(conn, key, entry(key).as_ref())
}

fn get_with(conn: &Connection, key: &str, keychain: Option<&keyring::Entry>) -> Option<String> {
    let _held = store_lock();
    if let Some(row) = db::get_app_setting(conn, key) {
        // Best-effort promotion into the keychain, then drop the plaintext copy AND VACUUM so
        // the token isn't left carve-able in the DB's free pages (SQLite doesn't zero deletes).
        // The marker goes first: a row dropped while it stayed would make the next read delete
        // the keychain copy just written.
        if let Some(e) = keychain {
            if e.set_password(&row).is_ok() && set_distrust(conn, key, false).is_ok() {
                let _ = db::set_app_setting(conn, key, None);
                db::vacuum(conn);
            }
        }
        return Some(row);
    }
    let e = keychain?;
    if keychain_distrusted(conn, key) {
        if matches!(e.delete_credential(), Ok(()) | Err(keyring::Error::NoEntry)) {
            let _ = set_distrust(conn, key, false);
        }
        return None;
    }
    e.get_password().ok()
}

/// Shown when the keychain refused to store a token on a machine that has one.
const SAVE_FAILED: &str = "Silo couldn't save the token in your system's credential store. \
     If it's locked, unlock it and try again.";
/// Shown when there's no usable keychain but it may hold an older token Silo couldn't remove.
const STALE_COPY: &str = "Silo couldn't save the token: your system's credential store \
     can't be used right now, and Silo couldn't clear an older token from it. \
     Unlock it and try again.";
/// Shown when disconnecting couldn't delete the keychain copy (Silo has stopped using it).
pub(crate) const REMOVE_FAILED: &str = "Silo couldn't remove the token from your system's \
     credential store. Silo won't use it, and removes it once the store allows (for example \
     after you unlock it). You can also delete the com.hllmr.silo entry there yourself.";

/// Store (`Some`) or clear (`None`) a secret. Keychain-primary; SQLite only when this machine
/// has no usable keychain at all. Any other keychain failure is returned, never swallowed.
pub fn set(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    set_with(conn, key, value, entry(key).as_ref(), keychain_available)
}

/// Forget a secret (disconnect). Silo stops using it either way. Returns false when the
/// keychain kept a copy it couldn't delete; Err only when the database couldn't be updated
/// (then nothing changed).
pub fn clear(conn: &Connection, key: &str) -> Result<bool, String> {
    let _held = store_lock();
    clear_with(conn, key, entry(key).as_ref(), keychain_available)
}

/// `set`, with the keychain entry and the "is there a working keychain?" probe passed in so
/// tests can use fakes instead of the real store.
fn set_with(
    conn: &Connection,
    key: &str,
    value: Option<&str>,
    keychain: Option<&keyring::Entry>,
    probe: impl Fn() -> bool,
) -> Result<(), String> {
    let _held = store_lock();
    match (value, keychain) {
        // Demo profile, or no keychain backend at all.
        (Some(v), None) => db::set_app_setting(conn, key, Some(v)),
        (Some(v), Some(e)) => save(conn, key, v, e, probe),
        (None, keychain) => match clear_with(conn, key, keychain, probe)? {
            true => Ok(()),
            false => Err(REMOVE_FAILED.to_string()),
        },
    }
}

/// Save to the keychain; SQLite only when there's no keychain service at all.
fn save(
    conn: &Connection,
    key: &str,
    v: &str,
    e: &keyring::Entry,
    probe: impl Fn() -> bool,
) -> Result<(), String> {
    match e.set_password(v) {
        Ok(()) => {
            set_distrust(conn, key, false)?;
            // Ensure it isn't also left behind in SQLite from a prior build.
            clear_fallback(conn, key)
        }
        // No usable keychain (e.g. a Linux box without Secret Service): keep the feature working
        // via SQLite; the keychain_available warning covers this. Only when the store fails at
        // the platform level for the write, a read and the probe: a store that answers at all
        // (an entry, "no entry", locked, access denied) exists, so its failure is an error.
        Err(err)
            if no_keychain_service(&err)
                && keychain_copy(e) == KeychainCopy::NoService
                && !probe() =>
        {
            // An older copy must go first, or a later read could pick it up for an account the
            // user no longer has connected. When the delete fails the same way, nothing answers;
            // any copy Silo can't see is distrusted until it can be deleted.
            match e.delete_credential() {
                Err(d) if no_keychain_service(&d) => {}
                Ok(()) | Err(keyring::Error::NoEntry) => {
                    log::warn!(
                        "{key}: the keychain refused the write but answered a delete: {err}"
                    );
                    return Err(SAVE_FAILED.into());
                }
                Err(d) => {
                    log::warn!("{key}: no usable keychain, and its old copy wouldn't delete: {d}");
                    return Err(STALE_COPY.into());
                }
            }
            set_distrust(conn, key, true)?;
            log::warn!("{key}: no usable keychain ({err}); storing it in the database");
            db::set_app_setting(conn, key, Some(v))
        }
        Err(err) => {
            log::warn!("{key}: couldn't save to the keychain: {err}");
            Err(SAVE_FAILED.into())
        }
    }
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

/// Delete the keychain copy and say what's left. A locked store is never "cleared".
fn clear_keychain(e: &keyring::Entry) -> Cleared {
    match e.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Cleared::Gone,
        Err(err) => match keychain_copy(e) {
            KeychainCopy::Gone => Cleared::Gone,
            KeychainCopy::NoService if no_keychain_service(&err) => Cleared::NoService,
            _ => Cleared::Kept,
        },
    }
}

/// Disconnect: remove the keychain copy and the SQLite row. Only a copy confirmed gone counts
/// as removed, except where there's no keychain service and the token lived in SQLite. When
/// the keychain may keep a copy, it's distrusted first, so Silo stops using it regardless.
fn clear_with(
    conn: &Connection,
    key: &str,
    keychain: Option<&keyring::Entry>,
    probe: impl Fn() -> bool,
) -> Result<bool, String> {
    let Some(e) = keychain else {
        clear_fallback(conn, key)?;
        return Ok(true);
    };
    let had_row = db::get_app_setting(conn, key).is_some();
    let removed = match clear_keychain(e) {
        Cleared::Gone => {
            set_distrust(conn, key, false)?;
            true
        }
        cleared => {
            // Distrust before dropping the row, so a failure here leaves everything as it was.
            set_distrust(conn, key, true)?;
            cleared == Cleared::NoService && had_row && !probe()
        }
    };
    if !removed {
        log::warn!("{key}: couldn't remove it from the keychain; it won't be used");
    }
    // The plaintext copy goes either way.
    clear_fallback(conn, key)?;
    Ok(removed)
}

/// A keychain error saying the platform's store itself failed (in keyring 3.6: no Secret
/// Service on D-Bus, or another backend error). `NoStorageAccess` is not this: it means a store
/// exists but is locked or refused access, so the user can fix it and Silo must not fall back.
/// The other variants are about this one entry (too long, ambiguous, bad encoding).
fn no_keychain_service(err: &keyring::Error) -> bool {
    matches!(err, keyring::Error::PlatformFailure(_))
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
fn clear_fallback(conn: &Connection, key: &str) -> Result<(), String> {
    if db::get_app_setting(conn, key).is_none() {
        return Ok(());
    }
    db::set_app_setting(conn, key, None)?;
    db::vacuum(conn);
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
        /// No service, or a broken backend.
        Platform,
        /// Locked, or access denied.
        NoAccess,
        /// A problem with this one entry.
        Invalid,
    }

    fn err(f: Fail) -> keyring::Error {
        match f {
            Fail::Platform => keyring::Error::PlatformFailure("fake platform".into()),
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
            Ok(())
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
    const KEYCHAIN_WORKS: fn() -> bool = || true;
    const NO_KEYCHAIN: fn() -> bool = || false;

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
        assert!(
            keychain_distrusted(&conn, KEY),
            "a copy it couldn't see is distrusted"
        );
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
        // Unreadable, but a delete answers: an error too.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
        });
        let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(SAVE_FAILED.to_string()));
        assert_eq!(row(&conn), None);
    }

    #[test]
    fn the_fallback_refuses_when_the_delete_says_the_store_is_locked() {
        // The service fails to write and read, but the delete reports a locked store: there may
        // be an old copy behind the lock.
        let conn = mem_db();
        let k = FakeKeychain::with(|s| {
            s.set_fails = Some(Fail::Platform);
            s.get_fails = Some(Fail::Platform);
            s.delete_fails = Some(Fail::NoAccess);
        });
        let e = set_with(&conn, KEY, Some("new"), Some(&k.entry()), NO_KEYCHAIN);
        assert_eq!(e, Err(STALE_COPY.to_string()));
        assert_eq!(row(&conn), None);
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
            Ok(true)
        );
        let k = FakeKeychain::with(|s| {
            s.stored = Some("tok".into());
            s.delete_fails = Some(Fail::NoAccess);
        });
        assert_eq!(
            clear_with(&conn, KEY, Some(&k.entry()), KEYCHAIN_WORKS),
            Ok(false)
        );
        assert_eq!(clear_with(&conn, KEY, None, KEYCHAIN_WORKS), Ok(true));
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
