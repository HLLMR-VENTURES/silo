//! Secret storage for API tokens. Prefers the OS credential store (Windows Credential
//! Manager, macOS Keychain, Linux Secret Service) so tokens never sit in plaintext
//! beside ordinary preferences. Two safety nets keep it robust:
//!
//! * **Migration** — a token written by an older build (plaintext in the `app_setting`
//!   table) is moved into the keychain the first time it's read, then cleared from SQLite.
//! * **Fallback** — on a machine with no working keychain (e.g. a headless Linux box),
//!   we fall back to the SQLite setting so the feature still works. That's no worse than
//!   before; everywhere with a keychain (all Windows/macOS, most Linux) is now secure.
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

/// Read a secret: keychain first; otherwise migrate any legacy SQLite value into the
/// keychain and return it.
pub fn get(conn: &Connection, key: &str) -> Option<String> {
    if let Some(e) = entry(key) {
        match e.get_password() {
            Ok(v) => return Some(v),
            Err(keyring::Error::NoEntry) => {} // not in keychain — check legacy SQLite
            Err(_) => {}                       // keychain unavailable — check SQLite fallback
        }
    }
    let legacy = db::get_app_setting(conn, key)?;
    // Best-effort promotion into the keychain, then drop the plaintext copy AND VACUUM so the
    // old token isn't left carve-able in the DB's free pages (SQLite doesn't zero deletes).
    // This runs at most once per key — after migration the legacy value is gone.
    if let Some(e) = entry(key) {
        if e.set_password(&legacy).is_ok() {
            let _ = db::set_app_setting(conn, key, None);
            db::vacuum(conn);
        }
    }
    Some(legacy)
}

/// Store (`Some`) or clear (`None`) a secret. Keychain-primary, with a SQLite fallback
/// only when the keychain can't be used.
pub fn set(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    match value {
        Some(v) => {
            if let Some(e) = entry(key) {
                if e.set_password(v).is_ok() {
                    // Ensure it isn't also left behind in SQLite from a prior build.
                    let _ = db::set_app_setting(conn, key, None);
                    return Ok(());
                }
            }
            // No usable keychain — keep the feature working via SQLite (unchanged from before).
            db::set_app_setting(conn, key, Some(v))
        }
        None => {
            if let Some(e) = entry(key) {
                let _ = e.delete_credential();
            }
            db::set_app_setting(conn, key, None)
        }
    }
}
