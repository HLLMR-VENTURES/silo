//! Fill-type budget (#341). FS25 supports at most 255 fill types: `FillTypeManager.addFillType`
//! refuses any past `2^SEND_NUM_BITS - 1` (`FillTypeManager.lua:316`) and logs
//! "FillTypeManager.addFillType too many fill types. Only 255 fill types are supported. Ignoring
//! 'X'." The game keeps loading, so the mod whose fill types were dropped breaks quietly. The
//! base game and its DLCs load first, so it's the mods that lose (measured in-game on FS25 1.25,
//! 2026-10-31: the base game and Riverbend Springs 182, every DLC 230, and FS25_SoilFertilizer
//! lost 10 of its fill types).
//!
//! What we can count:
//! * the base list `data/maps/maps_fillTypes.xml`, plus the `UNKNOWN` type the game adds first:
//!   exact;
//! * the map: a mod map's own `<fillTypes filename>` (in `map.xml`, and in its modDesc), exact;
//!   the base maps' own additions are encrypted, so [`BASE_MAP_EXTRA`] holds them: an estimate;
//! * DLCs: encrypted, so their fill types come from [`DLC_FILL_TYPES`]: an estimate;
//! * mods: their modDesc `<fillTypes filename>`, exact.
//!
//! A fill type that already exists is updated in place, never added again
//! (`FillTypeManager:loadFillTypes`), so each name counts once.

use crate::heighttypes::{dlc_names, installed_dlc, map_own_file};
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

/// The most fill types the game supports.
pub const CAP: usize = 255;

/// Slots left at or below which we add a note.
const NEARLY_FULL: usize = 5;

/// The type the game registers before any list (`FillType.UNKNOWN`).
const UNKNOWN: &str = "UNKNOWN";

/// How many fill types `data/maps/maps_fillTypes.xml` declares in FS25 1.25, for when the
/// install can't be read.
const BASE_FALLBACK_COUNT: usize = 177;

/// What the base maps add beyond the base list. Their `map.xml` is inside the encrypted game
/// archives, so these were read in-game (Riverbend Springs, FS25 1.25).
const BASE_MAP_EXTRA: &[&str] = &[];

/// Fill types each DLC adds, keyed by its `pdlc/<stem>.dlc` file name, read in-game from
/// `g_fillTypeManager` (FS25 1.25). They may drift with DLC updates. The other packs add none.
const DLC_FILL_TYPES: &[(&str, &[&str])] = &[];

