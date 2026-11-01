//! Mod icon extraction: pull a mod's `iconFilename` out of its archive/dir,
//! decode it (DDS BC1/BC3/BC7 or PNG/JPG), downscale to a thumbnail, and encode
//! PNG bytes. All CPU/IO — runs off the UI thread. Decoded thumbs get cached to
//! disk by the caller so this only runs once per mod version.

use image::imageops::FilterType;
use image::ImageFormat;
use std::io::Cursor;
use std::path::Path;

/// Thumbnail edge length. Mod icons are typically 256² (store) / 512²; a 128²
/// thumb is crisp in the list and tiny to cache.
pub const THUMB: u32 = 128;

/// Return a cached PNG thumbnail as a `data:` URL, decoding + caching on first
/// use. Keyed by mod path + mtime + icon name, so it only decodes once per mod
/// version and survives app restarts. Returns None on any failure (missing icon,
/// undecodable format) so the UI just falls back to the placeholder tile.
pub fn cached_data_url(
    cache_dir: &Path,
    mod_path: &Path,
    kind: &str,
    icon_filename: &str,
) -> Option<String> {
    let png = cached_thumbnail(cache_dir, mod_path, kind, icon_filename).ok()?;
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
    Some(format!("data:image/png;base64,{b64}"))
}

/// Disk-cached thumbnail PNG bytes.
pub fn cached_thumbnail(
    cache_dir: &Path,
    mod_path: &Path,
    kind: &str,
    icon_filename: &str,
) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(mod_path).map_err(|e| e.to_string())?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);

    let cached = cache_dir.join(format!("{}.png", cache_key(mod_path, mtime, icon_filename)));

    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(bytes);
    }
    let png = extract_icon_png(mod_path, kind, icon_filename)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&cached, &png);
    Ok(png)
}

/// Cache file stem for a mod's icon: the same across runs, builds and Rust versions.
fn cache_key(mod_path: &Path, mtime_ms: u128, icon_filename: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(mod_path.to_string_lossy().as_bytes());
    h.update([0u8]);
    h.update(mtime_ms.to_le_bytes());
    h.update([0u8]);
    h.update(icon_filename.as_bytes());
    h.finalize()[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Extract + decode + thumbnail a mod's icon, returning PNG bytes.
pub fn extract_icon_png(
    mod_path: &Path,
    kind: &str,
    icon_filename: &str,
) -> Result<Vec<u8>, String> {
    let raw = read_icon_bytes(mod_path, kind, icon_filename)?;
    let img = decode(&raw, icon_filename)?;
    let thumb = img.resize(THUMB, THUMB, FilterType::Triangle);
    let mut out = Cursor::new(Vec::new());
    thumb
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

/// modDesc references the icon relative to the mod root; the extension it lists
/// doesn't always match the file on disk (e.g. `.dds` listed, `.png` shipped), so
/// try the swapped extension too.
fn icon_candidates(name: &str) -> Vec<String> {
    let name = name.replace('\\', "/");
    let mut v = vec![name.clone()];
    let lower = name.to_lowercase();
    if lower.ends_with(".dds") {
        v.push(format!("{}.png", &name[..name.len() - 4]));
    } else if lower.ends_with(".png") {
        v.push(format!("{}.dds", &name[..name.len() - 4]));
    }
    v
}

/// Largest icon file read. Real ones are a few MB; the cap keeps a huge "icon" from being read
/// whole into memory.
const ICON_LIMIT: u64 = 64 * 1024 * 1024;

fn read_icon_bytes(mod_path: &Path, kind: &str, icon_filename: &str) -> Result<Vec<u8>, String> {
    match kind {
        "zip" => {
            let f = std::fs::File::open(mod_path).map_err(|e| e.to_string())?;
            let mut ar = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
            for cand in icon_candidates(icon_filename) {
                if let Ok(entry) = ar.by_name(&cand) {
                    // Cap the read so a zip-bombed "icon" can't exhaust memory.
                    return crate::scan::read_capped(entry, ICON_LIMIT, &cand);
                }
            }
            Err(format!("icon '{icon_filename}' not found in archive"))
        }
        _ => {
            for cand in icon_candidates(icon_filename) {
                // Only a file inside the mod's own folder, capped like the zip branch.
                if let Some(f) = crate::paths::open_inside(mod_path, &cand) {
                    return crate::scan::read_capped(f, ICON_LIMIT, &cand);
                }
            }
            Err(format!("icon '{icon_filename}' not found"))
        }
    }
}

fn decode(raw: &[u8], name: &str) -> Result<image::DynamicImage, String> {
    let is_dds = raw.starts_with(b"DDS ") || name.to_lowercase().ends_with(".dds");
    if is_dds {
        let dds = image_dds::ddsfile::Dds::read(Cursor::new(raw)).map_err(|e| e.to_string())?;
        let rgba = image_dds::image_from_dds(&dds, 0).map_err(|e| e.to_string())?;
        Ok(image::DynamicImage::ImageRgba8(rgba))
    } else {
        image::load_from_memory(raw).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_icon_cache_key_is_stable() {
        let k = cache_key(std::path::Path::new("/mods/FS25_A.zip"), 1234, "icon.dds");
        // sha256("/mods/FS25_A.zip" 0x00 1234u128-LE 0x00 "icon.dds"), first 16 bytes as hex.
        assert_eq!(k, "6c7699e483c86ced8f1ae81b22b4740b");
        assert_eq!(k.len(), 32);
        assert_ne!(
            k,
            cache_key(std::path::Path::new("/mods/FS25_A.zip"), 1235, "icon.dds")
        );
    }

    /// An unpacked mod's icon is read only from inside its own folder, and capped like a zip's.
    #[test]
    fn a_folder_mod_reads_its_icon_only_inside_its_folder() {
        let base = std::env::temp_dir().join(format!("silo_icon_inside_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let dir = base.join("FS25_A");
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("icons").join("icon.png"), b"png").unwrap();
        std::fs::write(base.join("outside.png"), b"png").unwrap();

        // The listed `.dds` falls back to the shipped `.png`, as before.
        assert_eq!(
            read_icon_bytes(&dir, "dir", r"icons\icon.dds").unwrap(),
            b"png"
        );
        assert!(read_icon_bytes(&dir, "dir", "../outside.png").is_err());
        let absolute = base.join("outside.png");
        assert!(read_icon_bytes(&dir, "dir", absolute.to_str().unwrap()).is_err());

        let big = std::fs::File::create(dir.join("big.png")).unwrap();
        big.set_len(ICON_LIMIT + 1).unwrap();
        drop(big);
        assert!(read_icon_bytes(&dir, "dir", "big.png").is_err());
        let _ = std::fs::remove_dir_all(&base);
    }
}
