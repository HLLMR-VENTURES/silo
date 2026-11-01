//! Provenance manifest hashing — the client half of the SiloAPI provenance contract.
//!
//! See `docs/PROVENANCE.md` (client) and SiloAPI `docs/PROVENANCE-CANONICALIZATION.md`
//! (the ratified byte format). The whole tamper-verification feature turns on this hash
//! matching the server's **byte-for-byte** — a one-character format divergence would make
//! every mod read "modified" (the exact false-positive the contract exists to prevent). So
//! the pure `manifest_hash` computation lives here behind the §7 conformance vectors, and
//! is ratified (green vectors on both sides) BEFORE any zip/scan integration is built.
//!
//! Ratified decisions (answers to the spec's §10 open items):
//!   §10.1 sort  — by the UTF-8 byte sequence of the path.
//!   §10.2 NFC   — yes, NFC-normalize the decoded path (fewer false "modified" verdicts).
//!   §10.3 decode— UTF-8-flag-aware with CP437 fallback happens at the zip-read layer
//!                 (upstream of this pure function); CP437 is a static table, so parity-safe.
//!   §10.4 hash  — per-entry sha256 is over UNCOMPRESSED bytes (computed upstream).
//!   §10.5 vectors — reproduced in the tests below.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::Path;
use unicode_normalization::UnicodeNormalization;

/// One regular-file entry: its canonicalized path and the lowercase-hex sha256 of the
/// entry's UNCOMPRESSED bytes.
#[derive(Debug, Clone)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
}

/// Per-entry zip-bomb guard: refuse to hash a single member that decompresses past this.
/// Generous (a map's `.i3d`/`.dds` can be large) but bounds a hostile tiny-in/huge-out entry.
const ENTRY_LIMIT: u64 = 4 * 1024 * 1024 * 1024;

/// The locally-computed manifest of an installed mod zip: the whole-file hash (exact-match
/// fast path), the ratified `manifestHash`, and the per-entry list (for the tamper diff).
#[derive(Debug, Clone)]
pub struct LocalManifest {
    pub archive_sha256: String,
    pub manifest_hash: String,
    pub entries: Vec<Entry>,
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// Canonicalize a raw zip entry name per spec §3: `\`→`/`, strip a single leading `./`
/// and any leading `/`, NFC-normalize, case preserved. (Decode and directory exclusion
/// happen at the zip-reading layer, upstream of this.)
pub fn canon_path(raw: &str) -> String {
    let slashed = raw.replace('\\', "/");
    let trimmed = slashed
        .strip_prefix("./")
        .unwrap_or(&slashed)
        .trim_start_matches('/');
    trimmed.nfc().collect()
}

/// Compute `manifestHash` from the file entries (spec §4): NFC-normalize paths, sort by
/// UTF-8 byte order, emit `<path>\0<sha256-hex>\n` per entry, sha256 the concatenation.
/// Returns lowercase hex. Order-independent of the input (it sorts).
///
/// Paths are expected canonical already ([`canon_path`] runs once, when the zip is read). Only
/// NFC is applied here, exactly as SiloAPI's `manifestHash` does: `canon_path` isn't idempotent
/// (`././a` → `./a` → `a`), so a second pass would hash a different path than the server.
///
/// Duplicate canonical paths are a spec anomaly (§4) — the caller that builds `Entry`s
/// from a real zip must fail loud on them; this pure function assumes distinct paths.
pub fn manifest_hash(entries: &[Entry]) -> String {
    let mut norm: Vec<(String, &str)> = entries
        .iter()
        .map(|e| (e.path.nfc().collect::<String>(), e.sha256.as_str()))
        .collect();
    // §4.2 — UTF-8 byte order. Rust's default `String` Ord already IS byte order; sorting
    // on `.as_bytes()` makes the contract explicit and immune to any future Ord change.
    norm.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    let mut h = Sha256::new();
    for (path, sha) in &norm {
        h.update(path.as_bytes());
        h.update([0x00]);
        h.update(sha.as_bytes());
        h.update([0x0a]);
    }
    hex_lower(&h.finalize())
}

/// Stream a reader through sha256, refusing to read past `limit` bytes (zip-bomb guard).
/// Returns the lowercase-hex digest.
fn sha256_stream<R: Read>(mut r: R, limit: u64) -> Result<String, String> {
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = r.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > limit {
            return Err(format!("an entry exceeds the {limit}-byte safety limit"));
        }
        h.update(&buf[..n]);
    }
    Ok(hex_lower(&h.finalize()))
}

