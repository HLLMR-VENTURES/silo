//! SQLite persistence. First use: a scan cache keyed by path+mtime+size that lets
//! warm rescans skip opening/parsing archives entirely. Real indexed tables (a PK
//! on path), not `LIKE 'mods_%'` key-value scans. Additive migrations; we never
//! wipe the cache on version bumps.

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// A cached scan row: the invalidation key (mtime+size) plus the serialized entry.
pub struct CacheEntry {
    pub mtime_ms: u64,
    pub size: u64,
    pub json: String,
}

/// User curation flags, keyed by mod tech-name (stable across moves/versions).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CurationRow {
    pub tech_name: String,
    pub favorite: bool,
    pub hidden: bool,
    pub broken: bool,
    #[serde(default)]
    pub rating: i64,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TagRow {
    pub tech_name: String,
    pub tag: String,
}

/// A mod linked to its GitHub repo for update-checking.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRow {
    pub tech_name: String,
    pub owner: String,
    pub repo: String,
}

/// A manual category reassignment, keyed by tech-name.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CategoryOverride {
    pub tech_name: String,
    pub category: String,
    pub subcategory: Option<String>,
}

/// Manifest row: a mod Silo has moved into `mods/archive/<category>/`. `active`
/// means it's currently projected (linked) into the flat root for the game.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizedRow {
    pub tech_name: String,
    pub file_name: String,
    pub kind: String,
    pub category: String,
    pub subcategory: Option<String>,
    pub active: bool,
}

/// A named, saved set of active mods.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Loadout {
    pub id: i64,
    pub name: String,
    pub mods: Vec<String>,
}