/// Declared `<fillType name>` values, upper-cased as the game keys them (`string.upper` in
/// `FillTypeManager:loadFillTypes`) and de-duplicated. Pure.
pub fn fill_type_names(xml: &[u8]) -> Vec<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut out = Vec::new();
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == b"fillType" => {
                if let Some(a) = e.attributes().flatten().find(|a| a.key.as_ref() == b"name") {
                    if let Ok(v) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0) {
                        let n = v.trim().to_uppercase();
                        if !n.is_empty() && !out.contains(&n) {
                            out.push(n);
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    out
}

/// The map's own fill-type file, from its `map.xml`. Pure.
pub fn map_fill_file(map_xml: &[u8]) -> Option<String> {
    map_own_file(map_xml, b"fillTypes")
}

/// What the game itself brings: the base list, the base maps' additions and the DLCs'.
#[derive(Debug, Clone, Default)]
pub struct GameData {
    /// `UNKNOWN` and the base list.
    pub base: Vec<String>,
    /// What a base map adds; replaced by a mod map's own list.
    pub map_extra: Vec<String>,
    pub dlc: Vec<String>,
}

fn owned(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl GameData {
    /// Read the base list from the install (falling back to [`BASE_FALLBACK_COUNT`] placeholder
    /// names) and look up the installed DLCs.
    pub fn load(install_dir: Option<&Path>) -> Self {
        let read = install_dir
            .and_then(|d| std::fs::read(d.join("data/maps/maps_fillTypes.xml")).ok())
            .map(|b| fill_type_names(&b))
            .filter(|names| !names.is_empty())
            .unwrap_or_else(|| {
                (0..BASE_FALLBACK_COUNT)
                    .map(|i| format!("#BASE{i}"))
                    .collect()
            });
        let mut base = vec![UNKNOWN.to_string()];
        base.extend(read.into_iter().filter(|n| n != UNKNOWN));
        let stems = install_dir.map(installed_dlc).unwrap_or_default();
        GameData {
            base,
            map_extra: owned(BASE_MAP_EXTRA),
            dlc: dlc_names(DLC_FILL_TYPES, &stems),
        }
    }

    /// [`load`](Self::load) once per run, for the game Silo detects.
    pub fn cached() -> &'static GameData {
        static DATA: OnceLock<GameData> = OnceLock::new();
        DATA.get_or_init(|| {
            let dir = crate::gamelaunch::detect().map(|g| std::path::PathBuf::from(g.install_dir));
            GameData::load(dir.as_deref())
        })
    }
}

/// One active mod's fill types.
#[derive(Debug, Clone)]
pub struct ModFills {
    pub label: String,
    pub tech_name: String,
    pub types: Vec<String>,
}

/// An active mod map's own fill types (its `map.xml` list and its modDesc list).
#[derive(Debug, Clone)]
pub struct MapFills {
    pub label: String,
    pub types: Vec<String>,
}

/// The budget for one map.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// The mod map's name, or `None` for a base map.
    pub map: Option<String>,
    /// The base list plus the map's own.
    pub base: usize,
    /// Fill types the DLCs add beyond that.
    pub dlc: usize,
    /// Counted from exactly-known sources only.
    pub exact_total: usize,
    /// Everything, estimates included.
    pub total: usize,
    /// Mods adding new fill types, most first: (label, tech name, count).
    pub mods: Vec<(String, String, usize)>,
}

/// Count one map's fill types. Pure.
pub fn evaluate(game: &GameData, map: Option<&MapFills>, mods: &[ModFills]) -> Budget {
    let mut exact: BTreeSet<&str> = game.base.iter().map(String::as_str).collect();
    let mut all = exact.clone();
    match map {
        Some(m) => {
            exact.extend(m.types.iter().map(String::as_str));
            all.extend(m.types.iter().map(String::as_str));
        }
        None => all.extend(game.map_extra.iter().map(String::as_str)),
    }
    let base = all.len();
    all.extend(game.dlc.iter().map(String::as_str));
    let dlc = all.len() - base;
    let fixed = all.clone();

    let mut per_mod = Vec::new();
    for m in mods {
        let new = m
            .types
            .iter()
            .filter(|t| !fixed.contains(t.as_str()))
            .count();
        all.extend(m.types.iter().map(String::as_str));
        exact.extend(m.types.iter().map(String::as_str));
        if new > 0 {
            per_mod.push((m.label.clone(), m.tech_name.clone(), new));
        }
    }
    per_mod.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    Budget {
        map: map.map(|m| m.label.clone()),
        base,
        dlc,
        exact_total: exact.len(),
        total: all.len(),
        mods: per_mod,
    }
}

/// The fullest budget across the candidate maps: each active mod map, or a base map when none
/// is active. Pure.
pub fn worst(game: &GameData, maps: &[MapFills], mods: &[ModFills]) -> Budget {
    if maps.is_empty() {
        return evaluate(game, None, mods);
    }
    maps.iter()
        .map(|m| evaluate(game, Some(m), mods))
        .max_by_key(|b| b.total)
        .expect("maps is non-empty")
}

/// Critical when the exactly-counted fill types alone run over, warning when the estimates push
/// it over, info at 5 or fewer left, else nothing. Pure.
pub fn severity(b: &Budget) -> Option<&'static str> {
    if b.exact_total > CAP {
        Some("critical")
    } else if b.total > CAP {
        Some("warning")
    } else if CAP - b.total <= NEARLY_FULL {
        Some("info")
    } else {
        None
    }
}

