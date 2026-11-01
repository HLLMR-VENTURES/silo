//! Ownership detection: whether a mods-folder entry is Silo's projection, and the scan for
//! foreign files sitting at organized mods' names.

use super::*;

/// True only if `link` in the flat root is provably the projection Silo created for archived
/// `src` — so removing it can't destroy a file the user put there themselves. Symlink /
/// junction resolves to `src`; a zip hardlink shares `src`'s file id; a cross-volume copy is
/// byte-identical to `src`; a copied dir carries our marker. Anything else is NOT ours.
pub(crate) fn is_silo_projection(link: &Path, src: &Path, kind: &str) -> bool {
    let Ok(meta) = link.symlink_metadata() else {
        return false;
    };
    let ft = meta.file_type();

    if ft.is_symlink() {
        // Our dir symlink / Windows junction resolves to the archived source.
        return matches!(
            (std::fs::canonicalize(link), std::fs::canonicalize(src)),
            (Ok(a), Ok(b)) if a == b
        );
    }

    if kind == "dir" || ft.is_dir() {
        // Copy-fallback directory projection carries our marker; a user's own folder won't.
        return link.join(SILO_MARKER).is_file();
    }

    // Regular file (zip). Same underlying file id → our hardlink (cheap, the common case).
    if let (Ok(a), Ok(b)) = (
        same_file::Handle::from_path(link),
        same_file::Handle::from_path(src),
    ) {
        if a == b {
            return true;
        }
    }
    // Distinct files: a cross-volume copy projection is byte-identical to the archive; a
    // user's replacement is not. Cheap size gate first, then a content hash.
    let (Ok(lm), Ok(sm)) = (std::fs::metadata(link), std::fs::metadata(src)) else {
        return false;
    };
    if lm.len() != sm.len() {
        return false;
    }
    matches!(
        (
            crate::provenance::sha256_file(link),
            crate::provenance::sha256_file(src),
        ),
        (Ok(a), Ok(b)) if a == b
    )
}

/// A file in the flat root occupying an organized mod's managed name that ISN'T Silo's
/// projection — something the user (or another tool) placed there. Silo never deletes it, but
/// the user should know their intended mod isn't what the game will load from that name.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignFile {
    pub tech_name: String,
    pub file_name: String,
    pub kind: String, // "zip" | "dir"
    /// Version of the file currently in the mods folder (what "adopt" would keep).
    pub flat_version: Option<String>,
    /// Version of Silo's managed/archived copy (what "restore" would put back).
    pub managed_version: Option<String>,
}

/// Best-effort mod version from a mod's `modDesc.xml`, for showing which build is which.
fn mod_version(path: &Path, kind: &str) -> Option<String> {
    crate::scan::read_moddesc_xml(path, kind)
        .ok()
        .and_then(|xml| crate::moddesc::parse(&xml).version)
}

/// Scan the flat root for foreign/mismatched files sitting at organized mods' names. Cheap:
/// parked mods leave the flat root empty, so it only identity-checks the handful of entries
/// actually present against their archived source. Reuses [`is_silo_projection`], so a real
/// hardlink/symlink/copy projection is never flagged — only a file Silo can't prove it created.
pub fn detect_foreign_projections(conn: &Connection, layout: &Layout) -> Vec<ForeignFile> {
    let root = &layout.mods;
    let key = crate::modsroot::canonical_key(root);
    let mut out = Vec::new();
    // Only organized mods count, each at its recorded archive path: a loose zip with no row is
    // an ordinary "unorganized" mod (Organize handles it), and an unmanaged file that happens to
    // sit in an archive folder is no one's counterpart. The manifest, not a directory listing,
    // decides which archived copy a name belongs to.
    for row in db::load_organized(conn, &key) {
        if row.file_name.eq_ignore_ascii_case(ARCHIVE)
            || row.file_name.eq_ignore_ascii_case("backups")
        {
            continue;
        }
        let link = root.join(&row.file_name);
        if link.symlink_metadata().is_err() {
            continue; // parked: nothing at the mod's name
        }
        let src = archive_path(&layout.archive, &row.category, &row.file_name);
        if src.symlink_metadata().is_err() {
            continue; // no archived copy to compare against (or to restore)
        }
        let kind = row.kind.as_str();
        // Fast path: provenance match → provably ours, no hash. Else the careful check (which
        // hashes a copy). Either way, (re)record provenance for a confirmed zip so the next scan
        // — and any deactivate — is O(1) instead of re-hashing gigabytes.
        if matches_provenance(conn, &key, &link, &row.file_name)
            || is_silo_projection(&link, &src, kind)
        {
            if kind == "zip" {
                if let Some((size, mtime)) = file_stats(&link) {
                    db::record_projection(conn, &key, &row.file_name, size, mtime);
                }
            }
            continue;
        }
        out.push(ForeignFile {
            flat_version: mod_version(&link, kind),
            managed_version: mod_version(&src, kind),
            tech_name: row.tech_name,
            file_name: row.file_name,
            kind: row.kind,
        });
    }
    out
}