/// Open (creating if needed) the Silo database and ensure the schema exists.
pub fn open(db_path: &Path) -> Result<Connection, String> {
    if let Some(dir) = db_path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let conn = Connection::open(db_path).map_err(|e| e.to_string())?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         CREATE TABLE IF NOT EXISTS mod_cache (
             path      TEXT PRIMARY KEY,
             mtime_ms  INTEGER NOT NULL,
             size      INTEGER NOT NULL,
             json      TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS curation (
             tech_name TEXT PRIMARY KEY,
             favorite  INTEGER NOT NULL DEFAULT 0,
             hidden    INTEGER NOT NULL DEFAULT 0,
             broken    INTEGER NOT NULL DEFAULT 0,
             note      TEXT
         );
         CREATE TABLE IF NOT EXISTS category_override (
             tech_name   TEXT PRIMARY KEY,
             category    TEXT NOT NULL,
             subcategory TEXT
         );
         CREATE TABLE IF NOT EXISTS organized (
             root        TEXT NOT NULL DEFAULT '',
             tech_name   TEXT NOT NULL,
             file_name   TEXT NOT NULL,
             kind        TEXT NOT NULL,
             category    TEXT NOT NULL,
             subcategory TEXT,
             active      INTEGER NOT NULL DEFAULT 0,
             PRIMARY KEY (root, tech_name)
         );
         CREATE TABLE IF NOT EXISTS loadout (
             id   INTEGER PRIMARY KEY AUTOINCREMENT,
             name TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS loadout_mod (
             loadout_id INTEGER NOT NULL,
             tech_name  TEXT NOT NULL,
             PRIMARY KEY (loadout_id, tech_name)
         );
         CREATE TABLE IF NOT EXISTS mod_tag (
             tech_name TEXT NOT NULL,
             tag       TEXT NOT NULL,
             PRIMARY KEY (tech_name, tag)
         );
         CREATE TABLE IF NOT EXISTS mod_repo (
             tech_name TEXT PRIMARY KEY,
             owner     TEXT NOT NULL,
             repo      TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS app_setting (
             key   TEXT PRIMARY KEY,
             value TEXT NOT NULL
         );
         -- Provenance for projected files: when Silo writes a mod into the flat root it records
         -- the file's (size, mtime). If those still match, the file is provably Silo's own
         -- untouched projection — so it can be removed without a full content-hash. A file the
         -- user swapped in has a different mtime and won't match, so it's never wrongly deleted.
         CREATE TABLE IF NOT EXISTS projection (
             root      TEXT NOT NULL DEFAULT '',
             file_name TEXT NOT NULL,
             size      INTEGER NOT NULL,
             mtime_ms  INTEGER NOT NULL,
             PRIMARY KEY (root, file_name)
         );",
    )
    .map_err(|e| e.to_string())?;

    // Additive migrations (ignore "duplicate column" on existing DBs).
    let _ = conn.execute(
        "ALTER TABLE curation ADD COLUMN rating INTEGER NOT NULL DEFAULT 0",
        [],
    );
    migrate_scope_by_root(&conn)?;
    Ok(conn)
}

fn has_column(conn: &Connection, table: &str, col: &str) -> bool {
    let Ok(mut st) = conn.prepare(&format!("PRAGMA table_info({table})")) else {
        return false;
    };
    let names = st.query_map([], |r| r.get::<_, String>(1));
    names
        .map(|it| it.flatten().any(|n| n == col))
        .unwrap_or(false)
}

/// v0.8 (#162): scope the organize manifest and projection records to a mods folder. SQLite
/// can't change a primary key in place, so both tables are rebuilt in one transaction; old
/// rows get `root = ''` until `claim_unscoped` stamps them with the default folder. Skipped
/// when already migrated. A failure rolls back and leaves the old schema intact.
fn migrate_scope_by_root(conn: &Connection) -> Result<(), String> {
    if has_column(conn, "organized", "root") && has_column(conn, "projection", "root") {
        return Ok(());
    }
    conn.execute_batch(
        "BEGIN;
         CREATE TABLE organized_new (
             root TEXT NOT NULL DEFAULT '', tech_name TEXT NOT NULL, file_name TEXT NOT NULL,
             kind TEXT NOT NULL, category TEXT NOT NULL, subcategory TEXT,
             active INTEGER NOT NULL DEFAULT 0, PRIMARY KEY (root, tech_name));
         INSERT INTO organized_new(root, tech_name, file_name, kind, category, subcategory, active)
             SELECT '', tech_name, file_name, kind, category, subcategory, active FROM organized;
         DROP TABLE organized;
         ALTER TABLE organized_new RENAME TO organized;
         CREATE TABLE projection_new (
             root TEXT NOT NULL DEFAULT '', file_name TEXT NOT NULL, size INTEGER NOT NULL,
             mtime_ms INTEGER NOT NULL, PRIMARY KEY (root, file_name));
         INSERT INTO projection_new(root, file_name, size, mtime_ms)
             SELECT '', file_name, size, mtime_ms FROM projection;
         DROP TABLE projection;
         ALTER TABLE projection_new RENAME TO projection;
         COMMIT;",
    )
    .map_err(|e| {
        let _ = conn.execute_batch("ROLLBACK;");
        format!("database migration failed: {e}")
    })
}

/// Stamp rows written before folders were tracked (`root = ''`) with `root`. Before v0.8 Silo
/// only ever managed the default folder, so callers pass that folder's key. Idempotent; a
/// stale unscoped row that collides with an existing scoped one is dropped.
pub fn claim_unscoped(conn: &Connection, root: &str) -> Result<(), String> {
    for sql in [
        "UPDATE OR IGNORE organized SET root = ?1 WHERE root = ''",
        "UPDATE OR IGNORE projection SET root = ?1 WHERE root = ''",
    ] {
        conn.execute(sql, [root]).map_err(|e| e.to_string())?;
    }
    conn.execute_batch(
        "DELETE FROM organized WHERE root = ''; DELETE FROM projection WHERE root = '';",
    )
    .map_err(|e| e.to_string())
}

/// How many mods Silo has organized in this folder.
pub fn count_organized(conn: &Connection, root: &str) -> i64 {
    conn.query_row(
        "SELECT COUNT(*) FROM organized WHERE root = ?1",
        [root],
        |r| r.get(0),
    )
    .unwrap_or(0)
}

/// Load all tags, grouped later by the caller.
pub fn load_tags(conn: &Connection) -> Vec<TagRow> {
    let Ok(mut stmt) =
        conn.prepare("SELECT tech_name, tag FROM mod_tag ORDER BY tag COLLATE NOCASE")
    else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(TagRow {
            tech_name: r.get(0)?,
            tag: r.get(1)?,
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

/// Generic app setting get/set (GitHub client id, token, user, …).
pub fn get_app_setting(conn: &Connection, key: &str) -> Option<String> {
    conn.query_row("SELECT value FROM app_setting WHERE key = ?1", [key], |r| {
        r.get(0)
    })
    .ok()
}

pub fn set_app_setting(conn: &Connection, key: &str, value: Option<&str>) -> Result<(), String> {
    match value {
        None => conn.execute("DELETE FROM app_setting WHERE key = ?1", [key]),
        Some(v) => conn.execute(
            "INSERT INTO app_setting(key, value) VALUES(?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            rusqlite::params![key, v],
        ),
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Rewrite the database file, reclaiming free pages. SQLite leaves deleted row bytes in
/// free pages until this runs — so after scrubbing a legacy plaintext secret we VACUUM to
/// ensure the old token can't be carved out of the file. Best-effort; never fatal.
pub fn vacuum(conn: &Connection) {
    let _ = conn.execute_batch("VACUUM");
}

/// Load all mod→repo links.
pub fn load_repos(conn: &Connection) -> Vec<RepoRow> {
    let Ok(mut stmt) = conn.prepare("SELECT tech_name, owner, repo FROM mod_repo") else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(RepoRow {
            tech_name: r.get(0)?,
            owner: r.get(1)?,
            repo: r.get(2)?,
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

/// Set (or clear, when owner is empty) a mod's GitHub repo link.
pub fn set_repo(conn: &Connection, tech_name: &str, owner: &str, repo: &str) -> Result<(), String> {
    if owner.trim().is_empty() || repo.trim().is_empty() {
        conn.execute("DELETE FROM mod_repo WHERE tech_name = ?1", [tech_name])
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO mod_repo(tech_name, owner, repo) VALUES(?1, ?2, ?3)
         ON CONFLICT(tech_name) DO UPDATE SET owner = excluded.owner, repo = excluded.repo",
        rusqlite::params![tech_name, owner.trim(), repo.trim()],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Replace all tags for a mod.
pub fn set_tags(conn: &mut Connection, tech_name: &str, tags: &[String]) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM mod_tag WHERE tech_name = ?1", [tech_name])
        .map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare("INSERT OR IGNORE INTO mod_tag(tech_name, tag) VALUES(?1, ?2)")
            .map_err(|e| e.to_string())?;
        for t in tags {
            let t = t.trim();
            if !t.is_empty() {
                stmt.execute(rusqlite::params![tech_name, t])
                    .map_err(|e| e.to_string())?;
            }
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Load all saved loadouts with their mod lists.
pub fn load_loadouts(conn: &Connection) -> Vec<Loadout> {
    let Ok(mut stmt) = conn.prepare("SELECT id, name FROM loadout ORDER BY name COLLATE NOCASE")
    else {
        return Vec::new();
    };
    let heads: Vec<(i64, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .map(|r| r.flatten().collect())
        .unwrap_or_default();

    heads
        .into_iter()
        .map(|(id, name)| {
            let mods = conn
                .prepare("SELECT tech_name FROM loadout_mod WHERE loadout_id = ?1")
                .and_then(|mut s| {
                    s.query_map([id], |r| r.get::<_, String>(0))
                        .map(|rows| rows.flatten().collect::<Vec<_>>())
                })
                .unwrap_or_default();
            Loadout { id, name, mods }
        })
        .collect()
}

/// Create (id None) or update (id Some) a loadout; returns its id.
pub fn save_loadout(
    conn: &mut Connection,
    id: Option<i64>,
    name: &str,
    mods: &[String],
) -> Result<i64, String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let id = match id {
        Some(id) => {
            tx.execute(
                "UPDATE loadout SET name = ?2 WHERE id = ?1",
                rusqlite::params![id, name],
            )
            .map_err(|e| e.to_string())?;
            tx.execute("DELETE FROM loadout_mod WHERE loadout_id = ?1", [id])
                .map_err(|e| e.to_string())?;
            id
        }
        None => {
            tx.execute("INSERT INTO loadout(name) VALUES(?1)", [name])
                .map_err(|e| e.to_string())?;
            tx.last_insert_rowid()
        }
    };
    {
        let mut stmt = tx
            .prepare("INSERT OR IGNORE INTO loadout_mod(loadout_id, tech_name) VALUES(?1, ?2)")
            .map_err(|e| e.to_string())?;
        for m in mods {
            stmt.execute(rusqlite::params![id, m])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(id)
}

pub fn delete_loadout(conn: &mut Connection, id: i64) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM loadout_mod WHERE loadout_id = ?1", [id])
        .map_err(|e| e.to_string())?;
    tx.execute("DELETE FROM loadout WHERE id = ?1", [id])
        .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

/// Load the organize manifest for one mods folder (`root` = its `modsroot::canonical_key`).
pub fn load_organized(conn: &Connection, root: &str) -> Vec<OrganizedRow> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT tech_name, file_name, kind, category, subcategory, active
         FROM organized WHERE root = ?1",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map([root], |r| {
        Ok(OrganizedRow {
            tech_name: r.get(0)?,
            file_name: r.get(1)?,
            kind: r.get(2)?,
            category: r.get(3)?,
            subcategory: r.get(4)?,
            active: r.get::<_, i64>(5)? != 0,
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

pub fn upsert_organized(conn: &Connection, root: &str, row: &OrganizedRow) -> Result<(), String> {
    conn.execute(
        "INSERT INTO organized(root, tech_name, file_name, kind, category, subcategory, active)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(root, tech_name) DO UPDATE SET
             file_name = excluded.file_name, kind = excluded.kind,
             category = excluded.category, subcategory = excluded.subcategory,
             active = excluded.active",
        rusqlite::params![
            root,
            row.tech_name,
            row.file_name,
            row.kind,
            row.category,
            row.subcategory,
            row.active as i64
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn set_organized_active(
    conn: &Connection,
    root: &str,
    tech_name: &str,
    active: bool,
) -> Result<(), String> {
    conn.execute(
        "UPDATE organized SET active = ?3 WHERE root = ?1 AND tech_name = ?2",
        rusqlite::params![root, tech_name, active as i64],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete_organized(conn: &Connection, root: &str, tech_name: &str) -> Result<(), String> {
    conn.execute(
        "DELETE FROM organized WHERE root = ?1 AND tech_name = ?2",
        rusqlite::params![root, tech_name],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Record a projected file's identity (size + mtime) so a later remove can confirm it's Silo's
/// own untouched copy without hashing it.
pub fn record_projection(conn: &Connection, root: &str, file_name: &str, size: u64, mtime_ms: i64) {
    let _ = conn.execute(
        "INSERT INTO projection(root, file_name, size, mtime_ms) VALUES(?1, ?2, ?3, ?4)
         ON CONFLICT(root, file_name) DO UPDATE SET size = excluded.size, mtime_ms = excluded.mtime_ms",
        rusqlite::params![root, file_name, size as i64, mtime_ms],
    );
}

/// The recorded (size, mtime_ms) for a projected file in this folder, if any.
pub fn projection_stats(conn: &Connection, root: &str, file_name: &str) -> Option<(u64, i64)> {
    conn.query_row(
        "SELECT size, mtime_ms FROM projection WHERE root = ?1 AND file_name = ?2",
        rusqlite::params![root, file_name],
        |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, i64>(1)?)),
    )
    .ok()
}

/// Drop a projection's provenance (it's been removed from the flat root).
pub fn forget_projection(conn: &Connection, root: &str, file_name: &str) {
    let _ = conn.execute(
        "DELETE FROM projection WHERE root = ?1 AND file_name = ?2",
        rusqlite::params![root, file_name],
    );
}

/// Make one folder's `organized` manifest match what's physically in its `archive/`. Upserts a
/// row for every archived mod (so `set_active` can project it) and drops rows for mods that are
/// no longer archived. This heals a manifest that has drifted from disk — e.g. a fresh DB with
/// a populated `archive/` — which otherwise leaves archived mods visible but un-activatable.
/// Other folders' rows are never touched.
pub fn reconcile_organized(
    conn: &mut Connection,
    root: &str,
    desired: &[OrganizedRow],
) -> Result<(), String> {
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    let keep: std::collections::HashSet<&str> =
        desired.iter().map(|r| r.tech_name.as_str()).collect();
    for existing in load_organized(&tx, root) {
        if !keep.contains(existing.tech_name.as_str()) {
            delete_organized(&tx, root, &existing.tech_name)?;
        }
    }
    for row in desired {
        upsert_organized(&tx, root, row)?;
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Load all curation rows.
pub fn load_curation(conn: &Connection) -> Vec<CurationRow> {
    let Ok(mut stmt) =
        conn.prepare("SELECT tech_name, favorite, hidden, broken, note, rating FROM curation")
    else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(CurationRow {
            tech_name: r.get(0)?,
            favorite: r.get::<_, i64>(1)? != 0,
            hidden: r.get::<_, i64>(2)? != 0,
            broken: r.get::<_, i64>(3)? != 0,
            note: r.get(4)?,
            rating: r.get::<_, i64>(5).unwrap_or(0),
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

/// Upsert one curation row. If all flags are false and note is empty, delete it.
pub fn set_curation(conn: &Connection, row: &CurationRow) -> Result<(), String> {
    let empty = !row.favorite
        && !row.hidden
        && !row.broken
        && row.rating == 0
        && row.note.as_deref().unwrap_or("").is_empty();
    if empty {
        conn.execute(
            "DELETE FROM curation WHERE tech_name = ?1",
            [&row.tech_name],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO curation(tech_name, favorite, hidden, broken, note, rating)
         VALUES(?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT(tech_name) DO UPDATE SET
             favorite = excluded.favorite,
             hidden   = excluded.hidden,
             broken   = excluded.broken,
             note     = excluded.note,
             rating   = excluded.rating",
        rusqlite::params![
            row.tech_name,
            row.favorite as i64,
            row.hidden as i64,
            row.broken as i64,
            row.note,
            row.rating,
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load all category overrides.
pub fn load_overrides(conn: &Connection) -> Vec<CategoryOverride> {
    let Ok(mut stmt) =
        conn.prepare("SELECT tech_name, category, subcategory FROM category_override")
    else {
        return Vec::new();
    };
    let rows = stmt.query_map([], |r| {
        Ok(CategoryOverride {
            tech_name: r.get(0)?,
            category: r.get(1)?,
            subcategory: r.get(2)?,
        })
    });
    rows.map(|r| r.flatten().collect()).unwrap_or_default()
}

/// Upsert (or clear) a category override.
pub fn set_override(conn: &Connection, row: &CategoryOverride) -> Result<(), String> {
    if row.category.is_empty() {
        conn.execute(
            "DELETE FROM category_override WHERE tech_name = ?1",
            [&row.tech_name],
        )
        .map_err(|e| e.to_string())?;
        return Ok(());
    }
    conn.execute(
        "INSERT INTO category_override(tech_name, category, subcategory)
         VALUES(?1, ?2, ?3)
         ON CONFLICT(tech_name) DO UPDATE SET
             category = excluded.category,
             subcategory = excluded.subcategory",
        rusqlite::params![row.tech_name, row.category, row.subcategory],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// Load the entire scan cache into memory (one query) for lock-free lookups
/// during the parallel scan.
/// Drop every cached scan row, forcing the next scan to re-parse (and re-categorize)
/// every mod from scratch. Used when categorization logic improves and the user wants
/// it applied to their existing library.
pub fn clear_cache(conn: &Connection) -> usize {
    conn.execute("DELETE FROM mod_cache", []).unwrap_or(0)
}

pub fn load_cache(conn: &Connection) -> HashMap<String, CacheEntry> {
    let mut map = HashMap::new();
    let Ok(mut stmt) = conn.prepare("SELECT path, mtime_ms, size, json FROM mod_cache") else {
        return map;
    };
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)? as u64,
            r.get::<_, i64>(2)? as u64,
            r.get::<_, String>(3)?,
        ))
    });
    if let Ok(rows) = rows {
        for (path, mtime_ms, size, json) in rows.flatten() {
            map.insert(
                path,
                CacheEntry {
                    mtime_ms,
                    size,
                    json,
                },
            );
        }
    }
    map
}

/// Upsert freshly-parsed rows in a single transaction.
pub fn upsert_many(
    conn: &mut Connection,
    rows: &[(String, u64, u64, String)],
) -> Result<(), String> {
    if rows.is_empty() {
        return Ok(());
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare(
                "INSERT INTO mod_cache(path, mtime_ms, size, json) VALUES(?1, ?2, ?3, ?4)
                 ON CONFLICT(path) DO UPDATE SET
                     mtime_ms = excluded.mtime_ms,
                     size     = excluded.size,
                     json     = excluded.json",
            )
            .map_err(|e| e.to_string())?;
        for (path, mtime, size, json) in rows {
            stmt.execute(rusqlite::params![path, *mtime as i64, *size as i64, json])
                .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

/// Delete cache rows for mods that are no longer present on disk.
pub fn prune_missing(conn: &mut Connection, present: &HashSet<String>) -> Result<usize, String> {
    let existing: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT path FROM mod_cache")
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.flatten().collect()
    };
    let stale: Vec<&String> = existing.iter().filter(|p| !present.contains(*p)).collect();
    if stale.is_empty() {
        return Ok(0);
    }
    let tx = conn.transaction().map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare("DELETE FROM mod_cache WHERE path = ?1")
            .map_err(|e| e.to_string())?;
        for p in &stale {
            stmt.execute([p]).map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(stale.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("silo_db_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base.join("silo.db")
    }

    #[test]
    fn migrates_a_pre_root_database_and_is_idempotent() {
        let path = temp_db("migrate");
        {
            // The v0.7 schema, with one row in each table.
            let c = rusqlite::Connection::open(&path).unwrap();
            c.execute_batch(
                "CREATE TABLE organized (tech_name TEXT PRIMARY KEY, file_name TEXT NOT NULL,
                     kind TEXT NOT NULL, category TEXT NOT NULL, subcategory TEXT,
                     active INTEGER NOT NULL DEFAULT 0);
                 CREATE TABLE projection (file_name TEXT PRIMARY KEY, size INTEGER NOT NULL,
                     mtime_ms INTEGER NOT NULL);
                 INSERT INTO organized VALUES ('FS25_A','FS25_A.zip','zip','Vehicles',NULL,1);
                 INSERT INTO projection VALUES ('FS25_A.zip', 10, 20);",
            )
            .unwrap();
        }
        let conn = open(&path).unwrap();
        // Old rows arrive unscoped (root = '') until claimed.
        assert_eq!(load_organized(&conn, "").len(), 1);
        claim_unscoped(&conn, "c:\\mods").unwrap();
        assert_eq!(load_organized(&conn, "c:\\mods")[0].tech_name, "FS25_A");
        assert_eq!(
            projection_stats(&conn, "c:\\mods", "FS25_A.zip"),
            Some((10, 20))
        );
        assert!(load_organized(&conn, "").is_empty());
        drop(conn);
        // Re-opening must not rebuild again or lose rows.
        let conn = open(&path).unwrap();
        assert_eq!(load_organized(&conn, "c:\\mods").len(), 1);
    }

    #[test]
    fn the_same_mod_can_be_organized_in_two_folders() {
        let conn = open(&temp_db("twofolders")).unwrap();
        let row = |active| OrganizedRow {
            tech_name: "FS25_A".into(),
            file_name: "FS25_A.zip".into(),
            kind: "zip".into(),
            category: "Vehicles".into(),
            subcategory: None,
            active,
        };
        upsert_organized(&conn, "a", &row(true)).unwrap();
        upsert_organized(&conn, "b", &row(false)).unwrap();
        assert!(load_organized(&conn, "a")[0].active);
        assert!(!load_organized(&conn, "b")[0].active);
        assert_eq!(count_organized(&conn, "a"), 1);
        delete_organized(&conn, "b", "FS25_A").unwrap();
        assert_eq!(
            count_organized(&conn, "a"),
            1,
            "deleting in b leaves a alone"
        );
    }

    #[test]
    fn reconcile_adopts_disk_and_prunes_gone() {
        let base = std::env::temp_dir().join(format!("silo_reconcile_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        let mut conn = open(&base.join("silo.db")).unwrap();

        // A stale manifest row for a mod no longer archived on disk.
        upsert_organized(
            &conn,
            "r",
            &OrganizedRow {
                tech_name: "FS25_Gone".into(),
                file_name: "FS25_Gone.zip".into(),
                kind: "zip".into(),
                category: "Old".into(),
                subcategory: None,
                active: true,
            },
        )
        .unwrap();

        // Disk truth: one archived mod, not yet in the manifest.
        let desired = vec![OrganizedRow {
            tech_name: "FS25_New".into(),
            file_name: "FS25_New.zip".into(),
            kind: "zip".into(),
            category: "Objects".into(),
            subcategory: None,
            active: false,
        }];
        reconcile_organized(&mut conn, "r", &desired).unwrap();

        let rows = load_organized(&conn, "r");
        assert_eq!(rows.len(), 1, "manifest should match disk exactly");
        assert_eq!(rows[0].tech_name, "FS25_New", "archived mod adopted");
        assert!(
            !rows.iter().any(|r| r.tech_name == "FS25_Gone"),
            "vanished mod pruned"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
