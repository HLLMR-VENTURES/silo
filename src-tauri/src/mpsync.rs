//! Multiplayer mod-set sync — export a manifest of the host's active mods so a joiner
//! can verify their folder matches, turning FS's #1 join-blocker ("mod mismatch") from
//! a mystery into a checklist.
//!
//! Works WITH the file-hash constraint, not against it: FS requires every player to
//! have the same mod files, so we hash each active mod and diff host vs joiner.
//!
//! We use OUR OWN content hash (MD5 of the zip bytes), NOT FS's internal fileHash —
//! verified 0/25 that FS's logged hash is not a plain MD5 of the file, and reproducing
//! GIANTS' algorithm isn't needed here: both sides run Silo, so a consistent hash
//! answers "same bytes?" perfectly. It's byte-strict — a re-zip with identical contents
//! reads as different, which errs toward "re-download to be sure" (the safe direction
//! for MP), and we say so rather than pretend a near-match is fine.

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

/// A mod to include in / check against a manifest (host- or joiner-side).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModRef {
    pub tech_name: String,
    pub path: String,
    pub kind: String, // "zip" | "dir"
    pub version: Option<String>,
}

/// One line of the shareable manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManifestEntry {
    pub tech_name: String,
    pub version: Option<String>,
    pub hash: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Silo hash algorithm tag — lets a future version detect/upgrade the scheme.
    pub algo: String,
    pub mods: Vec<ManifestEntry>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Mismatch {
    pub tech_name: String,
    pub expected: String, // host's version (or hash)
    pub got: String,      // joiner's version (or hash)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifyReport {
    pub ok: bool,
    /// In the manifest, absent locally — the joiner needs to get these.
    pub missing: Vec<ManifestEntry>,
    /// Present locally but a different version.
    pub version_mismatch: Vec<Mismatch>,
    /// Same version, different bytes — re-download to be safe.
    pub hash_mismatch: Vec<Mismatch>,
    /// Present locally, not in the manifest — must be removed/disabled to join.
    pub extra: Vec<String>,
}

const ALGO: &str = "md5-zip-v1";

/// MD5 of a file and its size, read in 64 KB chunks: an active set can hold several
/// multi-GB maps, hashed in parallel, so a whole file never sits in memory.
fn md5_file(path: &std::path::Path) -> std::io::Result<(String, u64)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut ctx = md5::Context::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        ctx.consume(&buf[..n]);
        size += n as u64;
    }
    Ok((format!("{:x}", ctx.compute()), size))
}

/// MD5 of a mod's bytes + its size. Zip mods hash the file; unpacked (dev) mods can't
/// participate in a byte-exact manifest, so they hash empty and are flagged by size 0.
fn hash_one(m: &ModRef) -> ManifestEntry {
    let (hash, size) = if m.kind == "zip" {
        md5_file(std::path::Path::new(&m.path)).unwrap_or((String::new(), 0))
    } else {
        (String::new(), 0) // unpacked dir — not byte-comparable
    };
    ManifestEntry {
        tech_name: m.tech_name.clone(),
        version: m.version.clone(),
        hash,
        size,
    }
}

/// Hash a set of mods in parallel (off the UI thread; I/O-bound over the active set).
pub fn hash_mods(mods: &[ModRef]) -> Vec<ManifestEntry> {
    mods.par_iter().map(hash_one).collect()
}

/// Build a shareable manifest from the host's active set.
pub fn build_manifest(mods: &[ModRef]) -> Manifest {
    let mut mods = hash_mods(mods);
    mods.sort_by(|a, b| a.tech_name.cmp(&b.tech_name));
    Manifest {
        algo: ALGO.to_string(),
        mods,
    }
}

/// Diff a manifest against the joiner's already-hashed local set. Pure — unit-tested.
/// True when `text` is a Multiplayer check file (a `Manifest`), not a mod set or anything else.
pub fn is_manifest(text: &str) -> bool {
    serde_json::from_str::<Manifest>(text).is_ok()
}

