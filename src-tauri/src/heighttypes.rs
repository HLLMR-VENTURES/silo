//! Ground-pile slot budget (#339). FS25 gives each map a fixed number of density-map height
//! types, the slots that let a material lie on the ground as a pile: `2^numChannels - 1`, so 63
//! on the base maps (6 channels). The base game already fills 48 of them from
//! `data/maps/maps_densityMapHeightTypes.xml`, and 51 on maps that grow coffee cherries and onions
//! (their crop XML adds 3; Riverbend Springs does, Hutan Pantai doesn't). The DLCs and mods share the rest. Mods register before the DLCs, so when the slots run
//! out the DLCs lose: the log says `addDensityMapHeightType "X": maximum number (63) of height
//! types already registered`, then Straw Harvest and Pumps n' Hoses disable themselves and close
//! the save (measured in-game on FS25 1.25, 2026-10-31).
//!
//! What we can count:
//! * the map's list: the base game's, or the map's own when its `map.xml` points
//!   `<densityMapHeightTypes filename>` at a file inside the map (treated as replacing the base
//!   list, the way the base maps point at `$data/maps/maps_densityMapHeightTypes.xml`): exact;
//!   plus the base crops' types, which only some maps load: an estimate;
//! * mods that declare `<densityMapHeightTypes filename>` in modDesc: exact;
//! * mods that register the same XML from Lua (SoilFertilizer, AdvancedSilageTypes and others):
//!   an estimate, from any `*heightType*.xml` in the zip;
//! * DLCs: encrypted, so their types come from [`DLC_TYPES`], measured in-game, also an estimate.
//!
//! Each fill type takes one slot however many sources declare it: re-declaring `STRAW` in a mod
//! kept the count the same in-game (the mod's settings replace the base ones).

use crate::moddesc::ModDesc;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::OnceLock;

/// Channels the base maps use, and the default when a list doesn't say.
pub const DEFAULT_CHANNELS: u32 = 6;

/// The base game's list (FS25 1.25 `data/maps/maps_densityMapHeightTypes.xml`), used when the
/// install can't be read.
const BASE_TYPES: &[&str] = &[
    "WHEAT",
    "BARLEY",
    "OAT",
    "CANOLA",
    "SORGHUM",
    "GRAPE",
    "MAIZE",
    "POTATO",
    "SUGARBEET",
    "SUGARBEET_CUT",
    "BEETROOT",
    "CARROT",
    "PARSNIP",
    "COTTON",
    "RICELONGGRAIN",
    "RICE",
    "GREENBEAN",
    "PEA",
    "SPINACH",
    "SOYBEAN",
    "SUNFLOWER",
    "CHAFF",
    "POPLAR",
    "SEEDS",
    "LIME",
    "MINERAL_FEED",
    "FERTILIZER",
    "SUGARCANE",
    "STRAW",
    "WHEAT_CUT",
    "BARLEY_CUT",
    "OAT_CUT",
    "CANOLA_CUT",
    "SOYBEAN_CUT",
    "GRASS_WINDROW",
    "DRYGRASS_WINDROW",
    "SILAGE",
    "FORAGE",
    "PIGFOOD",
    "MANURE",
    "LIQUIDMANURE",
    "WOODCHIPS",
    "TARP",
    "SNOW",
    "ROADSALT",
    "STONE",
    "OLIVE",
    "DIGESTATE",
];

/// The base crops' own height types (`data/foliage/coffeeCherries/coffeeCherries.xml`,
/// `data/foliage/onion/onion.xml`), used when the install can't be read.
const BASE_FOLIAGE_TYPES: &[&str] = &["COFFEECHERRIES", "ONION", "ONION_DIRTY"];

/// Height types each DLC adds, keyed by its `pdlc/<stem>.dlc` file name. The archives are
/// encrypted, so these were read in-game from `g_densityMapHeightManager` (FS25 1.25, each pack
/// alone and all together: 51 base, 56 with every pack) and may drift with DLC updates. The other
/// packs add none.
const DLC_TYPES: &[(&str, &[&str])] = &[
    ("strawHarvestPack", &["HAY_PELLETS", "STRAW_PELLETS"]),
    ("pumpsAndHosesPack", &["SEPARATED_MANURE"]),
    ("beansAndAlpacasPack", &["IRONORE", "COPPERORE"]),
];

