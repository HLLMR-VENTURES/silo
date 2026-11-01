//! Read FS25 savegames and their bound mod lists from `careerSavegame.xml`.
//! Each `<mod modName version required fileHash/>` records what a save was built
//! on; `required="true"` mods are ones the save genuinely needs. Silo uses this to
//! build a loadout from a save and to verify a save's mods are present.

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveMod {
    pub mod_name: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub required: bool,
    pub file_hash: Option<String>,
    /// pdlc_* entries are paid DLC, not user mods Silo manages.
    pub is_dlc: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Savegame {
    pub index: u32,
    pub folder: String,
    pub name: String,
    pub map_title: Option<String>,
    /// When the game last saved it (`settings/saveDate`, e.g. "2026-08-02"), as the game wrote it.
    pub save_date: Option<String>,
    pub mods: Vec<SaveMod>,
    /// Why `careerSavegame.xml` couldn't be read (cut off, damaged, empty), or `None`. An
    /// unreadable save is still listed (and can be backed up), with no mods: a partial
    /// list would make "Use this save's mods" switch to a partial set (#354).
    pub error: Option<String>,
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|c| c.into_owned())
        })
}

/// Parse a `careerSavegame.xml` into name/map/mod-list. Fails, with a short reason, on any
/// XML error, a damaged `<mod>` entry, anything but one `<careerSavegame>` root, or a root
/// that never closes: the `<mod>` entries come last, so a cut-off file would otherwise
/// yield a partial mod list.
fn parse(xml: &str, index: u32, folder: &str) -> Result<Savegame, String> {
    let mut name = String::new();
    let mut map_title = None;
    let mut save_date = None;
    let mut mods = Vec::new();

    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut root_seen = false;
    let cut_off = || "careerSavegame.xml is cut off or damaged".to_string();

    // A tag's local name, once it's known to stand where it may: the one root element is
    // `<careerSavegame>`, and nothing comes after it.
    let open = |e: &BytesStart, stack: &[String], root_seen: &mut bool| {
        let local = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
        if stack.is_empty() {
            if *root_seen {
                return Err(cut_off());
            }
            if local != "careerSavegame" {
                return Err("careerSavegame.xml isn't a savegame file".to_string());
            }
            *root_seen = true;
        }
        Ok(local)
    };

    // A `<mod>` entry. One whose attributes are damaged, or a save-level entry (a child of
    // the root) with no `modName`, fails the parse rather than being skipped, which would
    // leave a partial list.
    let read_mod = |e: &BytesStart, stack: &[String], mods: &mut Vec<SaveMod>| {
        crate::xmltext::check_attributes(e).map_err(|_| cut_off())?;
        match attr(e, b"modName").filter(|n| !n.trim().is_empty()) {
            Some(mod_name) => mods.push(SaveMod {
                is_dlc: mod_name.starts_with("pdlc_"),
                title: attr(e, b"title"),
                version: attr(e, b"version"),
                required: attr(e, b"required").as_deref() == Some("true"),
                file_hash: attr(e, b"fileHash"),
                mod_name,
            }),
            None if stack.len() == 1 => return Err(cut_off()),
            None => {}
        }
        Ok::<(), String>(())
    };

    // Element text arrives split at every `&amp;` (#266): gather it, then read it when
    // the next tag arrives, while `stack` still names the element it belongs to.
    let mut run = crate::xmltext::TextRun::default();
    let mut take_text = |stack: &[String], run: &mut crate::xmltext::TextRun| {
        if run.is_empty() {
            return;
        }
        let txt = run.take();
        let txt = txt.trim();
        let last = stack.last().map(String::as_str).unwrap_or("");
        let parent = stack.iter().rev().nth(1).map(String::as_str).unwrap_or("");
        if parent == "settings" && last == "savegameName" {
            name = txt.to_string();
        } else if parent == "settings" && last == "mapTitle" {
            map_title = Some(txt.to_string());
        } else if parent == "settings" && last == "saveDate" && !txt.is_empty() {
            save_date = Some(txt.to_string());
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Err(_) => return Err(cut_off()),
            Ok(Event::Start(e)) => {
                take_text(&stack, &mut run);
                let local = open(&e, &stack, &mut root_seen)?;
                if local == "mod" {
                    read_mod(&e, &stack, &mut mods)?;
                }
                stack.push(local); // real elements have a matching End
            }
            Ok(Event::Empty(e)) => {
                // Self-closing (e.g. <mod .../>): handle attrs, don't push.
                take_text(&stack, &mut run);
                if open(&e, &stack, &mut root_seen)? == "mod" {
                    read_mod(&e, &stack, &mut mods)?;
                }
            }
            Ok(Event::End(_)) => {
                take_text(&stack, &mut run);
                stack.pop();
            }
            // Outside the root, only whitespace may stand.
            Ok(Event::Text(t)) if stack.is_empty() => {
                if !t.iter().all(u8::is_ascii_whitespace) {
                    return Err(cut_off());
                }
            }
            Ok(Event::GeneralRef(_) | Event::CData(_)) if stack.is_empty() => return Err(cut_off()),
            Ok(Event::Text(t)) => run.push_text(&t),
            Ok(Event::GeneralRef(r)) => run.push_ref(&r),
            Ok(Event::CData(c)) => run.push_str(&String::from_utf8_lossy(&c)),
            _ => {}
        }
        buf.clear();
    }
    if !root_seen {
        return Err("careerSavegame.xml is empty or has no savegame in it".into());
    }
    if !stack.is_empty() {
        return Err(cut_off());
    }

    if name.is_empty() {
        name = folder.to_string();
    }
    Ok(Savegame {
        index,
        folder: folder.to_string(),
        name,
        map_title,
        save_date,
        mods,
        error: None,
    })
}