pub fn diff(manifest: &[ManifestEntry], local: &[ManifestEntry]) -> VerifyReport {
    use std::collections::HashMap;
    let local_by: HashMap<&str, &ManifestEntry> =
        local.iter().map(|e| (e.tech_name.as_str(), e)).collect();
    let manifest_names: std::collections::HashSet<&str> =
        manifest.iter().map(|e| e.tech_name.as_str()).collect();

    let mut missing = Vec::new();
    let mut version_mismatch = Vec::new();
    let mut hash_mismatch = Vec::new();

    for want in manifest {
        match local_by.get(want.tech_name.as_str()) {
            None => missing.push(want.clone()),
            Some(have) => {
                if want.version != have.version {
                    version_mismatch.push(Mismatch {
                        tech_name: want.tech_name.clone(),
                        expected: want.version.clone().unwrap_or_default(),
                        got: have.version.clone().unwrap_or_default(),
                    });
                } else if want.hash != have.hash {
                    // Same version, different bytes — the subtle case that still blocks MP.
                    hash_mismatch.push(Mismatch {
                        tech_name: want.tech_name.clone(),
                        expected: want.hash.clone(),
                        got: have.hash.clone(),
                    });
                }
            }
        }
    }

    let mut extra: Vec<String> = local
        .iter()
        .filter(|e| !manifest_names.contains(e.tech_name.as_str()))
        .map(|e| e.tech_name.clone())
        .collect();
    extra.sort();

    VerifyReport {
        ok: missing.is_empty()
            && version_mismatch.is_empty()
            && hash_mismatch.is_empty()
            && extra.is_empty(),
        missing,
        version_mismatch,
        hash_mismatch,
        extra,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The streamed hash is the same MD5 as hashing the whole file at once, across chunk
    /// boundaries (the check-file format doesn't change).
    #[test]
    fn streamed_hash_matches_the_whole_file_hash() {
        let dir = std::env::temp_dir().join(format!("silo_mp_md5_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("FS25_Big.zip");
        let bytes: Vec<u8> = (0..200_003u32).map(|i| (i * 7 % 251) as u8).collect();
        std::fs::write(&p, &bytes).unwrap();
        let (hash, size) = md5_file(&p).unwrap();
        assert_eq!(hash, format!("{:x}", md5::compute(&bytes)));
        assert_eq!(size, bytes.len() as u64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_check_file_is_told_from_a_mod_set_file() {
        let m = build_manifest(&[]);
        assert!(is_manifest(&serde_json::to_string(&m).unwrap()));
        assert!(!is_manifest(
            r#"{ "silo": 1, "name": "x", "mods": ["FS25_A"] }"#
        ));
        assert!(!is_manifest(
            r#"{ "schema": "silo.collection/1", "name": "x", "mods": [] }"#
        ));
        assert!(!is_manifest("not json"));
    }

    fn e(tech: &str, ver: &str, hash: &str) -> ManifestEntry {
        ManifestEntry {
            tech_name: tech.into(),
            version: Some(ver.into()),
            hash: hash.into(),
            size: 1,
        }
    }

    #[test]
    fn identical_sets_are_ok() {
        let host = vec![e("FS25_A", "1.0", "aaa"), e("FS25_B", "1.0", "bbb")];
        let joiner = host.clone();
        let r = diff(&host, &joiner);
        assert!(r.ok);
        assert!(r.missing.is_empty() && r.extra.is_empty());
    }

    #[test]
    fn catches_every_kind_of_drift() {
        let host = vec![
            e("FS25_Present", "1.0", "same"),
            e("FS25_Missing", "1.0", "x"),       // joiner lacks it
            e("FS25_OldVer", "2.0", "h2"),       // joiner has older
            e("FS25_Rezip", "1.0", "hostbytes"), // same ver, different bytes
        ];
        let joiner = vec![
            e("FS25_Present", "1.0", "same"),
            e("FS25_OldVer", "1.0", "h1"),
            e("FS25_Rezip", "1.0", "joinerbytes"),
            e("FS25_Extra", "1.0", "e"), // joiner has one host doesn't
        ];
        let r = diff(&host, &joiner);
        assert!(!r.ok);
        assert_eq!(r.missing.len(), 1);
        assert_eq!(r.missing[0].tech_name, "FS25_Missing");
        assert_eq!(r.version_mismatch.len(), 1);
        assert_eq!(r.version_mismatch[0].tech_name, "FS25_OldVer");
        assert_eq!(r.hash_mismatch.len(), 1);
        assert_eq!(r.hash_mismatch[0].tech_name, "FS25_Rezip");
        assert_eq!(r.extra, vec!["FS25_Extra".to_string()]);
    }

    #[test]
    fn version_mismatch_takes_precedence_over_hash() {
        // Different version AND different hash → report as version (the actionable one),
        // not double-counted as a hash clash.
        let host = vec![e("FS25_X", "2.0", "h2")];
        let joiner = vec![e("FS25_X", "1.0", "h1")];
        let r = diff(&host, &joiner);
        assert_eq!(r.version_mismatch.len(), 1);
        assert_eq!(r.hash_mismatch.len(), 0);
    }
}
