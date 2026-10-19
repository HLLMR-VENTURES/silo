//! Read the authoritative FS store category from a mod's storeItem XML. A vehicle/
//! tool/placeable XML carries `<storeData><category>tractorsM</category>…`, which is
//! GIANTS' own taxonomy — far more reliable than guessing from names.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use std::io::Read;
use std::path::Path;

/// Read the `<storeData><category>` of the first storeItem, if any.
pub fn first_store_category(mod_path: &Path, kind: &str, store_files: &[String]) -> Option<String> {
    // Skip mission-vehicle store items — a contract/script mod (e.g. AdditionalContracts)
    // ships `missionVehicles/*.xml` for its contracts; those aren't the mod's own shop
    // presence, and reading the first one miscategorizes the whole mod as that vehicle's
    // type. If ONLY mission vehicles remain, return None so the keyword heuristic decides.
    let first = store_files.iter().find(|f| !is_mission_vehicle(f))?;
    let xml = read_member(mod_path, kind, first)?;
    parse_store_category(&xml)
}

/// A store item under `missionVehicles/` is a contract-mission prop, not the mod's own
/// shop presence — don't let it decide the mod's category.
fn is_mission_vehicle(store_file: &str) -> bool {
    store_file
        .replace('\\', "/")
        .to_lowercase()
        .contains("missionvehicles/")
}

fn read_member(mod_path: &Path, kind: &str, member: &str) -> Option<String> {
    let member = member.replace('\\', "/");
    match kind {
        "zip" => {
            let f = std::fs::File::open(mod_path).ok()?;
            let mut ar = zip::ZipArchive::new(f).ok()?;
            let mut entry = ar.by_name(&member).ok()?;
            let mut s = String::new();
            entry.read_to_string(&mut s).ok()?;
            Some(s)
        }
        _ => std::fs::read_to_string(mod_path.join(&member)).ok(),
    }
}

/// Extract the text of `<category>` whose parent is `<storeData>`.
fn parse_store_category(xml: &str) -> Option<String> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();

    // The category's text arrives split at any `&amp;` (#266): gather it and read it
    // when `</category>` closes.
    let mut run = crate::xmltext::TextRun::default();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Start(e)) => {
                run.take();
                stack.push(String::from_utf8_lossy(e.local_name().as_ref()).into_owned());
            }
            Ok(Event::Empty(_)) => {
                run.take();
            }
            Ok(Event::End(_)) => {
                let last = stack.last().map(String::as_str).unwrap_or("");
                let parent = stack.iter().rev().nth(1).map(String::as_str).unwrap_or("");
                let txt = run.take();
                if last == "category" && parent == "storeData" {
                    let v = txt.trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
                stack.pop();
            }
            Ok(Event::Text(t)) => run.push_text(&t),
            Ok(Event::GeneralRef(r)) => run.push_ref(&r),
            _ => {}
        }
        buf.clear();
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_category() {
        let xml = r#"<vehicle><storeData><name>X</name><category>tractorsM</category></storeData></vehicle>"#;
        assert_eq!(parse_store_category(xml).as_deref(), Some("tractorsM"));
    }

    /// #266: a category with an entity reference isn't cut at the `&`.
    #[test]
    fn reads_category_with_an_entity() {
        let xml = r#"<vehicle><storeData><category>a&amp;b</category></storeData></vehicle>"#;
        assert_eq!(parse_store_category(xml).as_deref(), Some("a&b"));
    }

    #[test]
    fn mission_vehicles_are_skipped() {
        assert!(is_mission_vehicle("missionVehicles/chaff_jaguar990TT.xml"));
        assert!(is_mission_vehicle("data\\missionVehicles\\x.xml")); // backslashes + nested
        assert!(!is_mission_vehicle("store/myTractor.xml"));
    }
}