/// The plain-words explanation for a budget worth showing. Pure.
pub fn explain(b: &Budget) -> String {
    let on = match &b.map {
        Some(m) => format!("the base game and {m} use"),
        None => "the base game and your map use".into(),
    };
    let mod_count = b.total - b.base - b.dlc;
    let mut s = format!(
        "The game supports {CAP} material types (crops, feed, fertilizers, products and the like). Here {on} {}, your DLCs {}, and your active mods {} more: {} in all.",
        b.base, b.dlc, mod_count, b.total
    );
    if b.total > CAP {
        s.push_str(&format!(
            " The game skips every one past {CAP}. Mods load last, so they lose theirs: about {} of their materials won't exist, and the parts that use them won't work.",
            b.total - CAP
        ));
    } else {
        let left = CAP - b.total;
        s.push_str(&format!(
            " Only {left} {} left before the game starts skipping mods' materials.",
            if left == 1 { "is" } else { "are" }
        ));
    }
    let top: Vec<String> = b
        .mods
        .iter()
        .take(5)
        .map(|(l, _, n)| format!("{l} ({n})"))
        .collect();
    if !top.is_empty() {
        s.push_str(&format!(" Most added: {}.", top.join(", ")));
    }
    if b.dlc > 0 || (b.map.is_none() && b.total > b.exact_total) {
        s.push_str(" DLC and base map counts are measured and can change with updates.");
    }
    if b.total > CAP {
        s.push_str(&format!(" Turn off mods until you're at {CAP} or fewer."));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn many(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    fn game(base: usize, extra: usize, dlc: usize) -> GameData {
        GameData {
            base: many("BASE", base),
            map_extra: many("MAPX", extra),
            dlc: many("DLC", dlc),
        }
    }

    fn m(label: &str, types: Vec<String>) -> ModFills {
        ModFills {
            label: label.into(),
            tech_name: format!("FS25_{label}"),
            types,
        }
    }

    #[test]
    fn names_upper_and_dedup() {
        let xml = br#"<map><fillTypes>
            <fillType name="Limestone" title="Limestone"/>
            <fillType name="COMPOST"/>
            <fillType name="limestone"/>
        </fillTypes></map>"#;
        assert_eq!(fill_type_names(xml), ["LIMESTONE", "COMPOST"]);
        assert!(fill_type_names(b"<map></map>").is_empty());
        assert!(fill_type_names(b"not xml").is_empty());
    }

    #[test]
    fn map_fill_file_only_inside_the_map() {
        assert_eq!(
            map_fill_file(br#"<map><fillTypes filename="config\fillTypes.xml"/></map>"#),
            Some("config/fillTypes.xml".into())
        );
        assert_eq!(
            map_fill_file(br#"<map><fillTypes filename="$data/maps/maps_fillTypes.xml"/></map>"#),
            None
        );
    }

    /// The in-game overflow run (FS25 1.25): 182 base and map, 48 DLC, then 13 and 22 from two
    /// mods. 265 asked for, 10 skipped.
    #[test]
    fn the_measured_overflow_is_a_warning() {
        let g = game(178, 4, 48);
        let b = evaluate(
            &g,
            None,
            &[
                m("AdditionalFilltypes", many("A", 13)),
                m("SoilFertilizer", many("S", 22)),
            ],
        );
        assert_eq!((b.base, b.dlc, b.total), (182, 48, 265));
        // Without the estimates (base map extras, DLCs) it's 213, under the cap.
        assert_eq!(b.exact_total, 213);
        assert_eq!(severity(&b), Some("warning"));
        let text = explain(&b);
        assert!(
            text.contains("use 182, your DLCs 48, and your active mods 35 more: 265 in all"),
            "{text}"
        );
        assert!(text.contains("about 10 of their materials"), "{text}");
        assert!(
            text.contains("SoilFertilizer (22), AdditionalFilltypes (13)"),
            "{text}"
        );
        assert!(
            text.contains("Turn off mods until you're at 255 or fewer"),
            "{text}"
        );
    }

    #[test]
    fn exact_overflow_is_critical_and_room_says_nothing() {
        let g = game(178, 0, 0);
        assert_eq!(
            severity(&evaluate(&g, None, &[m("Huge", many("H", 80))])),
            Some("critical")
        );
        assert_eq!(
            severity(&evaluate(&g, None, &[m("Small", many("S", 20))])),
            None
        );
    }

    #[test]
    fn nearly_full_is_a_note() {
        let g = game(178, 4, 48);
        let b = evaluate(&g, None, &[m("Fills", many("F", 21))]);
        assert_eq!(b.total, 251);
        assert_eq!(severity(&b), Some("info"));
        assert!(explain(&b).contains("Only 4 are left"), "{}", explain(&b));
        assert_eq!(
            severity(&evaluate(&g, None, &[m("Fills", many("F", 19))])),
            None
        );
    }

    #[test]
    fn existing_names_count_once() {
        let mut g = game(178, 0, 0);
        g.dlc = vec!["IRONORE".into()];
        let b = evaluate(
            &g,
            None,
            &[
                m("A", vec!["IRONORE".into(), "BASE0".into(), "NEW".into()]),
                m("B", vec!["NEW".into()]),
            ],
        );
        assert_eq!(b.total, 180);
        // Per-mod counts are fill types beyond the base game and DLCs.
        assert_eq!(
            b.mods,
            [
                ("A".into(), "FS25_A".into(), 1),
                ("B".into(), "FS25_B".into(), 1)
            ]
        );
    }

    #[test]
    fn a_mod_map_replaces_the_base_map_extras() {
        let g = game(178, 4, 0);
        let map = MapFills {
            label: "Big Valley".into(),
            types: vec!["BASE0".into(), "VALLEY_ROCK".into()],
        };
        let b = evaluate(&g, Some(&map), &[]);
        assert_eq!((b.base, b.exact_total), (179, 179));
        let w = worst(&g, std::slice::from_ref(&map), &[]);
        assert_eq!(w.map.as_deref(), Some("Big Valley"));
        assert!(
            explain(&w).contains("the base game and Big Valley use 179"),
            "{}",
            explain(&w)
        );
        assert_eq!(worst(&g, &[], &[]).base, 182);
    }

    #[test]
    fn loads_the_base_list_with_unknown_first() {
        let dir = std::env::temp_dir().join(format!("silo_ft_install_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/maps")).unwrap();
        std::fs::write(
            dir.join("data/maps/maps_fillTypes.xml"),
            r#"<map><fillTypes><fillType name="WHEAT"/><fillType name="unknown"/><fillType name="BARLEY"/></fillTypes></map>"#,
        )
        .unwrap();
        let g = GameData::load(Some(&dir));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(g.base, ["UNKNOWN", "WHEAT", "BARLEY"]);
        // No install: UNKNOWN plus the 1.25 count.
        assert_eq!(GameData::load(None).base.len(), 178);
    }

    #[test]
    fn explanations_are_plain_words() {
        let b = evaluate(&game(178, 4, 48), None, &[m("Fills", many("F", 40))]);
        let text = explain(&b);
        for jargon in ["fill type", "fillType", "FillType", "—"] {
            assert!(!text.contains(jargon), "{jargon} in {text}");
        }
    }
}