/// sha256 of a whole file, streamed (bounded memory). Used for `archiveSha256`.
pub fn sha256_file(path: &Path) -> Result<String, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    sha256_stream(f, u64::MAX)
}

/// Compute the local manifest of an installed mod `.zip`: whole-file `archiveSha256`, the
/// ratified `manifestHash`, and the per-entry list. Directory entries are excluded; decode
/// follows the zip UTF-8 flag with CP437 fallback (the `zip` crate does this, matching the
/// server). A duplicate canonical path is a spec anomaly (§4) — we fail loud rather than
/// emit a partial manifest that would hash wrong.
pub fn manifest_from_zip(zip_path: &Path) -> Result<LocalManifest, String> {
    let archive_sha256 = sha256_file(zip_path)?;
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut ar = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;

    let mut entries: Vec<Entry> = Vec::with_capacity(ar.len());
    let mut seen: HashSet<String> = HashSet::new();
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).map_err(|e| e.to_string())?;
        if f.is_dir() {
            continue;
        }
        let path = canon_path(&f.name().map_err(|e| e.to_string())?);
        if path.is_empty() {
            continue;
        }
        if !seen.insert(path.clone()) {
            return Err(format!(
                "anomalous archive: duplicate canonical path {path:?}. Not verifiable"
            ));
        }
        let sha256 = sha256_stream(&mut f, ENTRY_LIMIT)?;
        entries.push(Entry { path, sha256 });
    }
    let manifest_hash = manifest_hash(&entries);
    Ok(LocalManifest {
        archive_sha256,
        manifest_hash,
        entries,
    })
}

/// Verdict of comparing a local mod against the canonical build.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerifyStatus {
    /// Byte-identical to the trusted build (by whole-zip or content manifest).
    Verified,
    /// Same mod, but files differ — the diff names them.
    Modified,
    /// No hashed canonical build to compare against (not proof of anything).
    Unverified,
}

/// The result the UI renders: a status, how a match was reached, and — when Modified — the
/// exact files that differ (the injected-code candidates).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyResult {
    pub status: VerifyStatus,
    /// `"exact"` (whole-zip match) or `"content"` (manifest match) when Verified.
    pub how: Option<String>,
    /// Local files absent from the canonical build.
    pub added: Vec<String>,
    /// Canonical files missing locally.
    pub removed: Vec<String>,
    /// Files present in both with different content.
    pub changed: Vec<String>,
    /// The canonical version compared against (for display).
    pub matched_version: Option<String>,
    /// Plain-language context (why Unverified, etc.).
    pub note: Option<String>,
}

impl VerifyResult {
    pub fn unverified(note: &str) -> Self {
        VerifyResult {
            status: VerifyStatus::Unverified,
            how: None,
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            matched_version: None,
            note: Some(note.to_string()),
        }
    }
}