/// Slots left at or below which we add a note.
const NEARLY_FULL: usize = 3;

/// Height types from one XML list: the fill type names (upper-cased, as the game keys them,
/// de-duplicated) and the list's `numChannels`, when it sets one. Pure.
pub fn parse_list(xml: &[u8]) -> (Vec<String>, Option<u32>) {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let (mut names, mut channels) = (Vec::new(), None);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => match e.name().as_ref() {
                b"densityMapHeightType" => {
                    if let Some(n) = attr(&e, b"fillTypeName") {
                        let n = n.trim().to_uppercase();
                        if !n.is_empty() && !names.contains(&n) {
                            names.push(n);
                        }
                    }
                }
                b"densityMapHeightTypes" if channels.is_none() => {
                    channels = attr(&e, b"numChannels").and_then(|v| v.trim().parse().ok());
                }
                _ => {}
            },
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    (names, channels)
}

/// The map's own height-type file, from its `map.xml`: `Some` only when it points inside the
/// map. `$data/...` (the base list) and no reference both mean the base list. Pure.
pub fn map_list_file(map_xml: &[u8]) -> Option<String> {
    map_own_file(map_xml, b"densityMapHeightTypes")
}

/// The file a map's `map.xml` names in its first `<tag filename>`, when it points inside the map
/// rather than at the base game's `$data/...`. Pure.
pub fn map_own_file(map_xml: &[u8], tag: &[u8]) -> Option<String> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(map_xml);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == tag => {
                let f = attr(&e, b"filename")?.trim().replace('\\', "/");
                return (!f.is_empty() && !f.starts_with('$')).then_some(f);
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
        buf.clear();
    }
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok())
        .map(|v| v.into_owned())
}

/// The cap for a list with this many channels: `2^n - 1` (the height type is decoded as
/// `band(bits, 2^heightTypeNumChannels - 1)`, `WheelPhysics.lua:444`).
pub fn cap_for(channels: u32) -> usize {
    (1usize << channels.clamp(1, 16)) - 1
}

/// What the game itself brings: the base list, the base crops' types and the installed DLCs'.
#[derive(Debug, Clone, Default)]
pub struct GameData {
    pub base: Vec<String>,
    pub base_channels: u32,
    /// The base crops' types. Only maps that grow those crops load them, so they're an estimate.
    pub foliage: Vec<String>,
    pub dlc: Vec<String>,
}

fn owned(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// Every height type the base crops declare in `data/foliage/<crop>/*.xml`.
fn foliage_types(install_dir: &Path) -> Option<Vec<String>> {
    foliage_names(install_dir, |b| parse_list(b).0)
}

/// Every name `parse` finds in the base crops' `data/foliage/<crop>/*.xml`, de-duplicated. The
/// crops a map grows bring these with them (ground-pile types, and fill types for #341).
pub fn foliage_names(install_dir: &Path, parse: fn(&[u8]) -> Vec<String>) -> Option<Vec<String>> {
    let mut out: Vec<String> = Vec::new();
    for crop in std::fs::read_dir(install_dir.join("data/foliage"))
        .ok()?
        .flatten()
    {
        let Ok(files) = std::fs::read_dir(crop.path()) else {
            continue;
        };
        for f in files.flatten() {
            let p = f.path();
            if p.extension().and_then(|e| e.to_str()) != Some("xml") {
                continue;
            }
            for t in std::fs::read(&p).map(|b| parse(&b)).unwrap_or_default() {
                if !out.contains(&t) {
                    out.push(t);
                }
            }
        }
    }
    Some(out)
}

impl GameData {
    /// Read the base list and crop types from the install (falling back to [`BASE_TYPES`] and
    /// [`BASE_FOLIAGE_TYPES`]) and look up the installed DLCs in `pdlc/`.
    pub fn load(install_dir: Option<&Path>) -> Self {
        let read = install_dir
            .and_then(|d| std::fs::read(d.join("data/maps/maps_densityMapHeightTypes.xml")).ok())
            .map(|b| parse_list(&b))
            .filter(|(names, _)| !names.is_empty());
        let (base, channels) = read.unwrap_or_else(|| (owned(BASE_TYPES), None));
        let foliage = install_dir
            .and_then(foliage_types)
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| owned(BASE_FOLIAGE_TYPES));
        let stems = install_dir.map(installed_dlc).unwrap_or_default();
        GameData {
            base,
            base_channels: channels.unwrap_or(DEFAULT_CHANNELS),
            foliage,
            dlc: dlc_types(&stems),
        }
    }

    /// [`load`](Self::load) once per run, for the game Silo detects. The install and its DLCs
    /// rarely change while Silo is open.
    pub fn cached() -> &'static GameData {
        static DATA: OnceLock<GameData> = OnceLock::new();
        DATA.get_or_init(|| {
            let dir = crate::gamelaunch::detect().map(|g| std::path::PathBuf::from(g.install_dir));
            GameData::load(dir.as_deref())
        })
    }
}

