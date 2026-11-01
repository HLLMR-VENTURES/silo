//! Fill-type budget (#341). FS25 supports at most 255 fill types: `FillTypeManager.addFillType`
//! refuses any past `2^SEND_NUM_BITS - 1` (`FillTypeManager.lua:316`) and logs
//! "FillTypeManager.addFillType too many fill types. Only 255 fill types are supported. Ignoring
//! 'X'." The game keeps loading, so the mod whose fill types were dropped breaks quietly. In the
//! measured runs the DLCs registered before the mods, so it was the mods that lost (FS25 1.25,
//! 2026-10-31: the base game and Riverbend Springs 182, every DLC 230, and FS25_SoilFertilizer
//! lost 10 of its fill types).
//!
//! What we can count:
//! * the base list `data/maps/maps_fillTypes.xml`, plus the `UNKNOWN` type the game adds first:
//!   exact. Without a readable install there is no check at all: a guessed base list would make
//!   every name a mod re-declares look new;
//! * a mod map's own `<fillTypes filename>` (in `map.xml`, and in its modDesc): exact;
//! * the base crops' fill types (`data/foliage/<crop>/*.xml`): only maps that grow those crops
//!   load them, and which those are is in the encrypted archives, so they're an estimate;
//! * DLCs: encrypted, so their fill types come from [`DLC_FILL_TYPES`]; a DLC can be off in a
//!   save, so they're an estimate;
//! * mods: their modDesc `<fillTypes filename>`, exact; other `*fillTypes*.xml` lists they ship
//!   (RealisticLivestock, MoreRealistic, Corn Production Pack and others load theirs from a
//!   script) are an estimate. A list under `mod_support/<dir>/` counts only with the map `<dir>`
//!   names (Realistic Livestock ships one per map it supports, #385; see
//!   [`heighttypes::support_dir`](crate::heighttypes::support_dir)).
//!
//! A fill type that already exists is updated in place, never added again
//! (`FillTypeManager:loadFillTypes`), so each name counts once.

use crate::heighttypes::{
    absorb_scoped, dlc_names, foliage_names, for_map, installed_dlc, map_own_file, named_list,
    read_estimates, severity_rank, Loads, Scoped, MAX_LIST_NAMES,
};
use std::collections::{BTreeSet, HashSet};
use std::path::Path;
use std::sync::OnceLock;

/// The most fill types the game supports.
pub const CAP: usize = 255;

/// Slots left at or below which we add a note.
const NEARLY_FULL: usize = 5;

/// The type the game registers before any list (`FillType.UNKNOWN`).
const UNKNOWN: &str = "UNKNOWN";

/// What the base crops add beyond the base list: the fill types they declare in
/// `data/foliage/<crop>/*.xml`, for when those files can't be read. Riverbend Springs grows these
/// crops (178 to 182); Zielonka and Hutan Pantai don't (178), measured in-game on FS25 1.25.
const BASE_MAP_EXTRA: &[&str] = &["BANANA", "COFFEECHERRIES", "ONION", "ONION_DIRTY"];

/// Fill types each DLC adds, keyed by its `pdlc/<stem>.dlc` file name, read in-game from
/// `g_fillTypeManager` (FS25 1.25). They may drift with DLC updates. The other packs add none.
const DLC_FILL_TYPES: &[(&str, &[&str])] = &[
    (
        "beansAndAlpacasPack",
        &[
            "ALPACAARMCHAIR",
            "ALPACACHAIR",
            "ALPACAFLEECE",
            "ALPACAWOOL",
            "ALPACAPILLOW",
            "ALPACAPLUSH",
            "ALPACASOCKS",
            "BANANACANDY",
            "BANANACHIPS",
            "BANANAMILK",
            "COFFEEBAGS",
            "COFFEEPACKED",
            "COFFEECAPSULES",
            "COFFEEYOGURT",
            "COFFEEBEVERAGE",
            "TIRAMISU",
            "COPPERORE",
            "IRONORE",
            "PIGMENTS",
            "RESIN",
            "PAINTBUCKETS",
            "BANANABOX",
            "BANANAFRIEDBOX",
            "BANANACAKE",
            "BANANAJAM",
            "FUNGICIDE",
            "ALPACA_WHITE",
            "ALPACA_BROWN",
            "ALPACA_GREY",
            "ALPACA_FAWN",
        ],
    ),
    (
        "highlandsFishingPack",
        &[
            "COAL",
            "STONEBIG",
            "CASTLEROCK",
            "SHINGLEROOF",
            "FISHFOOD",
            "SALMONYOUNG",
            "SALMON",
            "TROUTYOUNG",
            "TROUT",
            "FRIEDONION",
            "ONIONBAG",
            "ONIONSALT",
            "ONIONSOUP",
        ],
    ),
    ("pumpsAndHosesPack", &["SEPARATED_MANURE", "UMBILICAL_HOSE"]),
    (
        "strawHarvestPack",
        &["MOLASSES", "HAY_PELLETS", "STRAW_PELLETS"],
    ),
];