/// Pure comparison of a local manifest against a canonical one. Exact whole-zip match →
/// Verified("exact"); manifest-hash match → Verified("content"); otherwise a Modified verdict
/// with the added/removed/changed file lists.
pub fn compare(
    local: &LocalManifest,
    canon_archive_sha256: Option<&str>,
    canon_manifest_hash: &str,
    canon_entries: &[Entry],
) -> VerifyResult {
    let verified = |how: &str| VerifyResult {
        status: VerifyStatus::Verified,
        how: Some(how.to_string()),
        added: Vec::new(),
        removed: Vec::new(),
        changed: Vec::new(),
        matched_version: None,
        note: None,
    };

    if let Some(a) = canon_archive_sha256 {
        if a.eq_ignore_ascii_case(&local.archive_sha256) {
            return verified("exact");
        }
    }
    if canon_manifest_hash.eq_ignore_ascii_case(&local.manifest_hash) {
        return verified("content");
    }

    let local_map: HashMap<&str, &str> = local
        .entries
        .iter()
        .map(|e| (e.path.as_str(), e.sha256.as_str()))
        .collect();
    let canon_map: HashMap<&str, &str> = canon_entries
        .iter()
        .map(|e| (e.path.as_str(), e.sha256.as_str()))
        .collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    for (p, s) in &local_map {
        match canon_map.get(p) {
            None => added.push((*p).to_string()),
            Some(cs) if !cs.eq_ignore_ascii_case(s) => changed.push((*p).to_string()),
            _ => {}
        }
    }
    let mut removed: Vec<String> = canon_map
        .keys()
        .filter(|p| !local_map.contains_key(*p))
        .map(|p| (*p).to_string())
        .collect();
    added.sort();
    changed.sort();
    removed.sort();

    VerifyResult {
        status: VerifyStatus::Modified,
        how: None,
        added,
        removed,
        changed,
        matched_version: None,
        note: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(path: &str, sha: &str) -> Entry {
        Entry {
            path: path.into(),
            sha256: sha.into(),
        }
    }

    // ── SiloAPI docs/PROVENANCE-CANONICALIZATION.md §7 — ratification vectors ──
    // Green here == the byte format is agreed with infra == infra builds the pipeline.

    #[test]
    fn vector_1_ascii() {
        let entries = [
            e(
                "modDesc.xml",
                "45fcb867bb43ef043f1f1fe5ea6f95edd2099a71152259d35358a03907704c2c",
            ),
            e(
                "scripts/main.lua",
                "d287bb7f9d15abdc5b6e98536263815744b6ef21c8f3c839fc434ca70d8efe99",
            ),
            e(
                "maps/map01.i3d",
                "441f2166bc95eb490f49e712ac4cfd2f1dfc1a57f5111b699965eadf47c6dd0c",
            ),
        ];
        assert_eq!(
            manifest_hash(&entries),
            "9fea10cd5565c39b36fe3c6d977f92b950fbbcfe1774849143340ba5dbcb8792"
        );
    }

    #[test]
    fn vector_2_unicode_nfc() {
        // `ü` = U+00FC (NFC, precomposed) — written as an explicit escape so the source
        // file's own encoding can't smuggle in a decomposed form.
        let entries = [
            e(
                "modDesc.xml",
                "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
            ),
            e(
                "scripts/Z\u{00FC}rich.lua",
                "594e519ae499312b29433b7dd8a97ff068defcba9755b6d5d00e84c524d67b06",
            ),
            e(
                "scripts/a.lua",
                "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d",
            ),
        ];
        assert_eq!(
            manifest_hash(&entries),
            "8dceff0991ea088ca8a27b774396122fdf54d38c3aba91c47b303e422e19433e"
        );
    }

    // A `././` entry name canonicalizes ONCE (at zip read) to `./scripts/a.lua`; the hash then
    // applies NFC only. Expected value computed with SiloAPI's `manifestHash(canonicalPath(..))`
    // (src/lib/manifest.ts). Canonicalizing twice would hash `scripts/a.lua` instead
    // (61b5f323…), a false "Modified" against the server.
    #[test]
    fn vector_3_dot_slash_entry_is_canonicalized_once() {
        let canon = canon_path("././scripts/a.lua");
        assert_eq!(canon, "./scripts/a.lua");
        let entries = [
            e(
                "modDesc.xml",
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            ),
            e(
                &canon,
                "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7",
            ),
        ];
        assert_eq!(
            manifest_hash(&entries),
            "26524b7cdeb98d9387b616cc1b3cceadcbfd71b8d5e0377cc5e69be5e32438ef"
        );
    }

    #[test]
    fn empty_manifest_is_sha256_of_nothing() {
        assert_eq!(
            manifest_hash(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn canon_normalizes_separators_and_prefixes() {
        assert_eq!(canon_path("./scripts\\main.lua"), "scripts/main.lua");
        assert_eq!(canon_path("/a/b.lua"), "a/b.lua");
        // NFC: a decomposed `u`+combining-diaeresis collapses to U+00FC.
        assert_eq!(canon_path("u\u{0308}.lua"), "\u{00FC}.lua");
    }

    #[test]
    fn manifest_from_zip_hashes_files_and_excludes_dirs() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("silo_prov_zip_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("m.zip");
        {
            let f = std::fs::File::create(&zpath).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.add_directory("scripts/", opts).unwrap(); // must be excluded
            zw.start_file("modDesc.xml", opts).unwrap();
            zw.write_all(b"hello").unwrap();
            zw.start_file("scripts/a.lua", opts).unwrap();
            zw.write_all(b"world").unwrap();
            zw.finish().unwrap();
        }

        let m = manifest_from_zip(&zpath).unwrap();
        assert_eq!(m.entries.len(), 2, "the directory entry must be excluded");
        // Cross-check against the ratified hash of the same files (sha256 of "hello"/"world").
        let expected = manifest_hash(&[
            e(
                "modDesc.xml",
                "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824",
            ),
            e(
                "scripts/a.lua",
                "486ea46224d1bb4fb680f34f7c9ad96a8f24ec88be73ea8e5a6c65260e9cb8a7",
            ),
        ]);
        assert_eq!(m.manifest_hash, expected);

        let _ = std::fs::remove_dir_all(&dir);
    }

    // `././a.lua` and `a.lua` are distinct entries: each is canonicalized once at read, so they
    // stay `./a.lua` and `a.lua` (never merged into one hashed path), and the manifest hash is the
    // server's (SiloAPI `manifestHash(canonicalPath(..))` over the same two entries).
    #[test]
    fn manifest_from_zip_canonicalizes_entry_names_once() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("silo_prov_dot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let zpath = dir.join("m.zip");
        {
            let f = std::fs::File::create(&zpath).unwrap();
            let mut zw = zip::ZipWriter::new(f);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zw.start_file("././a.lua", opts).unwrap();
            zw.write_all(b"hello").unwrap();
            zw.start_file("a.lua", opts).unwrap();
            zw.write_all(b"world").unwrap();
            zw.finish().unwrap();
        }

        let m = manifest_from_zip(&zpath).unwrap();
        let mut paths: Vec<&str> = m.entries.iter().map(|e| e.path.as_str()).collect();
        paths.sort();
        assert_eq!(paths, vec!["./a.lua", "a.lua"]);
        assert_eq!(
            m.manifest_hash,
            "9b7f4b32bdc8b1674d345c63dd4f516343654271188961b1b2abacd98f337a27"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn compare_verifies_exact_and_content_and_diffs_modified() {
        let local = LocalManifest {
            archive_sha256: "aa".into(),
            manifest_hash: "bb".into(),
            entries: vec![e("a.lua", "1"), e("b.lua", "2")],
        };

        // Whole-zip exact match (case-insensitive hex).
        let r = compare(&local, Some("AA"), "zzz", &[]);
        assert_eq!(r.status, VerifyStatus::Verified);
        assert_eq!(r.how.as_deref(), Some("exact"));

        // Manifest (content) match when the archive differs (re-zipped, same content).
        let r = compare(&local, Some("different"), "BB", &[]);
        assert_eq!(r.status, VerifyStatus::Verified);
        assert_eq!(r.how.as_deref(), Some("content"));

        // Modified: b.lua changed, c.lua present canonically but missing locally.
        let canon = [e("a.lua", "1"), e("b.lua", "9"), e("c.lua", "3")];
        let r = compare(&local, Some("different"), "different", &canon);
        assert_eq!(r.status, VerifyStatus::Modified);
        assert_eq!(r.changed, vec!["b.lua"]);
        assert_eq!(r.removed, vec!["c.lua"]);
        assert!(r.added.is_empty());
    }

    #[test]
    fn hash_is_input_order_independent() {
        let a = [
            e(
                "b.lua",
                "3e23e8160039594a33894f6564e1b1348bbd7a0088d42c4acb73eeaed59c009d",
            ),
            e(
                "a.lua",
                "45fcb867bb43ef043f1f1fe5ea6f95edd2099a71152259d35358a03907704c2c",
            ),
        ];
        let b = [a[1].clone(), a[0].clone()];
        assert_eq!(manifest_hash(&a), manifest_hash(&b));
    }
}