/// A save whose `careerSavegame.xml` couldn't be read: listed under its folder name, with
/// no mods and the reason.
fn unreadable(index: u32, folder: &str, why: String) -> Savegame {
    Savegame {
        index,
        folder: folder.to_string(),
        name: folder.to_string(),
        map_title: None,
        save_date: None,
        mods: Vec::new(),
        error: Some(why),
    }
}

/// Read and parse one save's `careerSavegame.xml`; `None` when the folder has none (it isn't
/// a savegame). A file that exists but can't be read or parsed is an unreadable save.
fn read_save(xml_path: &Path, index: u32, folder: &str) -> Option<Savegame> {
    if !xml_path.is_file() {
        return None;
    }
    let parsed = std::fs::read_to_string(xml_path)
        .map_err(|_| "careerSavegame.xml couldn't be read".to_string())
        .and_then(|xml| parse(&xml, index, folder));
    Some(parsed.unwrap_or_else(|why| unreadable(index, folder, why)))
}

/// True when `p` is a directory itself, not a symlink or junction to one (links aren't followed).
fn is_plain_dir(p: &Path) -> bool {
    std::fs::symlink_metadata(p)
        .map(|m| m.file_type().is_dir())
        .unwrap_or(false)
}

/// List all savegames under the FS25 user dir, parsed and sorted by slot.
pub fn list_savegames(user_dir: &Path) -> Vec<Savegame> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(user_dir) else {
        return out;
    };
    for entry in rd.flatten() {
        // A real folder only: a symlink or junction named like a savegame points somewhere else.
        if !is_plain_dir(&entry.path()) {
            continue;
        }
        let folder = entry.file_name().to_string_lossy().into_owned();
        let Some(index) = folder
            .strip_prefix("savegame")
            .and_then(|n| n.parse::<u32>().ok())
        else {
            continue;
        };
        let xml_path = entry.path().join("careerSavegame.xml");
        if let Some(save) = read_save(&xml_path, index, &folder) {
            out.push(save);
        }
    }
    out.sort_by_key(|s| s.index);
    out
}

fn copy_dir(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for e in std::fs::read_dir(src)? {
        let e = e?;
        let to = dst.join(e.file_name());
        let kind = e.file_type()?;
        if kind.is_symlink() {
            // Never follow a link (symlink or junction) out of the save: what it points at
            // isn't part of the savegame and could be anywhere.
            continue;
        }
        if kind.is_dir() {
            copy_dir(&e.path(), &to)?;
        } else {
            std::fs::copy(e.path(), &to)?;
        }
    }
    Ok(())
}