/// Declared `<fillType name>` values, upper-cased as the game keys them (`string.upper` in
/// `FillTypeManager:loadFillTypes`) and de-duplicated. Only `<root><fillTypes><fillType>` counts,
/// the path the game iterates; a file that doesn't parse counts as empty. Pure.
pub fn fill_type_names(xml: &[u8]) -> Vec<String> {
    named_list(xml, b"fillTypes", b"fillType", b"name")
        .map(|(names, _)| names)
        .unwrap_or_default()
}

/// The map's own fill-type file, from its `map.xml`. See [`map_own_file`]. Pure.
pub fn map_fill_file(map_xml: &[u8]) -> Option<Option<String>> {
    map_own_file(map_xml, b"fillTypes")
}

/// Does this member's file name look like a fill-type list a script might load?
pub fn is_candidate(member: &str) -> bool {
    let base = member.rsplit('/').next().unwrap_or(member).to_lowercase();
    base.contains("filltypes") && base.ends_with(".xml")
}

/// What the game itself brings: the base list, the base crops' additions and the DLCs'.
#[derive(Debug, Clone, Default)]
pub struct GameData {
    /// `UNKNOWN` and the base list.
    pub base: Vec<String>,
    /// The base crops' fill types, loaded only on maps that grow them.
    pub map_extra: Vec<String>,
    /// The installed DLCs' fill types.
    pub dlc: Vec<String>,
}

fn owned(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

impl GameData {
    /// Read the base list and the crops' fill types from the install and look up the installed
    /// DLCs. `None` when the base list can't be read.
    pub fn load(install_dir: Option<&Path>) -> Option<Self> {
        let dir = install_dir?;
        let read = std::fs::read(dir.join("data/maps/maps_fillTypes.xml"))
            .ok()
            .map(|b| fill_type_names(&b))
            .filter(|names| !names.is_empty())?;
        let mut base = vec![UNKNOWN.to_string()];
        base.extend(read.into_iter().filter(|n| n != UNKNOWN));
        Some(GameData {
            base,
            map_extra: foliage_names(dir, fill_type_names)
                .filter(|f| !f.is_empty())
                .unwrap_or_else(|| owned(BASE_MAP_EXTRA)),
            dlc: dlc_names(DLC_FILL_TYPES, &installed_dlc(dir)),
        })
    }

    /// [`load`](Self::load) once per run, for the game Silo detects.
    pub fn cached() -> Option<&'static GameData> {
        static DATA: OnceLock<Option<GameData>> = OnceLock::new();
        DATA.get_or_init(|| {
            let dir = crate::gamelaunch::detect().map(|g| std::path::PathBuf::from(g.install_dir));
            GameData::load(dir.as_deref())
        })
        .as_ref()
    }

    /// The fill types known for certain to exist: the base list and the mods' declared lists.
    /// A height type needs its fill type to take a slot; one whose fill type is only possible (a
    /// DLC that may be off, a crop only some maps grow, a list a script may load) stays an
    /// estimate. Add a map's own fill types for that map's candidate.
    pub fn known_exact(&self, mods: &[ModFills]) -> HashSet<String> {
        let mut out: HashSet<String> = self.base.iter().cloned().collect();
        for m in mods {
            out.extend(m.exact.iter().cloned());
        }
        out
    }
}