/// The installed DLCs: the stems of the install's `pdlc/*.dlc` files.
pub fn installed_dlc(install_dir: &Path) -> Vec<String> {
    std::fs::read_dir(install_dir.join("pdlc"))
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            (p.extension()?.to_str()? == "dlc")
                .then(|| p.file_stem()?.to_str().map(String::from))?
        })
        .collect()
}

/// The height types the given DLCs add, per [`DLC_TYPES`]. Pure.
fn dlc_types(stems: &[String]) -> Vec<String> {
    dlc_names(DLC_TYPES, stems)
}

/// The names a per-DLC table gives for the installed DLCs, de-duplicated, in table order. Pure.
pub fn dlc_names(table: &[(&str, &[&str])], stems: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (stem, types) in table {
        if stems.iter().any(|s| s.eq_ignore_ascii_case(stem)) {
            for t in *types {
                if !out.iter().any(|o| o == t) {
                    out.push(t.to_string());
                }
            }
        }
    }
    out
}

/// One active mod's height types.
#[derive(Debug, Clone)]
pub struct ModTypes {
    pub label: String,
    pub tech_name: String,
    pub types: Vec<String>,
    /// Declared in modDesc (exact), not found in a script-loaded XML (estimate).
    pub exact: bool,
}

/// A map's list, when the map brings its own.
#[derive(Debug, Clone)]
pub struct MapTypes {
    pub label: String,
    /// The map's own list, or `None` for the base list.
    pub list: Option<(Vec<String>, u32)>,
    /// What the map's modDesc adds on top, like any mod.
    pub extra: Option<ModTypes>,
}

/// Read a mod's height types. A declared modDesc file wins; otherwise, for non-maps, any
/// `*heightType*.xml` in the zip counts as an estimate. `read` fetches a member's bytes.
pub fn mod_types(
    label: &str,
    tech_name: &str,
    md: &ModDesc,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    members: &dyn Fn() -> Vec<String>,
) -> Option<ModTypes> {
    let (types, exact) = if let Some(f) = &md.height_types_file {
        (parse_list(&read(f)?).0, true)
    } else if md.is_map {
        return None;
    } else {
        let mut all: Vec<String> = Vec::new();
        for m in members() {
            let base = m.rsplit('/').next().unwrap_or(&m).to_lowercase();
            if base.contains("heighttype") && base.ends_with(".xml") {
                for t in read(&m).map(|b| parse_list(&b).0).unwrap_or_default() {
                    if !all.contains(&t) {
                        all.push(t);
                    }
                }
            }
        }
        (all, false)
    };
    (!types.is_empty()).then(|| ModTypes {
        label: label.into(),
        tech_name: tech_name.into(),
        types,
        exact,
    })
}

/// The budget for one map.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// The mod map's name, or `None` for the base list.
    pub map: Option<String>,
    pub cap: usize,
    pub base: usize,
    /// Slots the DLCs add beyond the map's list.
    pub dlc: usize,
    /// Slots counted from the map list and exactly-declared mods only.
    pub exact_total: usize,
    /// Every slot, estimates included.
    pub total: usize,
    /// Mods adding new slots, most first: (label, tech name, slots, exact).
    pub mods: Vec<(String, String, usize, bool)>,
}