/// The folder of savegame `folder`, if it's a plain name that `list_savegames` reports and a
/// real folder (not a symlink or junction). A backup can then only ever copy a real savegame,
/// never another folder a caller names or a link points at.
pub fn listed_folder(user_dir: &Path, folder: &str) -> Result<PathBuf, String> {
    let not_found =
        || format!("Savegame \"{folder}\" wasn't found. Refresh the list and try again.");
    if crate::paths::safe_file_name(folder).is_err()
        || !list_savegames(user_dir).iter().any(|s| s.folder == folder)
    {
        return Err(not_found());
    }
    let p = user_dir.join(folder);
    if !is_plain_dir(&p) {
        return Err(not_found());
    }
    Ok(p)
}

/// Back up a savegame folder to `SiloBackups/<folder>_<epoch>` (a plain copy —
/// non-destructive; the original is untouched). Returns the backup path.
pub fn backup(user_dir: &Path, folder: &str) -> Result<String, String> {
    let src = listed_folder(user_dir, folder)?;
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let dst = user_dir
        .join("SiloBackups")
        .join(format!("{folder}_{secs}"));
    copy_dir(&src, &dst).map_err(|e| e.to_string())?;
    crate::fslog::moved("backup-save", &src, &dst);
    Ok(dst.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<careerSavegame>
        <settings>
            <savegameName>My game save</savegameName>
            <mapTitle>Riverbend Springs</mapTitle>
            <saveDate>2026-08-02</saveDate>
        </settings>
        <mod modName="pdlc_vredoPack" title="Vredo Pack" version="1.1.0.0" required="false" fileHash="abc"/>
        <mod modName="FS25_precisionFarming" title="Precision Farming" version="1.5.0.0" required="true" fileHash="def"/>
    </careerSavegame>"#;

    #[test]
    fn parses_name_and_mods() {
        let s = parse(SAMPLE, 1, "savegame1").unwrap();
        assert_eq!(s.name, "My game save");
        assert_eq!(s.map_title.as_deref(), Some("Riverbend Springs"));
        assert_eq!(s.save_date.as_deref(), Some("2026-08-02"));
        assert_eq!(s.mods.len(), 2);
        let pf = s
            .mods
            .iter()
            .find(|m| m.mod_name == "FS25_precisionFarming")
            .unwrap();
        assert!(pf.required && !pf.is_dlc);
        let dlc = s
            .mods
            .iter()
            .find(|m| m.mod_name == "pdlc_vredoPack")
            .unwrap();
        assert!(dlc.is_dlc && !dlc.required);
    }

    /// #266: a save or map name with `&amp;` keeps both halves (the old parser kept
    /// only the last text run, so "Elm & Ash Farm" showed as "Ash Farm").
    #[test]
    fn keeps_names_with_an_escaped_ampersand() {
        let xml = r#"<careerSavegame><settings>
            <savegameName>Elm &amp; Ash Farm</savegameName>
            <mapTitle>Hills &amp; Dales</mapTitle>
            <saveDate>2026-08-02</saveDate>
        </settings></careerSavegame>"#;
        let s = parse(xml, 2, "savegame2").unwrap();
        assert_eq!(s.name, "Elm & Ash Farm");
        assert_eq!(s.map_title.as_deref(), Some("Hills & Dales"));
        assert_eq!(s.save_date.as_deref(), Some("2026-08-02"));
    }

    /// A real-shaped careerSavegame.xml: BOM, declaration, the root's attributes, settings,
    /// then the `<mod>` entries last, as FS25 writes it.
    fn career(mods: usize) -> String {
        let mut xml = String::from("\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\"?>\n<careerSavegame revision=\"2\" valid=\"true\">\n    <settings>\n        <savegameName>My game save</savegameName>\n        <mapTitle>Riverbend Springs</mapTitle>\n        <saveDate>2026-08-02</saveDate>\n    </settings>\n    <statistics>\n        <money>500000</money>\n    </statistics>\n");
        for i in 0..mods {
            xml.push_str(&format!("    <mod modName=\"FS25_mod{i}\" title=\"Mod &amp; {i}\" version=\"1.0.0.0\" required=\"true\" fileHash=\"h{i}\"/>\n"));
        }
        xml.push_str("</careerSavegame>\n");
        xml
    }

    #[test]
    fn parses_a_real_shaped_save() {
        let s = parse(&career(3), 1, "savegame1").unwrap();
        assert_eq!(s.name, "My game save");
        assert_eq!(s.mods.len(), 3);
        assert_eq!(s.mods[2].title.as_deref(), Some("Mod & 2"));
        assert!(s.error.is_none());
    }

    /// #354: a file cut off anywhere (mid-tag, between mods, before the root closes) is a
    /// failed parse, never a save with part of its mod list.
    #[test]
    fn a_truncated_save_fails_to_parse() {
        let full = career(3);
        let mod_start = full.find("<mod ").unwrap();
        for cut in [
            full.len() - 3,                           // inside </careerSavegame>
            full.rfind("</careerSavegame>").unwrap(), // all mods, no root end
            full.rfind("<mod ").unwrap(),             // between mods
            mod_start + 20,                           // inside the first <mod
            full.find("<statistics>").unwrap(),       // before any mod
            full.find("<savegameName>").unwrap() + 5, // inside settings
        ] {
            let err = parse(&full[..cut], 1, "savegame1").unwrap_err();
            assert!(err.contains("cut off"), "cut at {cut}: {err}");
        }
    }

    #[test]
    fn an_empty_or_foreign_file_fails_to_parse() {
        for (xml, why) in [
            ("", "empty"),
            ("\u{feff}<?xml version=\"1.0\"?>\n", "empty"),
            ("<x/>", "isn't a savegame"),
            ("<game><mod modName=\"FS25_a\"/></game>", "isn't a savegame"),
            ("<mod modName=\"FS25_a\"/>", "isn't a savegame"),
        ] {
            let err = parse(xml, 1, "savegame1").unwrap_err();
            assert!(err.contains(why), "{xml:?}: {err}");
        }
    }

    /// #354: a damaged `<mod>` entry, or anything after the root closes, fails the parse
    /// rather than leaving out the entries it can't read.
    #[test]
    fn a_damaged_mod_entry_or_content_after_the_root_fails_to_parse() {
        let full = career(2);
        let end = full.rfind("</careerSavegame>").unwrap();
        for extra in [
            "<mod modName=\"FS25_&missing;\"/>",
            "<mod modName=FS25_x/>",
            "<mod modName=\"FS25_x\" modName=\"FS25_y\"/>",
            "<mod modName=\"a & b\"/>",
            "<mod title=\"Missing identity\" required=\"true\"/>",
            "<mod modName=\" \" title=\"Blank\"/>",
        ] {
            let xml = format!("{}    {extra}\n{}", &full[..end], &full[end..]);
            let err = parse(&xml, 1, "savegame1").unwrap_err();
            assert!(err.contains("cut off or damaged"), "{extra}: {err}");
        }
        for after in [
            "<mod modName=\"FS25_b\"/>",
            "<careerSavegame/>",
            "trailing",
            "&amp;",
        ] {
            let xml = format!("{full}{after}");
            let err = parse(&xml, 1, "savegame1").unwrap_err();
            assert!(err.contains("cut off or damaged"), "{after}: {err}");
        }
        // Whitespace and comments after the root are fine.
        assert!(parse(&format!("{full}\n<!-- c -->\n"), 1, "savegame1").is_ok());
    }

    /// #354: a name written as CDATA is read.
    #[test]
    fn reads_a_cdata_save_name() {
        let xml = "<careerSavegame><settings><savegameName><![CDATA[Elm & Ash]]></savegameName></settings></careerSavegame>";
        assert_eq!(parse(xml, 1, "savegame1").unwrap().name, "Elm & Ash");
    }

    /// #354: an unreadable save is listed (so it can still be backed up), with no mods and
    /// the reason; a folder with no careerSavegame.xml isn't a save.
    #[test]
    fn an_unreadable_save_is_listed_with_its_reason() {
        let root = std::env::temp_dir().join(format!("silo_sg_unreadable_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for n in 1..=3 {
            std::fs::create_dir_all(root.join(format!("savegame{n}"))).unwrap();
        }
        let full = career(2);
        std::fs::write(root.join("savegame1/careerSavegame.xml"), &full).unwrap();
        let cut = &full[..full.rfind("<mod ").unwrap()];
        std::fs::write(root.join("savegame2/careerSavegame.xml"), cut).unwrap();
        // savegame3 has no careerSavegame.xml.

        let saves = list_savegames(&root);
        assert_eq!(saves.len(), 2);
        assert_eq!(saves[0].mods.len(), 2);
        assert!(saves[0].error.is_none());
        assert_eq!(saves[1].folder, "savegame2");
        assert!(saves[1].mods.is_empty(), "no partial mod list");
        assert!(saves[1].error.as_deref().unwrap().contains("cut off"));
        assert!(backup(&root, "savegame2").is_ok());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn backup_copies_savegame() {
        let root = std::env::temp_dir().join("silo_sg_backup_test");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("savegame1")).unwrap();
        std::fs::write(root.join("savegame1/careerSavegame.xml"), "<x/>").unwrap();
        let dst = backup(&root, "savegame1").unwrap();
        assert!(Path::new(&dst).join("careerSavegame.xml").is_file());
        // Original untouched.
        assert!(root.join("savegame1/careerSavegame.xml").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// A directory link: a symlink on Linux/macOS, an NTFS junction on Windows (no privilege
    /// needed, and what a user or another tool would most likely have made).
    fn link_dir(target: &Path, link: &Path) {
        #[cfg(unix)]
        std::os::unix::fs::symlink(target, link).unwrap();
        #[cfg(windows)]
        junction::create(target, link).unwrap();
    }

    #[test]
    fn a_linked_savegame_folder_is_neither_listed_nor_backed_up() {
        let base = std::env::temp_dir().join(format!("silo_sg_link_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("user");
        let elsewhere = base.join("elsewhere");
        std::fs::create_dir_all(root.join("savegame1")).unwrap();
        std::fs::write(root.join("savegame1/careerSavegame.xml"), SAMPLE).unwrap();
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("careerSavegame.xml"), SAMPLE).unwrap();
        link_dir(&elsewhere, &root.join("savegame3"));

        let listed: Vec<String> = list_savegames(&root)
            .into_iter()
            .map(|s| s.folder)
            .collect();
        assert_eq!(listed, vec!["savegame1".to_string()]);
        assert!(listed_folder(&root, "savegame3").is_err());
        assert!(backup(&root, "savegame3").is_err());
        assert!(!root.join("SiloBackups").exists(), "nothing was copied");
        assert!(
            elsewhere.join("careerSavegame.xml").is_file(),
            "the target is untouched"
        );

        #[cfg(windows)]
        junction::delete(root.join("savegame3")).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn backup_does_not_follow_links_inside_a_savegame() {
        let base = std::env::temp_dir().join(format!("silo_sg_inner_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let root = base.join("user");
        let outside = base.join("outside");
        std::fs::create_dir_all(root.join("savegame1")).unwrap();
        std::fs::write(root.join("savegame1/careerSavegame.xml"), SAMPLE).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        std::fs::write(outside.join("secret.txt"), "not part of the save").unwrap();
        link_dir(&outside, &root.join("savegame1/escape"));

        let dst = PathBuf::from(backup(&root, "savegame1").unwrap());
        assert!(dst.join("careerSavegame.xml").is_file());
        assert!(
            std::fs::symlink_metadata(dst.join("escape")).is_err(),
            "the link and what it points at stay out of the backup"
        );

        #[cfg(windows)]
        junction::delete(root.join("savegame1/escape")).unwrap();
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn backup_refuses_anything_but_a_listed_savegame() {
        let root = std::env::temp_dir().join(format!("silo_sg_refuse_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("savegame1")).unwrap();
        std::fs::write(root.join("savegame1/careerSavegame.xml"), SAMPLE).unwrap();
        std::fs::create_dir_all(root.join("savegame2")).unwrap(); // no careerSavegame.xml: not listed
        std::fs::create_dir_all(root.join("mods")).unwrap();
        let outside = std::env::temp_dir().join(format!("silo_sg_outside_{}", std::process::id()));
        std::fs::create_dir_all(&outside).unwrap();
        let outside_str = outside.to_string_lossy().into_owned();
        for bad in [
            "",
            "..",
            "../x",
            "mods",
            "savegame2",
            "savegame1/..",
            "savegame1\\..\\mods",
            outside_str.as_str(),
        ] {
            assert!(backup(&root, bad).is_err(), "should refuse {bad:?}");
        }
        assert!(!root.join("SiloBackups").exists(), "nothing was copied");
        let dst = backup(&root, "savegame1").unwrap();
        assert!(Path::new(&dst).join("careerSavegame.xml").is_file());
        let _ = std::fs::remove_dir_all(&root);
        let _ = std::fs::remove_dir_all(&outside);
    }
}
