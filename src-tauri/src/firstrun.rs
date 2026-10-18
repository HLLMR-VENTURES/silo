//! First-run state (#250): whether the setup flow and the one-time "What's new" card were
//! seen, and whether the library already holds anything Silo made (organized mods, curation).
//! The flags live in `app_setting` under `ui_flag:<name>`, so a webview storage wipe doesn't
//! bring the setup back. Only the names in `FLAGS` can be written from the frontend.

use rusqlite::Connection;
use serde::Serialize;

use crate::db;

/// The flags the frontend may set, as stored under `ui_flag:<name>`.
pub const FLAGS: &[&str] = &["onboarding_done", "whats_new_seen"];
const PREFIX: &str = "ui_flag:";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FirstRunState {
    /// The setup flow was finished or skipped.
    pub onboarding_done: bool,
    /// The "What's new in Silo 1.0" card was dismissed (or the setup flow made it moot).
    pub whats_new_seen: bool,
    /// Any mods folder has organized mods in the library.
    pub has_organized: bool,
    /// Any mod has a curation row (favorite, hidden, flagged or a note).
    pub has_curation: bool,
}

fn exists(conn: &Connection, table: &str) -> Result<bool, String> {
    conn.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table})"), [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n != 0)
    .map_err(|e| e.to_string())
}

/// Read the first-run state. A failed read is an error, never "a new library": the setup
/// flow must not appear over a library that's there but unreadable.
pub fn read(conn: &Connection) -> Result<FirstRunState, String> {
    let set = db::app_settings_with_prefix(conn, PREFIX)?;
    let on = |name: &str| {
        set.iter()
            .any(|(k, v)| k.strip_prefix(PREFIX) == Some(name) && v == "1")
    };
    Ok(FirstRunState {
        onboarding_done: on("onboarding_done"),
        whats_new_seen: on("whats_new_seen"),
        has_organized: exists(conn, "organized")?,
        has_curation: exists(conn, "curation")?,
    })
}

/// Set (or clear) one of the known flags.
pub fn set_flag(conn: &Connection, name: &str, on: bool) -> Result<(), String> {
    if !FLAGS.contains(&name) {
        return Err(format!("Unknown setting: {name}"));
    }
    db::set_app_setting(conn, &format!("{PREFIX}{name}"), on.then_some("1"))
}

#[tauri::command]
pub async fn first_run_state(app: tauri::AppHandle) -> Result<FirstRunState, String> {
    let db_file = crate::db_path(&app)?;
    crate::off_main(move || read(&db::open(&db_file)?)).await
}

#[tauri::command]
pub async fn set_first_run_flag(
    app: tauri::AppHandle,
    name: String,
    on: bool,
) -> Result<(), String> {
    let db_file = crate::db_path(&app)?;
    crate::off_main(move || set_flag(&db::open(&db_file)?, &name, on)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("silo_firstrun_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base.join("silo.db")
    }

    #[test]
    fn a_new_library_has_nothing_set() {
        let conn = db::open(&temp_db("new")).unwrap();
        assert_eq!(
            read(&conn).unwrap(),
            FirstRunState {
                onboarding_done: false,
                whats_new_seen: false,
                has_organized: false,
                has_curation: false,
            }
        );
    }

    #[test]
    fn flags_round_trip_and_only_known_names_are_written() {
        let conn = db::open(&temp_db("flags")).unwrap();
        set_flag(&conn, "onboarding_done", true).unwrap();
        assert!(read(&conn).unwrap().onboarding_done);
        assert!(!read(&conn).unwrap().whats_new_seen);
        set_flag(&conn, "onboarding_done", false).unwrap();
        assert!(!read(&conn).unwrap().onboarding_done);
        // Anything else (e.g. a GitHub or mods-folder setting) is refused.
        assert!(set_flag(&conn, "gh_user", true).is_err());
        assert!(set_flag(&conn, "../onboarding_done", true).is_err());
        assert_eq!(db::get_app_setting(&conn, "ui_flag:gh_user"), None);
    }

    #[test]
    fn organized_mods_and_curation_rows_are_seen() {
        let conn = db::open(&temp_db("rows")).unwrap();
        conn.execute(
            "INSERT INTO organized(root, tech_name, file_name, kind, category) VALUES('', 'A', 'A.zip', 'zip', 'Tractors')",
            [],
        )
        .unwrap();
        let s = read(&conn).unwrap();
        assert!(s.has_organized && !s.has_curation);
        conn.execute(
            "INSERT INTO curation(tech_name, favorite) VALUES('B', 1)",
            [],
        )
        .unwrap();
        assert!(read(&conn).unwrap().has_curation);
    }

    #[test]
    fn a_failed_read_is_an_error() {
        let conn = db::open(&temp_db("broken")).unwrap();
        conn.execute_batch("DROP TABLE organized").unwrap();
        assert!(read(&conn).is_err());
    }
}