/// Count one map's slots. Pure.
pub fn evaluate(game: &GameData, map: Option<&MapTypes>, mods: &[ModTypes]) -> Budget {
    let (base, channels) = match map.and_then(|m| m.list.clone()) {
        Some((list, ch)) => (list, ch),
        None => (game.base.clone(), game.base_channels),
    };
    // The map's list is exact; the base crops' types load only on maps that grow them.
    let mut exact: BTreeSet<&str> = base.iter().map(String::as_str).collect();
    let base_set: BTreeSet<&str> = exact
        .iter()
        .copied()
        .chain(game.foliage.iter().map(String::as_str))
        .collect();
    let mut all: BTreeSet<&str> = base_set.clone();
    let dlc_before = all.len();
    all.extend(game.dlc.iter().map(String::as_str));
    let dlc = all.len() - dlc_before;
    let fixed: BTreeSet<&str> = all.clone();

    let mut active: Vec<&ModTypes> = mods.iter().collect();
    if let Some(extra) = map.and_then(|m| m.extra.as_ref()) {
        active.push(extra);
    }
    let mut per_mod = Vec::new();
    for m in &active {
        let new = m
            .types
            .iter()
            .filter(|t| !fixed.contains(t.as_str()))
            .count();
        all.extend(m.types.iter().map(String::as_str));
        if m.exact {
            exact.extend(m.types.iter().map(String::as_str));
        }
        if new > 0 {
            per_mod.push((m.label.clone(), m.tech_name.clone(), new, m.exact));
        }
    }
    per_mod.sort_by(|a, b| {
        b.2.cmp(&a.2)
            .then(a.0.to_lowercase().cmp(&b.0.to_lowercase()))
    });
    Budget {
        map: map.map(|m| m.label.clone()),
        cap: cap_for(channels),
        base: base_set.len(),
        dlc,
        exact_total: exact.len(),
        total: all.len(),
        mods: per_mod,
    }
}

/// The tightest budget across the candidate maps: each active mod map (the player is likely on
/// one of them), or the base list when none is active. Pure.
pub fn worst(game: &GameData, maps: &[MapTypes], mods: &[ModTypes]) -> Budget {
    if maps.is_empty() {
        return evaluate(game, None, mods);
    }
    maps.iter()
        .map(|m| evaluate(game, Some(m), mods))
        .max_by_key(|b| b.total as i64 - b.cap as i64)
        .expect("maps is non-empty")
}

/// How serious a budget is: critical when the exactly-counted slots alone run over, warning when
/// the estimates push it over, info when 3 or fewer are left, else nothing. Pure.
pub fn severity(b: &Budget) -> Option<&'static str> {
    if b.exact_total > b.cap {
        Some("critical")
    } else if b.total > b.cap {
        Some("warning")
    } else if b.cap - b.total <= NEARLY_FULL {
        Some("info")
    } else {
        None
    }
}

