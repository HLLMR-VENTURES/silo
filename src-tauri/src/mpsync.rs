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
    /// MD5 of the zip, or `None` when Silo has no fingerprint for it (an unpacked folder, or a
    /// zip it couldn't read). On disk that is `""` with size 0, as 1.0.3 wrote it, so check
    /// files stay readable both ways.
    #[serde(with = "empty_hash")]
    pub hash: Option<String>,
    pub size: u64,
}

/// `hash` on disk: a string, `""` for "no fingerprint". A missing fingerprint never reaches
/// the comparison as `""`, so two of them can't read as a match (#355).
mod empty_hash {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Option<String>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(v.as_deref().unwrap_or(""))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.filter(|h| !h.is_empty()))
    }
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
    /// Same version, but one side (or both) has no fingerprint, so Silo can't tell whether the
    /// files match. Never counted as a match.
    pub unverifiable: Vec<Unverifiable>,
    /// Present locally, not in the manifest — must be removed/disabled to join.
    pub extra: Vec<String>,
}

/// A mod Silo couldn't compare, because a copy of it has no fingerprint.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unverifiable {
    pub tech_name: String,
    /// The server's check file has no fingerprint for it.
    pub server: bool,
    /// Silo couldn't fingerprint the joiner's copy (an unpacked folder, or an unreadable zip).
    pub yours: bool,
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
/// participate in a byte-exact manifest, and a zip that can't be read has no trustworthy
/// fingerprint: both get `hash: None` (size 0), which never compares as a match.
fn hash_one(m: &ModRef) -> ManifestEntry {
    let fingerprint = if m.kind == "zip" {
        match md5_file(std::path::Path::new(&m.path)) {
            Ok(f) => Some(f),
            Err(e) => {
                log::warn!("mp check: couldn't read {} ({}): {e}", m.tech_name, m.path);
                None
            }
        }
    } else {
        None // unpacked dir — not byte-comparable
    };
    let (hash, size) = match fingerprint {
        Some((hash, size)) => (Some(hash), size),
        None => (None, 0),
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

/// The packaged mods of `mods` that went into `entries` without a fingerprint: their zip
/// couldn't be read, so the host is told (unpacked folders are expected to have none).
pub fn unreadable(mods: &[ModRef], entries: &[ManifestEntry]) -> Vec<String> {
    use std::collections::HashSet;
    let zips: HashSet<&str> = mods
        .iter()
        .filter(|m| m.kind == "zip")
        .map(|m| m.tech_name.as_str())
        .collect();
    let mut out: Vec<String> = entries
        .iter()
        .filter(|e| e.hash.is_none() && zips.contains(e.tech_name.as_str()))
        .map(|e| e.tech_name.clone())
        .collect();
    out.sort();
    out.dedup();
    out
}

/// True when `text` is a Multiplayer check file (a `Manifest`), not a mod set or anything else.
pub fn is_manifest(text: &str) -> bool {
    serde_json::from_str::<Manifest>(text).is_ok()
}

/// Diff a manifest against the joiner's already-hashed local set. Pure — unit-tested.
/// A mod matches only when both sides have a fingerprint and hash and size agree.
pub fn diff(manifest: &[ManifestEntry], local: &[ManifestEntry]) -> VerifyReport {
    use std::collections::HashMap;
    let local_by: HashMap<&str, &ManifestEntry> =
        local.iter().map(|e| (e.tech_name.as_str(), e)).collect();
    let manifest_names: std::collections::HashSet<&str> =
        manifest.iter().map(|e| e.tech_name.as_str()).collect();

    let mut missing = Vec::new();
    let mut version_mismatch = Vec::new();
    let mut hash_mismatch = Vec::new();
    let mut unverifiable = Vec::new();

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
                } else {
                    match (&want.hash, &have.hash) {
                        (Some(w), Some(h)) if w == h && want.size == have.size => {}
                        // Same version, different bytes — the subtle case that still blocks MP.
                        (Some(w), Some(h)) => hash_mismatch.push(Mismatch {
                            tech_name: want.tech_name.clone(),
                            expected: w.clone(),
                            got: h.clone(),
                        }),
                        // No fingerprint on one side: neither a match nor a known difference.
                        (w, h) => unverifiable.push(Unverifiable {
                            tech_name: want.tech_name.clone(),
                            server: w.is_none(),
                            yours: h.is_none(),
                        }),
                    }
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
            && unverifiable.is_empty()
            && extra.is_empty(),
        missing,
        version_mismatch,
        hash_mismatch,
        unverifiable,
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
            hash: Some(hash.into()),
            size: 1,
        }
    }

    /// An entry with no fingerprint (a folder, or a zip that couldn't be read).
    fn none(tech: &str, ver: &str) -> ManifestEntry {
        ManifestEntry {
            tech_name: tech.into(),
            version: Some(ver.into()),
            hash: None,
            size: 0,
        }
    }

    fn zip(tech: &str, path: &str) -> ModRef {
        ModRef {
            tech_name: tech.into(),
            path: path.into(),
            kind: "zip".into(),
            version: Some("1.0".into()),
        }
    }

    /// Both sides unpacked (or both unreadable) used to compare `"" == ""` and say
    /// "You match" (#355).
    #[test]
    fn two_missing_fingerprints_are_unverifiable_not_a_match() {
        let host = vec![none("FS25_Dev", "1.0")];
        let joiner = vec![none("FS25_Dev", "1.0")];
        let r = diff(&host, &joiner);
        assert!(!r.ok);
        assert!(r.hash_mismatch.is_empty());
        assert_eq!(r.unverifiable.len(), 1);
        assert!(r.unverifiable[0].server && r.unverifiable[0].yours);
    }

    #[test]
    fn one_missing_fingerprint_is_unverifiable_and_says_which_side() {
        let r = diff(&[none("FS25_A", "1.0")], &[e("FS25_A", "1.0", "aaa")]);
        assert!(!r.ok);
        assert_eq!(r.unverifiable.len(), 1);
        assert!(r.unverifiable[0].server && !r.unverifiable[0].yours);

        let r = diff(&[e("FS25_A", "1.0", "aaa")], &[none("FS25_A", "1.0")]);
        assert!(!r.ok);
        assert!(!r.unverifiable[0].server && r.unverifiable[0].yours);
    }

    #[test]
    fn same_hash_different_size_is_a_mismatch() {
        let host = vec![e("FS25_A", "1.0", "aaa")];
        let mut joiner = host.clone();
        joiner[0].size = 2;
        let r = diff(&host, &joiner);
        assert!(!r.ok);
        assert_eq!(r.hash_mismatch.len(), 1);
    }

    #[test]
    fn version_mismatch_wins_over_a_missing_fingerprint() {
        let r = diff(&[none("FS25_A", "2.0")], &[none("FS25_A", "1.0")]);
        assert_eq!(r.version_mismatch.len(), 1);
        assert!(r.unverifiable.is_empty());
    }

    /// A 1.0.3 check file wrote a folder mod as `"hash": ""`, size 0. It still loads, that entry
    /// reads as "no fingerprint", and a new file writes the same shape.
    #[test]
    fn check_files_from_1_0_3_still_load() {
        let old = r#"{ "algo": "md5-zip-v1", "mods": [
            { "techName": "FS25_Zip", "version": "1.0", "hash": "abc", "size": 10 },
            { "techName": "FS25_Dev", "version": null, "hash": "", "size": 0 }
        ] }"#;
        assert!(is_manifest(old));
        let m: Manifest = serde_json::from_str(old).unwrap();
        assert_eq!(m.mods[0].hash.as_deref(), Some("abc"));
        assert_eq!(m.mods[1].hash, None);
        let back = serde_json::to_value(&m).unwrap();
        assert_eq!(back["mods"][0]["hash"], "abc");
        assert_eq!(back["mods"][1]["hash"], "");
        assert_eq!(back["mods"][1]["size"], 0);
    }

    #[test]
    fn an_unreadable_zip_has_no_fingerprint_and_is_reported() {
        let dir = std::env::temp_dir().join(format!("silo_mp_unread_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let ok = dir.join("FS25_Ok.zip");
        std::fs::write(&ok, b"zipbytes").unwrap();
        let gone = dir.join("FS25_Gone.zip"); // never written: the read fails
        let mods = vec![
            zip("FS25_Ok", ok.to_str().unwrap()),
            zip("FS25_Gone", gone.to_str().unwrap()),
            ModRef {
                tech_name: "FS25_Dev".into(),
                path: dir.to_string_lossy().into(),
                kind: "dir".into(),
                version: None,
            },
        ];
        let m = build_manifest(&mods);
        let gone_entry = m.mods.iter().find(|e| e.tech_name == "FS25_Gone").unwrap();
        assert_eq!(gone_entry.hash, None);
        assert_eq!(gone_entry.size, 0);
        let ok_entry = m.mods.iter().find(|e| e.tech_name == "FS25_Ok").unwrap();
        assert!(ok_entry.hash.is_some());
        // Folders are expected to have no fingerprint; only the zip counts as unreadable.
        assert_eq!(unreadable(&mods, &m.mods), vec!["FS25_Gone".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
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