/// One active mod's fill types.
#[derive(Debug, Clone, Default)]
pub struct ModFills {
    pub label: String,
    pub tech_name: String,
    /// From its modDesc `<fillTypes filename>`: the game loads these.
    pub exact: Vec<String>,
    /// From other `*fillTypes*.xml` lists it ships: a script may load them.
    pub estimate: Vec<String>,
    /// Lists under `mod_support/<dir>/`: estimates only where `<dir>` loads (#385).
    pub scoped: Scoped,
}

impl ModFills {
    fn all(&self) -> impl Iterator<Item = &str> {
        self.exact.iter().chain(&self.estimate).map(String::as_str)
    }

    /// Count the map-specific lists that load per `loads` (a map, or the active mods) as
    /// estimates from now on.
    pub fn absorb_scoped(&mut self, loads: Loads) {
        absorb_scoped(&self.exact, &mut self.estimate, &mut self.scoped, loads);
    }
}

/// Read a mod's fill types: the modDesc-declared file exactly, plus (for non-maps) any other
/// `*fillTypes*.xml` it ships as an estimate, map-specific under `mod_support/<dir>/`. `candidates` lists the members that pass
/// [`is_candidate`].
pub fn mod_fills(
    label: &str,
    tech_name: &str,
    md: &crate::moddesc::ModDesc,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    candidates: &dyn Fn() -> Vec<String>,
) -> Option<ModFills> {
    let declared = md.fill_types_file.as_deref();
    let exact = declared
        .and_then(read)
        .map(|b| fill_type_names(&b))
        .unwrap_or_default();
    let (estimate, scoped) = if !md.is_map && exact.len() < MAX_LIST_NAMES {
        read_estimates(&exact, declared, candidates(), read, fill_type_names)
    } else {
        Default::default()
    };
    (!exact.is_empty() || !estimate.is_empty() || !scoped.is_empty()).then(|| ModFills {
        label: label.into(),
        tech_name: tech_name.into(),
        exact,
        estimate,
        scoped,
    })
}

/// An active mod map's own fill types (its `map.xml` list and its modDesc list).
#[derive(Debug, Clone)]
pub struct MapFills {
    pub label: String,
    /// The map's [`map_keys`](crate::heighttypes::map_keys): which `mod_support/<dir>/` lists
    /// load with it.
    pub keys: Vec<String>,
    pub types: Vec<String>,
}

/// The budget for one map.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// The mod map's name, or `None` for the base-game maps.
    pub map: Option<String>,
    /// The base list, the map's own and the base crops'.
    pub base: usize,
    /// Fill types the installed DLCs add beyond that.
    pub dlc: usize,
    /// Whether the base crops' fill types are in `base`.
    pub crops_estimated: bool,
    /// Counted from exactly-known sources only.
    pub exact_total: usize,
    /// Everything, estimates included.
    pub total: usize,
    /// Mods adding new fill types, most first: (label, tech name, count, all of them declared).
    pub mods: Vec<(String, String, usize, bool)>,
}

