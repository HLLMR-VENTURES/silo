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
/// use. Keyed by mod path + icon file identity + icon name, so it only decodes once per
/// icon version and survives app restarts. Returns None on any failure (missing icon,
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
    let (mtime, size) = icon_stamp(mod_path, kind, icon_filename)?;
    let cached = cache_dir.join(format!(
        "{}.png",
        cache_key(mod_path, mtime, size, icon_filename)
    ));

    if let Ok(bytes) = std::fs::read(&cached) {
        return Ok(bytes);
    }
    let png = extract_icon_png(mod_path, kind, icon_filename)?;
    let _ = std::fs::create_dir_all(cache_dir);
    let _ = std::fs::write(&cached, &png);
    Ok(png)
}

/// The `(mtime_ms, size)` the thumbnail cache is keyed on: the zip's own for a zip, which
/// changes with any rebuild. For a folder mod, the icon file's own: the folder's mtime doesn't
/// change when the icon inside it is edited in place (#353).
fn icon_stamp(mod_path: &Path, kind: &str, icon_filename: &str) -> Result<(u128, u64), String> {
    let meta = match kind {
        "zip" => std::fs::metadata(mod_path).map_err(|e| e.to_string())?,
        _ => {
            let (f, _) = open_dir_icon(mod_path, icon_filename)?;
            f.metadata().map_err(|e| e.to_string())?
        }
    };
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    Ok((mtime, meta.len()))
}

/// Cache file stem for a mod's icon: the same across runs, builds and Rust versions.
fn cache_key(mod_path: &Path, mtime_ms: u128, size: u64, icon_filename: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(mod_path.to_string_lossy().as_bytes());
    h.update([0u8]);
    h.update(mtime_ms.to_le_bytes());
    h.update(size.to_le_bytes());
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
    let img = decode(&raw)?;
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
            let (f, cand) = open_dir_icon(mod_path, icon_filename)?;
            crate::scan::read_capped(f, ICON_LIMIT, &cand)
        }
    }
}

/// Open a folder mod's icon, trying the swapped extension too (see [`icon_candidates`]): only
/// a file inside the mod's own folder. Returns the open file and the name it was found under.
fn open_dir_icon(mod_path: &Path, icon_filename: &str) -> Result<(std::fs::File, String), String> {
    for cand in icon_candidates(icon_filename) {
        if let Some(f) = crate::paths::open_inside(mod_path, &cand) {
            return Ok((f, cand));
        }
    }
    Err(format!("icon '{icon_filename}' not found"))
}

/// Decode an icon by what the bytes are, not what they're called: the swapped-extension
/// fallback reads a PNG under a `.dds` name (and the reverse), so only the `DDS ` magic picks
/// the DDS decoder; anything else goes to `image`, which recognises PNG and JPEG itself.
fn decode(raw: &[u8]) -> Result<image::DynamicImage, String> {
    if raw.starts_with(b"DDS ") {
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
        let zip = std::path::Path::new("/mods/FS25_A.zip");
        let k = cache_key(zip, 1234, 99, "icon.dds");
        // sha256("/mods/FS25_A.zip" 0x00 1234u128-LE 99u64-LE 0x00 "icon.dds"), first 16 bytes.
        assert_eq!(k, "81176ae74d9c77a401fb798880379bcc");
        assert_eq!(k.len(), 32);
        assert_ne!(k, cache_key(zip, 1235, 99, "icon.dds"));
        // A rebuilt zip with the same mtime but another size gets its own thumbnail (#353).
        assert_ne!(k, cache_key(zip, 1234, 100, "icon.dds"));
    }

    /// A one-colour `w`×`h` PNG.
    fn png(w: u32, h: u32, rgb: [u8; 3]) -> Vec<u8> {
        let img = image::RgbImage::from_pixel(w, h, image::Rgb(rgb));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let base = std::env::temp_dir().join(format!("silo_icon_{tag}_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).unwrap();
        base
    }

    /// #353: the decoder is picked by the bytes, so a PNG read under its listed `.dds` name (the
    /// swapped-extension fallback) decodes instead of failing in the DDS reader.
    #[test]
    fn a_png_shipped_for_a_listed_dds_decodes() {
        let raw = png(4, 4, [255, 0, 0]);
        assert_eq!(decode(&raw).unwrap().width(), 4);
        assert!(
            decode(b"DDS not really").is_err(),
            "DDS magic goes to the DDS reader"
        );

        let base = temp("dds_png");
        let dir = base.join("FS25_A");
        std::fs::create_dir_all(dir.join("icons")).unwrap();
        std::fs::write(dir.join("icons").join("icon.png"), &raw).unwrap();
        assert!(extract_icon_png(&dir, "dir", "icons/icon.dds").is_ok());

        let zip = base.join("FS25_B.zip");
        let mut w = zip::ZipWriter::new(std::fs::File::create(&zip).unwrap());
        w.start_file("icon.png", zip::write::SimpleFileOptions::default())
            .unwrap();
        std::io::Write::write_all(&mut w, &raw).unwrap();
        w.finish().unwrap();
        assert!(extract_icon_png(&zip, "zip", "icon.dds").is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    /// #353: a folder mod's thumbnail follows edits to its icon file, which don't change the
    /// folder's own mtime.
    #[test]
    fn a_folder_mods_thumbnail_follows_its_icon_file() {
        let base = temp("dir_refresh");
        let dir = base.join("FS25_A");
        let cache = base.join("cache");
        std::fs::create_dir_all(&dir).unwrap();
        let icon = dir.join("icon.png");
        std::fs::write(&icon, png(4, 4, [255, 0, 0])).unwrap();
        let red = cached_thumbnail(&cache, &dir, "dir", "icon.png").unwrap();
        assert_eq!(
            cached_thumbnail(&cache, &dir, "dir", "icon.png").unwrap(),
            red,
            "served from the cache"
        );

        // Rewrite the icon in place; the folder's own mtime doesn't move.
        let folder_mtime = std::fs::metadata(&dir).unwrap().modified().unwrap();
        let later = std::fs::metadata(&icon).unwrap().modified().unwrap()
            + std::time::Duration::from_secs(5);
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&icon)
            .unwrap();
        std::io::Write::write_all(&mut f, &png(4, 4, [0, 0, 255])).unwrap();
        f.set_modified(later).unwrap();
        drop(f);
        assert_eq!(
            std::fs::metadata(&dir).unwrap().modified().unwrap(),
            folder_mtime
        );

        let blue = cached_thumbnail(&cache, &dir, "dir", "icon.png").unwrap();
        assert_ne!(blue, red, "the edited icon is decoded again");
        let _ = std::fs::remove_dir_all(&base);
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