/// The plain-words explanation for a budget worth showing. Pure.
pub fn explain(b: &Budget) -> String {
    let on = match &b.map {
        Some(m) => format!("{m} has"),
        None => "Your map has".into(),
    };
    let mod_slots: usize = b.total - b.base - b.dlc;
    let top: Vec<String> = b
        .mods
        .iter()
        .take(5)
        .map(|(l, _, n, _)| format!("{l} ({n})"))
        .collect();
    let mut s = format!(
        "{on} room for {} ground-pile types, the materials you can tip onto the ground. The base game uses {}, your DLCs {}, and your active mods {} more: {} in all.",
        b.cap, b.base, b.dlc, mod_slots, b.total
    );
    if b.total > b.cap {
        s.push_str(" When they run out, Straw Harvest and Pumps n' Hoses refuse to load and the game closes your save.");
    } else {
        let left = b.cap - b.total;
        s.push_str(&format!(
            " Only {left} {} left, so one more mod that adds piles could stop Straw Harvest and Pumps n' Hoses from loading.",
            if left == 1 { "is" } else { "are" }
        ));
    }
    if !top.is_empty() {
        s.push_str(&format!(" Most slots: {}.", top.join(", ")));
    }
    let estimated = b.mods.iter().filter(|m| !m.3).count();
    if b.dlc > 0 || estimated > 0 {
        s.push_str(" Some counts are estimates: DLC counts can change with updates");
        if estimated > 0 {
            s.push_str(", and mods that add piles from a script may not use all of theirs");
        }
        s.push('.');
    }
    if b.total > b.cap {
        s.push_str(&format!(
            " Turn off mods until you're at {} or fewer.",
            b.cap
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(base: usize, dlc: &[&str]) -> GameData {
        GameData {
            base: (0..base).map(|i| format!("BASE{i}")).collect(),
            base_channels: 6,
            foliage: vec![],
            dlc: dlc.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn m(label: &str, types: &[&str], exact: bool) -> ModTypes {
        ModTypes {
            label: label.into(),
            tech_name: format!("FS25_{label}"),
            types: types.iter().map(|s| s.to_string()).collect(),
            exact,
        }
    }

    fn many(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    fn mm(label: &str, types: Vec<String>, exact: bool) -> ModTypes {
        ModTypes {
            label: label.into(),
            tech_name: format!("FS25_{label}"),
            types,
            exact,
        }
    }

    #[test]
    fn parses_a_list_and_its_channels() {
        let xml = br#"<map><densityMapHeightTypes firstChannel="0" numChannels="7">
            <densityMapHeightType fillTypeName="urea" maxSurfaceAngle="20"><collision scale="1"/></densityMapHeightType>
            <densityMapHeightType fillTypeName="UREA"/>
            <densityMapHeightType fillTypeName="GYPSUM"/>
        </densityMapHeightTypes></map>"#;
        let (names, ch) = parse_list(xml);
        assert_eq!(names, vec!["UREA", "GYPSUM"]);
        assert_eq!(ch, Some(7));
        assert_eq!(
            parse_list(b"<map><densityMapHeightTypes/></map>"),
            (vec![], None)
        );
        assert_eq!(parse_list(b"not xml").0.len(), 0);
    }

    #[test]
    fn map_list_file_only_when_it_points_inside_the_map() {
        assert_eq!(
            map_list_file(br#"<map><densityMapHeightTypes filename="map\xml\types.xml"/></map>"#),
            Some("map/xml/types.xml".into())
        );
        assert_eq!(
            map_list_file(
                br#"<map><densityMapHeightTypes filename="$data/maps/maps_densityMapHeightTypes.xml"/></map>"#
            ),
            None
        );
        assert_eq!(map_list_file(b"<map><fillTypes/></map>"), None);
        // A commented-out reference is no reference (FS25_BackRoadsCounty4x does this).
        assert_eq!(
            map_list_file(br#"<map><!-- <densityMapHeightTypes filename="x.xml" /> --></map>"#),
            None
        );
    }

    #[test]
    fn cap_follows_the_channels() {
        assert_eq!(cap_for(6), 63);
        assert_eq!(cap_for(7), 127);
    }

    #[test]
    fn plenty_of_room_says_nothing() {
        let b = evaluate(&game(48, &[]), None, &[m("Silage", &["A", "B"], true)]);
        assert_eq!((b.total, b.cap), (50, 63));
        assert_eq!(severity(&b), None);
    }

    #[test]
    fn one_fill_type_takes_one_slot_however_many_declare_it() {
        let b = evaluate(
            &game(48, &["PELLETS"]),
            None,
            &[
                m("A", &["X", "Y", "BASE0", "PELLETS"], true),
                m("B", &["X", "Z"], true),
            ],
        );
        // 48 base + PELLETS + X, Y, Z.
        assert_eq!(b.total, 52);
        assert_eq!(b.dlc, 1);
        // Per-mod counts only count slots beyond the base list and the DLCs.
        assert_eq!(b.mods[0], ("A".into(), "FS25_A".into(), 2, true));
        assert_eq!(b.mods[1], ("B".into(), "FS25_B".into(), 2, true));
    }

    #[test]
    fn exact_overflow_is_critical() {
        let b = evaluate(&game(48, &[]), None, &[mm("Fills", many("F", 22), true)]);
        assert_eq!(b.exact_total, 70);
        assert_eq!(severity(&b), Some("critical"));
        let text = explain(&b);
        assert!(text.contains("room for 63 ground-pile types"), "{text}");
        assert!(text.contains("70 in all"), "{text}");
        assert!(text.contains("Fills (22)"), "{text}");
        assert!(
            text.contains("Turn off mods until you're at 63 or fewer"),
            "{text}"
        );
        assert!(!text.contains("estimates"), "{text}");
    }

    #[test]
    fn estimates_pushing_it_over_are_a_warning() {
        let b = evaluate(
            &game(48, &["D1", "D2", "D3"]),
            None,
            &[
                mm("Exact", many("E", 10), true),
                mm("Script", many("S", 6), false),
            ],
        );
        // Exact: 48 + 10 = 58, fine. With DLC 3 and the script mod's 6: 67.
        assert_eq!((b.exact_total, b.total), (58, 67));
        assert_eq!(severity(&b), Some("warning"));
        let text = explain(&b);
        assert!(
            text.contains("your DLCs 3, and your active mods 16 more: 67 in all"),
            "{text}"
        );
        assert!(text.contains("from a script"), "{text}");
    }

    #[test]
    fn nearly_full_is_a_note() {
        let b = evaluate(&game(48, &[]), None, &[mm("Big", many("B", 13), true)]);
        assert_eq!(b.total, 61);
        assert_eq!(severity(&b), Some("info"));
        assert!(explain(&b).contains("Only 2 are left"), "{}", explain(&b));
        let one = evaluate(&game(48, &[]), None, &[mm("Big", many("B", 14), true)]);
        assert!(
            explain(&one).contains("Only 1 is left"),
            "{}",
            explain(&one)
        );
        let four = evaluate(&game(48, &[]), None, &[mm("Big", many("B", 11), true)]);
        assert_eq!(severity(&four), None);
    }

    #[test]
    fn a_map_with_its_own_list_replaces_the_base_and_sets_the_cap() {
        let map = MapTypes {
            label: "Big Valley".into(),
            list: Some((many("M", 40), 7)),
            extra: Some(m("Big Valley", &["EXTRA"], true)),
        };
        let b = evaluate(
            &game(48, &[]),
            Some(&map),
            &[mm("Fills", many("F", 30), true)],
        );
        assert_eq!((b.base, b.cap, b.total), (40, 127, 71));
        assert_eq!(severity(&b), None);
        assert_eq!(b.map.as_deref(), Some("Big Valley"));
    }

    #[test]
    fn worst_map_wins_and_no_maps_means_the_base_list() {
        let roomy = MapTypes {
            label: "Roomy".into(),
            list: Some((many("M", 10), 6)),
            extra: None,
        };
        let stock = MapTypes {
            label: "Stock-list map".into(),
            list: None,
            extra: None,
        };
        let mods = [mm("Fills", many("F", 20), true)];
        let b = worst(&game(48, &[]), &[roomy, stock], &mods);
        assert_eq!(b.map.as_deref(), Some("Stock-list map"));
        assert_eq!(b.total, 68);
        let base = worst(&game(48, &[]), &[], &mods);
        assert_eq!(base.map, None);
        assert!(explain(&base).starts_with("Your map has room"));
    }

    #[test]
    fn mod_types_prefers_the_declared_file() {
        let list = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="A"/></densityMapHeightTypes></map>"#.to_vec();
        let other = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="B"/></densityMapHeightTypes></map>"#.to_vec();
        let read = |name: &str| match name {
            "xml/declared.xml" => Some(list.clone()),
            "xml/densityMapHeightTypes.xml" => Some(other.clone()),
            _ => None,
        };
        let members = || {
            vec![
                "modDesc.xml".to_string(),
                "xml/densityMapHeightTypes.xml".to_string(),
            ]
        };

        let declared = ModDesc {
            height_types_file: Some("xml/declared.xml".into()),
            ..Default::default()
        };
        let t = mod_types("M", "FS25_M", &declared, &read, &members).unwrap();
        assert_eq!((t.types, t.exact), (vec!["A".to_string()], true));

        // No declaration: the script-loaded file counts, as an estimate.
        let t = mod_types("M", "FS25_M", &ModDesc::default(), &read, &members).unwrap();
        assert_eq!((t.types, t.exact), (vec!["B".to_string()], false));

        // A map's own list is handled as the map's, never as a mod estimate.
        let map = ModDesc {
            is_map: true,
            ..Default::default()
        };
        assert!(mod_types("M", "FS25_M", &map, &read, &members).is_none());
        // Nothing found: no entry.
        assert!(mod_types("M", "FS25_M", &ModDesc::default(), &read, &Vec::new).is_none());
    }

    #[test]
    fn dlc_lookup_ignores_case_and_unknown_packs() {
        assert!(dlc_types(&["someUnknownPack".into()]).is_empty());
        // Measured in-game: Straw Harvest 2, Pumps n' Hoses 1, Beans & Alpacas 2, others none.
        let all = dlc_types(&[
            "STRAWHARVESTPACK".into(),
            "pumpsAndHosesPack".into(),
            "beansAndAlpacasPack".into(),
            "vredoPack".into(),
        ]);
        assert_eq!(
            all,
            [
                "HAY_PELLETS",
                "STRAW_PELLETS",
                "SEPARATED_MANURE",
                "IRONORE",
                "COPPERORE"
            ]
        );
    }

    #[test]
    fn base_crop_types_count_as_an_estimate() {
        let mut g = game(48, &[]);
        g.foliage = vec!["ONION".into(), "COFFEECHERRIES".into(), "BASE0".into()];
        let b = evaluate(&g, None, &[]);
        assert_eq!((b.base, b.total), (50, 50));
        // Only some maps grow those crops, so they never count toward a Problem.
        assert_eq!(b.exact_total, 48);
        // A map with its own list still loads the base crops.
        let map = MapTypes {
            label: "Own list".into(),
            list: Some((many("M", 10), 6)),
            extra: None,
        };
        assert_eq!(evaluate(&g, Some(&map), &[]).total, 13);
    }

    #[test]
    fn loads_the_lists_from_an_install() {
        let dir = std::env::temp_dir().join(format!("silo_ht_install_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/maps")).unwrap();
        std::fs::create_dir_all(dir.join("data/foliage/onion")).unwrap();
        std::fs::create_dir_all(dir.join("pdlc")).unwrap();
        std::fs::write(
            dir.join("data/maps/maps_densityMapHeightTypes.xml"),
            r#"<map><densityMapHeightTypes numChannels="6"><densityMapHeightType fillTypeName="WHEAT"/><densityMapHeightType fillTypeName="STRAW"/></densityMapHeightTypes></map>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("data/foliage/onion/onion.xml"),
            r#"<foliageType><densityMapHeightTypes><densityMapHeightType fillTypeName="ONION"/></densityMapHeightTypes></foliageType>"#,
        )
        .unwrap();
        std::fs::write(dir.join("pdlc/strawHarvestPack.dlc"), b"x").unwrap();
        std::fs::write(dir.join("pdlc/readme.txt"), b"x").unwrap();
        let g = GameData::load(Some(&dir));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(g.base, ["WHEAT", "STRAW"]);
        assert_eq!(g.foliage, ["ONION"]);
        assert_eq!(g.dlc, ["HAY_PELLETS", "STRAW_PELLETS"]);
        assert_eq!(g.base_channels, 6);

        // No install: the built-in 1.25 lists, 51 slots, and no DLCs.
        let none = GameData::load(None);
        assert_eq!(none.base.len() + none.foliage.len(), 51);
        assert!(none.dlc.is_empty());
    }

    #[test]
    fn explanations_are_plain_words() {
        let b = evaluate(
            &game(48, &["D"]),
            None,
            &[
                mm("Fills", many("F", 22), true),
                mm("Soil", many("S", 3), false),
            ],
        );
        let text = explain(&b);
        for jargon in ["height type", "densityMap", "fillType", "channel", "—"] {
            assert!(!text.contains(jargon), "{jargon} in {text}");
        }
    }
}