/// Count one map's fill types. Mods' map-specific lists (`scoped`) don't count here: [`worst`]
/// adds the ones for each map. Pure.
pub fn evaluate(game: &GameData, map: Option<&MapFills>, mods: &[ModFills]) -> Budget {
    let mut exact: BTreeSet<&str> = game.base.iter().map(String::as_str).collect();
    if let Some(m) = map {
        exact.extend(m.types.iter().map(String::as_str));
    }
    // The base crops' fill types may load on any map, a mod map included.
    let mut all = exact.clone();
    all.extend(game.map_extra.iter().map(String::as_str));
    let crops_estimated = all.len() > exact.len();
    let base = all.len();
    all.extend(game.dlc.iter().map(String::as_str));
    let dlc = all.len() - base;
    let fixed = all.clone();

    let mut per_mod = Vec::new();
    for m in mods {
        let new = m.all().filter(|t| !fixed.contains(t)).count();
        all.extend(m.all());
        exact.extend(m.exact.iter().map(String::as_str));
        if new > 0 {
            per_mod.push((
                m.label.clone(),
                m.tech_name.clone(),
                new,
                m.estimate.is_empty(),
            ));
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
        crops_estimated,
        exact_total: exact.len(),
        total: all.len(),
        mods: per_mod,
    }
}

/// The worst budget across the maps the save might use: the base-game maps and each active mod
/// map. Mods' lists for a map count only with that map; the base-game maps match none. Most
/// serious first, then fullest. Pure.
pub fn worst(game: &GameData, maps: &[MapFills], mods: &[ModFills]) -> Budget {
    std::iter::once(evaluate(game, None, mods))
        .chain(
            maps.iter()
                .map(|m| evaluate(game, Some(m), &mods_on(Some(m), mods))),
        )
        .max_by_key(|b| (severity_rank(severity(b)), b.total))
        .expect("the base-game maps are always a candidate")
}

/// `mods` as they load on `map` (the base-game maps when `None`). Pure.
pub(crate) fn mods_on<'a>(
    map: Option<&MapFills>,
    mods: &'a [ModFills],
) -> std::borrow::Cow<'a, [ModFills]> {
    match map {
        None => std::borrow::Cow::Borrowed(mods),
        Some(m) => for_map(mods, &m.keys, ModFills::absorb_scoped, |f| &f.scoped),
    }
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
        None => "the base game uses".into(),
    };
    let mod_count = b.total - b.base - b.dlc;
    let mut s = format!(
        "The game supports {CAP} material types (crops, feed, fertilizers, products and the like). Here {on} {}, your installed DLCs {}, and your active mods {} more: {} in all.",
        b.base, b.dlc, mod_count, b.total
    );
    if b.total > CAP {
        let over = b.total - CAP;
        s.push_str(&format!(
            " If that's what loads, the game skips every one past {CAP}, about {over}, and what uses them may not work."
        ));
        if mod_count > 0 {
            s.push_str(" In testing the DLCs registered before mods, so it was mods' materials that went missing.");
        }
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
        .map(|(l, _, n, _)| format!("{l} ({n})"))
        .collect();
    if !top.is_empty() {
        s.push_str(&format!(
            " New types per mod (some mods share them): {}.",
            top.join(", ")
        ));
    }
    let mut why: Vec<&str> = Vec::new();
    if b.dlc > 0 {
        why.push("a DLC that's off in your save adds none");
    }
    if b.crops_estimated {
        why.push("a few belong to crops only some maps grow");
    }
    if b.mods.iter().any(|m| !m.3) {
        why.push("some mods' lists may not load");
    }
    if !why.is_empty() {
        s.push_str(&format!(" Some counts are estimates: {}.", why.join("; ")));
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
            exact: types,
            estimate: vec![],
            scoped: vec![],
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

    /// A list of many entries reads in linear time, keeping first-seen order; a list with more
    /// distinct names than [`MAX_LIST_NAMES`] counts as unreadable, like a broken file (#351).
    #[test]
    fn huge_lists_read_fast_and_past_the_cap_count_as_unreadable() {
        use crate::heighttypes::MAX_LIST_NAMES;
        let list = |names: &mut dyn Iterator<Item = String>| {
            let mut xml = String::from("<map><fillTypes>");
            for n in names {
                xml.push_str(&format!("<fillType name=\"{n}\"/>"));
            }
            xml.push_str("</fillTypes></map>");
            xml.into_bytes()
        };
        // 200k entries cycling through exactly the cap's worth of names: all kept, in order.
        let at_cap = list(&mut (0..200_000).map(|i| format!("t{}", i % MAX_LIST_NAMES)));
        let started = std::time::Instant::now();
        let names = fill_type_names(&at_cap);
        assert_eq!(names.len(), MAX_LIST_NAMES);
        assert_eq!(names[0], "T0");
        assert_eq!(
            names[MAX_LIST_NAMES - 1],
            format!("T{}", MAX_LIST_NAMES - 1)
        );
        assert!(started.elapsed() < std::time::Duration::from_secs(20));
        // One distinct name more and the list is unreadable.
        let over = list(&mut (0..=MAX_LIST_NAMES).map(|i| format!("t{i}")));
        assert!(fill_type_names(&over).is_empty());
        // A mod's estimate merges its lists in linear time too, without repeating exact names.
        let md = crate::moddesc::ModDesc {
            fill_types_file: Some("xml/fillTypes.xml".into()),
            ..Default::default()
        };
        let read = |m: &str| match m {
            "xml/fillTypes.xml" => Some(list(&mut (0..5_000).map(|i| format!("a{i}")))),
            _ => Some(list(&mut (0..MAX_LIST_NAMES).map(|i| format!("a{i}")))),
        };
        let candidates = || (0..20).map(|i| format!("x/{i}_fillTypes.xml")).collect();
        let f = mod_fills("M", "FS25_M", &md, &read, &candidates).unwrap();
        assert_eq!(f.exact.len(), 5_000);
        assert_eq!(f.estimate.len(), MAX_LIST_NAMES - 5_000);
        assert_eq!(f.estimate[0], "A5000");
        // Names split across many lists: the estimate stops growing at the cap, and at most
        // MAX_LIST_FILES lists are read.
        let reads = std::cell::Cell::new(0);
        let read = |m: &str| {
            reads.set(reads.get() + 1);
            let i: usize = m.split('/').nth(1)?.split('_').next()?.parse().ok()?;
            Some(list(&mut (0..1_000).map(|k| format!("n{i}_{k}"))))
        };
        let plain = crate::moddesc::ModDesc::default();
        let f = mod_fills("M", "FS25_M", &plain, &read, &candidates).unwrap();
        assert_eq!(f.estimate.len(), MAX_LIST_NAMES);
        assert_eq!(reads.get(), MAX_LIST_NAMES / 1_000);
        let read_empty = |_: &str| {
            reads.set(reads.get() + 1);
            Some(list(&mut std::iter::empty()))
        };
        let many = || (0..150).map(|i| format!("x/{i}_fillTypes.xml")).collect();
        reads.set(0);
        assert!(mod_fills("M", "FS25_M", &plain, &read_empty, &many).is_none());
        assert_eq!(reads.get(), crate::heighttypes::MAX_LIST_FILES);
    }

    /// Only `<root><fillTypes><fillType>` registers: a bale's list of fill types it accepts
    /// doesn't, and comments don't (AdditionalFilltypes keeps three entries commented out).
    #[test]
    fn names_only_where_the_game_reads_them() {
        let bale = br#"<bale><fillTypes><fillType name="GRASS_WINDROW"/></fillTypes></bale>"#;
        assert_eq!(fill_type_names(bale), ["GRASS_WINDROW"]);
        let nested =
            br#"<bale><mounting><fillTypes><fillType name="GRASS"/></fillTypes></mounting></bale>"#;
        assert!(fill_type_names(nested).is_empty());
        let commented = br#"<map><fillTypes><!-- <fillType name="QUICKLIME"/> --><fillType name="UREA"/></fillTypes></map>"#;
        assert_eq!(fill_type_names(commented), ["UREA"]);
        let broken = br#"<map><fillTypes><fillType name="UREA"/></nope></map>"#;
        assert!(fill_type_names(broken).is_empty());
    }

    #[test]
    fn candidates_are_fill_type_xml_by_name() {
        assert!(is_candidate("xml/fillTypes.xml"));
        assert!(is_candidate("data/BSU_fillTypes.XML"));
        assert!(!is_candidate("objects/bales/roundbale125/roundbale125.xml"));
        assert!(!is_candidate("data/defaultSettings.xml"));
    }

    #[test]
    fn mod_fills_counts_declared_and_shipped_lists() {
        let declared_xml = br#"<map><fillTypes><fillType name="A"/></fillTypes></map>"#.to_vec();
        let shipped =
            br#"<map><fillTypes><fillType name="A"/><fillType name="B"/></fillTypes></map>"#
                .to_vec();
        let read = |name: &str| match name {
            "fillTypes.xml" => Some(declared_xml.clone()),
            "xml/v2/fillTypes.xml" | "mod_support/FS25_Valley/v2/fillTypes.xml" => {
                Some(shipped.clone())
            }
            _ => None,
        };
        let candidates = || {
            vec![
                "fillTypes.xml".to_string(),
                "xml/v2/fillTypes.xml".to_string(),
                "mod_support/FS25_Valley/v2/fillTypes.xml".to_string(),
            ]
        };
        let md = crate::moddesc::ModDesc {
            fill_types_file: Some("fillTypes.xml".into()),
            ..Default::default()
        };
        let f = mod_fills("M", "FS25_M", &md, &read, &candidates).unwrap();
        assert_eq!(
            (f.exact, f.estimate),
            (vec!["A".to_string()], vec!["B".to_string()])
        );
        // A `mod_support/<map>/` list is set aside for that map (#385), without the exact names.
        assert_eq!(
            f.scoped,
            [("FS25_Valley".to_string(), vec!["B".to_string()])]
        );
        let script = mod_fills("M", "FS25_M", &Default::default(), &read, &candidates).unwrap();
        assert_eq!(script.estimate, ["A", "B"]);
        assert_eq!(
            script.scoped,
            [(
                "FS25_Valley".to_string(),
                vec!["A".to_string(), "B".to_string()]
            )]
        );
        let map = crate::moddesc::ModDesc {
            is_map: true,
            ..Default::default()
        };
        assert!(mod_fills("M", "FS25_M", &map, &read, &candidates).is_none());
    }

    #[test]
    fn map_fill_file_only_inside_the_map() {
        assert_eq!(
            map_fill_file(br#"<map><fillTypes filename="config\fillTypes.xml"/></map>"#),
            Some(Some("config/fillTypes.xml".into()))
        );
        assert_eq!(
            map_fill_file(br#"<map><fillTypes filename="$data/maps/maps_fillTypes.xml"/></map>"#),
            Some(None)
        );
        // A map.xml that doesn't parse: Silo can't tell.
        assert_eq!(
            map_fill_file(br#"<map><fillTypes filename="a.xml"/>"#),
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
        // Without the estimates (crops, DLCs) it's 213, under the cap.
        assert_eq!(b.exact_total, 213);
        assert_eq!(severity(&b), Some("warning"));
        let text = explain(&b);
        assert!(
            text.contains(
                "uses 182, your installed DLCs 48, and your active mods 35 more: 265 in all"
            ),
            "{text}"
        );
        assert!(text.contains("past 255, about 10"), "{text}");
        assert!(
            text.contains("it was mods' materials that went missing"),
            "{text}"
        );
        assert!(
            text.contains("SoilFertilizer (22), AdditionalFilltypes (13)"),
            "{text}"
        );
        assert!(
            text.contains("a DLC that's off in your save adds none"),
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
    fn script_lists_are_an_estimate() {
        let g = game(178, 0, 0);
        let mut f = m("Livestock", vec![]);
        f.estimate = many("L", 80);
        let b = evaluate(&g, None, &[f]);
        assert_eq!((b.exact_total, b.total), (178, 258));
        assert_eq!(severity(&b), Some("warning"));
        assert!(
            explain(&b).contains("some mods' lists may not load"),
            "{}",
            explain(&b)
        );
    }

    /// #385: Realistic Livestock ships `mod_support/<map>/fillTypes.xml` for maps it supports and
    /// loads one only with that map. They count on that map alone, never on the base-game maps.
    #[test]
    fn map_specific_lists_count_only_on_their_map() {
        let g = game(178, 0, 0);
        let mut livestock = m("Livestock", vec![]);
        livestock.estimate = many("RL", 10);
        livestock.scoped = vec![
            ("FS25_HofBergmann".into(), many("HOF", 70)),
            ("FS25_The_Mechet".into(), many("MECHET", 2)),
        ];
        let mods = [livestock];
        // The base-game maps match no folder: 178 + 10.
        let base = evaluate(&g, None, &mods);
        assert_eq!((base.total, severity(&base)), (188, None));
        assert_eq!(worst(&g, &[], &mods).total, 188);

        let map = |label: &str, tech: &str| MapFills {
            label: label.into(),
            keys: crate::heighttypes::map_keys(tech, label),
            types: vec![],
        };
        // Matched by tech name: 178 + 10 + 70 = 258, an estimate, so a warning.
        let hof = worst(&g, &[map("Hof Bergmann", "FS25_HofBergmann")], &mods);
        assert_eq!(hof.map.as_deref(), Some("Hof Bergmann"));
        assert_eq!((hof.exact_total, hof.total), (178, 258));
        assert_eq!(severity(&hof), Some("warning"));
        assert_eq!(
            hof.mods,
            [("Livestock".into(), "FS25_Livestock".into(), 80, false)]
        );
        // Matched by title when the tech name differs; only that map's own list counts.
        let mechet = worst(&g, &[map("The Mechet", "FS25_Mechet_v2")], &mods);
        assert_eq!(mechet.total, 190);
        // Another map loads none of them.
        let other = worst(&g, &[map("Big Valley", "FS25_BigValley")], &mods);
        assert_eq!(other.total, 188);
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
                ("A".into(), "FS25_A".into(), 1, true),
                ("B".into(), "FS25_B".into(), 1, true)
            ]
        );
    }

    #[test]
    fn a_mod_map_adds_its_own_and_may_grow_the_base_crops() {
        let g = game(178, 4, 0);
        let map = MapFills {
            label: "Big Valley".into(),
            keys: vec![],
            types: vec!["BASE0".into(), "VALLEY_ROCK".into()],
        };
        let b = evaluate(&g, Some(&map), &[]);
        assert_eq!((b.base, b.exact_total), (183, 179));
        let w = worst(&g, std::slice::from_ref(&map), &[]);
        assert_eq!(w.map.as_deref(), Some("Big Valley"));
        assert!(
            explain(&w).contains("the base game and Big Valley use 183"),
            "{}",
            explain(&w)
        );
        assert_eq!(worst(&g, &[], &[]).base, 182);
    }

    /// Only certain sources make a fill type known: not DLCs (may be off), crops (some maps) or
    /// lists a script may load.
    #[test]
    fn known_fill_types_are_the_certain_ones() {
        let g = game(2, 1, 1);
        let mut f = m("A", vec!["NEW".into()]);
        f.estimate = vec!["MAYBE".into()];
        let known = g.known_exact(&[f]);
        for n in ["BASE0", "BASE1", "NEW"] {
            assert!(known.contains(n), "{n}");
        }
        for n in ["MAPX0", "DLC0", "MAYBE"] {
            assert!(!known.contains(n), "{n}");
        }
    }

    /// The in-game counts (FS25 1.25): 178 base, 182 on Riverbend Springs, 230 with every DLC.
    #[test]
    fn the_measured_tables_add_up() {
        assert_eq!(BASE_MAP_EXTRA.len(), 4);
        let counts: Vec<usize> = DLC_FILL_TYPES.iter().map(|(_, t)| t.len()).collect();
        assert_eq!(counts, [30, 13, 2, 3]);
        let stems: Vec<String> = DLC_FILL_TYPES.iter().map(|(s, _)| s.to_string()).collect();
        let g = GameData {
            base: many("BASE", 178),
            map_extra: owned(BASE_MAP_EXTRA),
            dlc: dlc_names(DLC_FILL_TYPES, &stems),
        };
        let b = evaluate(&g, None, &[]);
        assert_eq!((b.base, b.dlc, b.total), (182, 48, 230));
    }

    #[test]
    fn loads_the_base_list_with_unknown_first() {
        let dir = std::env::temp_dir().join(format!("silo_ft_install_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("data/maps")).unwrap();
        std::fs::create_dir_all(dir.join("data/foliage/banana")).unwrap();
        std::fs::write(
            dir.join("data/foliage/banana/banana.xml"),
            r#"<foliageType><fillTypes><fillType name="BANANA"/><!-- <fillType name="OLD"/> --></fillTypes></foliageType>"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("data/maps/maps_fillTypes.xml"),
            r#"<map><fillTypes><fillType name="WHEAT"/><fillType name="unknown"/><fillType name="BARLEY"/></fillTypes></map>"#,
        )
        .unwrap();
        let g = GameData::load(Some(&dir)).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(g.base, ["UNKNOWN", "WHEAT", "BARLEY"]);
        // The crops' own fill types, with commented-out ones skipped.
        assert_eq!(g.map_extra, ["BANANA"]);
        // No install: no check, rather than a guessed base list.
        assert!(GameData::load(None).is_none());
        assert!(GameData::load(Some(Path::new("Z:/no/such/install"))).is_none());
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
