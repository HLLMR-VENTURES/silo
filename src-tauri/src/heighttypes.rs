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
use std::collections::{BTreeSet, HashSet};
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
/// de-duplicated) and the list's `numChannels`, when it sets one. Only entries where the game
/// reads them count (`<root><densityMapHeightTypes><densityMapHeightType>`), and a file that
/// doesn't parse counts as empty, since the game can't load it either. Pure.
pub fn parse_list(xml: &[u8]) -> (Vec<String>, Option<u32>) {
    parse_list_checked(xml).unwrap_or_default()
}

/// [`parse_list`], but `None` when the file doesn't parse, so a caller can tell "names nothing"
/// from "can't be read". Pure.
pub fn parse_list_checked(xml: &[u8]) -> Option<(Vec<String>, Option<u32>)> {
    named_list(
        xml,
        b"densityMapHeightTypes",
        b"densityMapHeightType",
        b"fillTypeName",
    )
    .map(|(names, channels)| (names, channels.and_then(|v| v.trim().parse().ok())))
}

/// The most distinct names Silo reads from one list. The game's own limits are 255 fill types
/// and 63 height types, and the biggest real lists hold a few hundred, so a list past this is
/// junk (a 32 MiB member of tiny entries compresses to almost nothing) and counts as unreadable.
pub const MAX_LIST_NAMES: usize = 10_000;

/// The most script-loaded lists Silo reads from one mod for its estimate (real mods ship up to
/// five). Once the estimate holds [`MAX_LIST_NAMES`] names it stops growing too: it's far past
/// any cap by then.
pub const MAX_LIST_FILES: usize = 100;

/// Append the items `seen` doesn't hold yet to `out`, in order, and remember them. Linear, so a
/// huge list can't stall a check the way `Vec::contains` would.
pub(crate) fn push_new(
    out: &mut Vec<String>,
    seen: &mut HashSet<String>,
    items: impl IntoIterator<Item = String>,
) {
    for t in items {
        if !seen.contains(&t) {
            seen.insert(t.clone());
            out.push(t);
        }
    }
}

/// The `name_attr` values of `<root><list><item>` entries, upper-cased and de-duplicated, and the
/// list element's `numChannels`. `None` when the XML isn't a complete, well-formed document or
/// an element on the way has a malformed attribute: the game can't load such a file. Also `None`
/// past [`MAX_LIST_NAMES`] distinct names. Pure.
pub(crate) fn named_list(
    xml: &[u8],
    list: &[u8],
    item: &[u8],
    name_attr: &[u8],
) -> Option<(Vec<String>, Option<String>)> {
    let (mut names, mut seen, mut channels) = (Vec::new(), HashSet::new(), None);
    walk(xml, |depth, parent, e| {
        let name = e.name();
        if depth == 1 && name.as_ref() == list && channels.is_none() {
            channels = attr(e, b"numChannels")?;
        }
        if depth == 2 && parent == list && name.as_ref() == item {
            if let Some(n) = attr(e, name_attr)? {
                let n = n.trim().to_uppercase();
                if !n.is_empty() && !seen.contains(&n) {
                    if names.len() >= MAX_LIST_NAMES {
                        return None;
                    }
                    seen.insert(n.clone());
                    names.push(n);
                }
            }
        }
        Some(())
    })?;
    Some((names, channels))
}

/// Visit every opening element of a whole document with its depth (0 for the root) and its
/// parent's name. `None` if the document is malformed or incomplete, or `visit` says so.
fn walk(
    xml: &[u8],
    mut visit: impl FnMut(usize, &[u8], &quick_xml::events::BytesStart) -> Option<()>,
) -> Option<()> {
    use quick_xml::events::Event;
    use quick_xml::Reader;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    // Open elements from the root down; `Empty` elements never open.
    let mut stack: Vec<Vec<u8>> = Vec::new();
    let mut roots = 0;
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf).ok()? {
            Event::Start(e) => {
                roots += usize::from(stack.is_empty());
                well_formed(&e)?;
                visit(stack.len(), stack.last().map_or(&[][..], |p| p), &e)?;
                stack.push(e.name().as_ref().to_vec());
            }
            Event::Empty(e) => {
                roots += usize::from(stack.is_empty());
                well_formed(&e)?;
                visit(stack.len(), stack.last().map_or(&[][..], |p| p), &e)?;
            }
            Event::End(_) => {
                stack.pop()?;
            }
            // Text or CDATA outside the root makes the document invalid.
            Event::Text(t) if stack.is_empty() && !t.is_empty() => return None,
            Event::CData(_) if stack.is_empty() => return None,
            // Only the five predefined entities and character references resolve: these files
            // declare no others.
            Event::GeneralRef(r) => {
                if r.is_char_ref() {
                    r.resolve_char_ref().ok()??;
                } else if !matches!(&r[..], b"lt" | b"gt" | b"amp" | b"apos" | b"quot") {
                    return None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buf.clear();
    }
    (stack.is_empty() && roots == 1).then_some(())
}

/// Every attribute of an element parses and decodes (no unknown entity), with no name repeated.
fn well_formed(e: &quick_xml::events::BytesStart) -> Option<()> {
    for a in e.attributes().with_checks(true) {
        a.ok()?
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .ok()?;
    }
    Some(())
}

/// The map's own height-type file, from its `map.xml`. See [`map_own_file`]. Pure.
pub fn map_list_file(map_xml: &[u8]) -> Option<Option<String>> {
    map_own_file(map_xml, b"densityMapHeightTypes")
}

/// The file a map's `map.xml` names in `<root><tag filename>`: `Some(Some(file))` when it points
/// inside the map, `Some(None)` when it has no such reference or points at the base game's
/// `$data/...`, and `None` when the `map.xml` doesn't parse whole, so Silo can't tell. Only the
/// root's own child counts. Pure.
pub fn map_own_file(map_xml: &[u8], tag: &[u8]) -> Option<Option<String>> {
    let mut found: Option<Option<String>> = None;
    walk(map_xml, |depth, _, e| {
        if depth == 1 && e.name().as_ref() == tag && found.is_none() {
            found = Some(attr(e, b"filename")?);
        }
        Some(())
    })?;
    Some(
        found
            .flatten()
            .map(|f| f.trim().replace('\\', "/"))
            .filter(|f| !f.is_empty() && !f.starts_with('$')),
    )
}

/// An attribute's value: `Some(None)` when it's absent, `None` when the element's attributes
/// are malformed (a broken or repeated attribute), which makes the whole file unreadable.
fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<Option<String>> {
    let mut out = None;
    for a in e.attributes().with_checks(true) {
        let a = a.ok()?;
        if a.key.as_ref() == key {
            let v = a
                .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()?;
            out = Some(v.into_owned());
        }
    }
    Some(out)
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
    /// Installed DLCs that turn themselves off without their slots, by name.
    pub at_risk: Vec<String>,
}

/// DLCs that turn themselves off and close the save when they can't get their slots (measured
/// in-game, FS25 1.25), keyed by `pdlc/<stem>.dlc`.
const AT_RISK: &[(&str, &str)] = &[
    ("strawHarvestPack", "Straw Harvest"),
    ("pumpsAndHosesPack", "Pumps n' Hoses"),
];

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
    let (mut out, mut seen) = (Vec::new(), HashSet::new());
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
            let names = std::fs::read(&p).map(|b| parse(&b)).unwrap_or_default();
            push_new(&mut out, &mut seen, names);
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
            at_risk: AT_RISK
                .iter()
                .filter(|(stem, _)| stems.iter().any(|s| s.eq_ignore_ascii_case(stem)))
                .map(|(_, name)| name.to_string())
                .collect(),
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
#[derive(Debug, Clone, Default)]
pub struct ModTypes {
    pub label: String,
    pub tech_name: String,
    /// Declared in modDesc: the game loads these.
    pub exact: Vec<String>,
    /// Found in other `*heightType*.xml` files the mod ships: a script may load them.
    pub estimate: Vec<String>,
}

impl ModTypes {
    fn all(&self) -> impl Iterator<Item = &str> {
        self.exact.iter().chain(&self.estimate).map(String::as_str)
    }

    /// Move declared entries whose fill type Silo can't find to the estimates. The game rejects a
    /// height type whose fill type isn't registered ("has invalid fill type"), but a mod or map may
    /// register the fill type somewhere Silo doesn't look (a script, a crop file), so not finding
    /// it is no proof: the entry still counts, just not toward a Problem.
    pub fn downgrade_unknown(&mut self, known: &HashSet<String>) {
        let (keep, unsure): (Vec<String>, Vec<String>) =
            self.exact.drain(..).partition(|t| known.contains(t));
        self.exact = keep;
        let mut seen: HashSet<String> = self.estimate.iter().cloned().collect();
        push_new(&mut self.estimate, &mut seen, unsure);
    }
}

/// Where a map's height types come from.
#[derive(Debug, Clone, PartialEq)]
pub enum MapList {
    /// It has no list of its own: the base game's.
    Base,
    /// Its own list, read in full. `unsure` holds the entries whose fill type Silo can't find.
    Own {
        names: Vec<String>,
        unsure: Vec<String>,
        channels: u32,
    },
    /// It points at a list Silo can't read, or one that names nothing: what loads is unknown, so
    /// the base game's types count as an estimate. `channels` is the list's own `numChannels`
    /// when Silo could read it; without it the cap is unknown too, and a 6-channel guess can't
    /// make a Problem.
    Unsure { channels: Option<u32> },
}

/// A candidate map: its list and what its modDesc adds.
#[derive(Debug, Clone)]
pub struct MapTypes {
    pub label: String,
    pub list: MapList,
    /// What the map's modDesc adds on top, like any mod.
    pub extra: Option<ModTypes>,
}

impl MapTypes {
    /// [`ModTypes::downgrade_unknown`] for the map's own list and its extras.
    pub fn downgrade_unknown(&mut self, known: &HashSet<String>) {
        if let MapList::Own { names, unsure, .. } = &mut self.list {
            let (keep, move_out): (Vec<String>, Vec<String>) =
                names.drain(..).partition(|t| known.contains(t));
            *names = keep;
            unsure.extend(move_out);
        }
        if let Some(extra) = &mut self.extra {
            extra.downgrade_unknown(known);
        }
    }
}

/// Does this member's file name look like a height-type list a script might load?
pub fn is_candidate(member: &str) -> bool {
    let base = member.rsplit('/').next().unwrap_or(member).to_lowercase();
    base.contains("heighttype") && base.ends_with(".xml")
}

/// Read a mod's height types: the modDesc-declared file exactly, plus (for non-maps) any other
/// `*heightType*.xml` in it as an estimate. `read` fetches a member's bytes; `candidates` lists the
/// members that pass [`is_candidate`].
pub fn mod_types(
    label: &str,
    tech_name: &str,
    md: &ModDesc,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
    candidates: &dyn Fn() -> Vec<String>,
) -> Option<ModTypes> {
    let declared = md.height_types_file.as_deref();
    let exact = declared
        .and_then(read)
        .map(|b| parse_list(&b).0)
        .unwrap_or_default();
    let mut estimate: Vec<String> = Vec::new();
    if !md.is_map {
        // Seeded with the exact names, so an estimate never repeats one.
        let mut seen: HashSet<String> = exact.iter().cloned().collect();
        for m in candidates().into_iter().take(MAX_LIST_FILES) {
            if seen.len() >= MAX_LIST_NAMES {
                break;
            }
            if Some(m.as_str()) == declared {
                continue;
            }
            let names = read(&m).map(|b| parse_list(&b).0).unwrap_or_default();
            push_new(&mut estimate, &mut seen, names);
        }
    }
    (!exact.is_empty() || !estimate.is_empty()).then(|| ModTypes {
        label: label.into(),
        tech_name: tech_name.into(),
        exact,
        estimate,
    })
}

/// The budget for one map.
#[derive(Debug, Clone, PartialEq)]
pub struct Budget {
    /// The mod map's name, or `None` for the base-game maps.
    pub map: Option<String>,
    pub cap: usize,
    pub base: usize,
    /// Slots the installed DLCs add beyond the map's list.
    pub dlc: usize,
    /// Whether the base crops' types are in `base` (only some maps grow those crops).
    pub crops_estimated: bool,
    /// Whether Silo couldn't read the map's own list, or it named nothing.
    pub list_unsure: bool,
    /// Whether `cap` is a guess (the map's list couldn't be read).
    pub cap_unsure: bool,
    /// Installed DLCs that turn themselves off without their slots.
    pub at_risk: Vec<String>,
    /// Slots counted from the map list and modDesc-declared mod lists only.
    pub exact_total: usize,
    /// Every slot, estimates included.
    pub total: usize,
    /// Mods adding new slots, most first: (label, tech name, slots, all of them declared).
    pub mods: Vec<(String, String, usize, bool)>,
}

/// Count one map's slots. Pure.
pub fn evaluate(game: &GameData, map: Option<&MapTypes>, mods: &[ModTypes]) -> Budget {
    let (base, unsure, channels, cap_unsure) = match map.map(|m| &m.list) {
        None | Some(MapList::Base) => (game.base.clone(), vec![], game.base_channels, false),
        Some(MapList::Own {
            names,
            unsure,
            channels,
        }) => (names.clone(), unsure.clone(), *channels, false),
        Some(MapList::Unsure { channels }) => (
            vec![],
            game.base.clone(),
            channels.unwrap_or(DEFAULT_CHANNELS),
            channels.is_none(),
        ),
    };
    // The map's list is exact; what Silo couldn't confirm, and the base crops' types (they load
    // only on maps that grow them), are estimates.
    let mut exact: BTreeSet<&str> = base.iter().map(String::as_str).collect();
    let listed: BTreeSet<&str> = exact
        .iter()
        .copied()
        .chain(unsure.iter().map(String::as_str))
        .collect();
    let list_unsure = listed.len() > exact.len();
    let base_set: BTreeSet<&str> = listed
        .iter()
        .copied()
        .chain(game.foliage.iter().map(String::as_str))
        .collect();
    let crops_estimated = base_set.len() > listed.len();
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
        cap: cap_for(channels),
        base: base_set.len(),
        dlc,
        crops_estimated,
        list_unsure,
        cap_unsure,
        at_risk: game.at_risk.clone(),
        exact_total: exact.len(),
        total: all.len(),
        mods: per_mod,
    }
}

/// How serious a severity is, for picking the worst map: critical first.
pub(crate) fn severity_rank(s: Option<&str>) -> u8 {
    match s {
        Some("critical") => 3,
        Some("warning") => 2,
        Some("info") => 1,
        _ => 0,
    }
}

/// The worst budget across the maps the save might use: the base-game maps and each active mod
/// map (only the savegame's map loads, and Silo doesn't know which that is). Worst means most
/// serious first, then least room. Pure.
pub fn worst(game: &GameData, maps: &[MapTypes], mods: &[ModTypes]) -> Budget {
    std::iter::once(evaluate(game, None, mods))
        .chain(maps.iter().map(|m| evaluate(game, Some(m), mods)))
        .max_by_key(|b| (severity_rank(severity(b)), b.total as i64 - b.cap as i64))
        .expect("the base-game maps are always a candidate")
}

/// How serious a budget is: critical when the exactly-counted slots alone run over, warning when
/// the estimates push it over, info when 3 or fewer are left, else nothing. Pure.
pub fn severity(b: &Budget) -> Option<&'static str> {
    if b.exact_total > b.cap && !b.cap_unsure {
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
        None => "The base-game maps have".into(),
    };
    let mod_slots: usize = b.total - b.base - b.dlc;
    let top: Vec<String> = b
        .mods
        .iter()
        .take(5)
        .map(|(l, _, n, _)| format!("{l} ({n})"))
        .collect();
    let mut s = format!(
        "{on} room for {} ground-pile types, the materials you can tip onto the ground. The base game uses {}, your installed DLCs {}, and your active mods {} more: {} in all.",
        b.cap, b.base, b.dlc, mod_slots, b.total
    );
    let risk = b.at_risk.join(" and ");
    if b.total > b.cap {
        s.push_str(" If that's what loads, the game rejects the ones that don't fit");
        match b.at_risk.len() {
            0 => s.push_str(", and those materials can't lie on the ground as piles."),
            n => s.push_str(&format!(
                ". In testing, mods registered before the DLCs, so {risk} may then turn {} off and close the save.",
                if n == 1 { "itself" } else { "themselves" }
            )),
        }
    } else {
        let left = b.cap - b.total;
        let is = if left == 1 { "is" } else { "are" };
        if b.at_risk.is_empty() {
            s.push_str(&format!(
                " Only {left} {is} left before the game starts rejecting them."
            ));
        } else {
            s.push_str(&format!(
                " Only {left} {is} left, so one more mod that adds piles could stop {risk} from loading."
            ));
        }
    }
    if !top.is_empty() {
        s.push_str(&format!(
            " New types per mod (some mods share them): {}.",
            top.join(", ")
        ));
    }
    let mut why: Vec<&str> = Vec::new();
    if b.dlc > 0 {
        why.push("a DLC that's off in your save takes none");
    }
    if b.crops_estimated {
        why.push("a few belong to crops only some maps grow");
    }
    if b.list_unsure {
        why.push("Silo couldn't fully read the map's own list");
    }
    if b.mods.iter().any(|m| !m.3) {
        why.push("some mods' lists may not load, or name materials Silo can't find");
    }
    if !why.is_empty() {
        s.push_str(&format!(" Some counts are estimates: {}.", why.join("; ")));
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
            at_risk: vec![],
        }
    }

    fn m(label: &str, types: &[&str], exact: bool) -> ModTypes {
        mm(label, types.iter().map(|s| s.to_string()).collect(), exact)
    }

    fn many(prefix: &str, n: usize) -> Vec<String> {
        (0..n).map(|i| format!("{prefix}{i}")).collect()
    }

    fn mm(label: &str, types: Vec<String>, exact: bool) -> ModTypes {
        let (exact, estimate) = if exact {
            (types, vec![])
        } else {
            (vec![], types)
        };
        ModTypes {
            label: label.into(),
            tech_name: format!("FS25_{label}"),
            exact,
            estimate,
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
        // Only where the game reads them: entries elsewhere, or in a file that doesn't parse,
        // take no slot.
        let elsewhere = br#"<map><examples><densityMapHeightType fillTypeName="X"/></examples>
            <densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>"#;
        assert_eq!(parse_list(elsewhere).0, ["Y"]);
        let broken = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></wrong></map>"#;
        assert!(parse_list(broken).0.is_empty());
        assert_eq!(parse_list_checked(broken), None);
        // A repeated attribute on any element, or text outside the root, breaks the file too.
        let dup = br#"<map><x a="1" a="2"/><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>"#;
        assert_eq!(parse_list_checked(dup), None);
        let stray = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>junk"#;
        assert_eq!(parse_list_checked(stray), None);
        let cdata = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map><![CDATA[junk]]>"#;
        assert_eq!(parse_list_checked(cdata), None);
        // An undeclared entity, in an attribute nothing reads or in text, breaks the file too.
        let entity = br#"<map unused="&bogus;"><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>"#;
        assert_eq!(parse_list_checked(entity), None);
        let text = br#"<map><note>&bogus;</note><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>"#;
        assert_eq!(parse_list_checked(text), None);
        let fine = br#"<map><note>A &amp; B &#233;</note><densityMapHeightTypes><densityMapHeightType fillTypeName="Y"/></densityMapHeightTypes></map>"#;
        assert_eq!(
            parse_list_checked(fine),
            Some((vec!["Y".to_string()], None))
        );
        // An empty but valid list is not a failure.
        assert_eq!(
            parse_list_checked(b"<map><densityMapHeightTypes/></map>"),
            Some((vec![], None))
        );
    }

    /// Past [`MAX_LIST_NAMES`] distinct names a height-type list can't be read, the same as a
    /// broken file, so the map's list counts as unsure (#351).
    #[test]
    fn a_list_past_the_name_cap_is_unreadable() {
        let list = |n: usize| {
            let mut xml = String::from("<map><densityMapHeightTypes numChannels=\"7\">");
            for i in 0..n {
                xml.push_str(&format!("<densityMapHeightType fillTypeName=\"t{i}\"/>"));
            }
            xml.push_str("</densityMapHeightTypes></map>");
            xml.into_bytes()
        };
        let (names, ch) = parse_list_checked(&list(MAX_LIST_NAMES)).unwrap();
        assert_eq!((names.len(), ch), (MAX_LIST_NAMES, Some(7)));
        assert_eq!(parse_list_checked(&list(MAX_LIST_NAMES + 1)), None);
        assert!(parse_list(&list(MAX_LIST_NAMES + 1)).0.is_empty());
    }

    #[test]
    fn push_new_keeps_order_and_skips_seen() {
        let mut out = vec!["A".to_string()];
        let mut seen: HashSet<String> = ["A".to_string(), "Z".to_string()].into();
        push_new(
            &mut out,
            &mut seen,
            ["B", "A", "Z", "C", "B"].map(String::from),
        );
        assert_eq!(out, ["A", "B", "C"]);
        assert!(seen.contains("C"));
    }

    #[test]
    fn map_list_file_only_when_it_points_inside_the_map() {
        assert_eq!(
            map_list_file(br#"<map><densityMapHeightTypes filename="map\xml\types.xml"/></map>"#),
            Some(Some("map/xml/types.xml".into()))
        );
        assert_eq!(
            map_list_file(
                br#"<map><densityMapHeightTypes filename="$data/maps/maps_densityMapHeightTypes.xml"/></map>"#
            ),
            Some(None)
        );
        assert_eq!(map_list_file(b"<map><fillTypes/></map>"), Some(None));
        // A map.xml that doesn't parse whole, or has a broken attribute anywhere: can't tell.
        assert_eq!(
            map_list_file(b"<map><densityMapHeightTypes filename=\"x.xml\"/>"),
            None
        );
        assert_eq!(
            map_list_file(
                br#"<map><other a="1" a="2"/><densityMapHeightTypes filename="x.xml"/></map>"#
            ),
            None
        );
        // A reference nested deeper is not the map's.
        assert_eq!(
            map_list_file(
                br#"<map><examples><densityMapHeightTypes filename="old.xml"/></examples></map>"#
            ),
            Some(None)
        );
        // A commented-out reference is no reference (FS25_BackRoadsCounty4x does this).
        assert_eq!(
            map_list_file(br#"<map><!-- <densityMapHeightTypes filename="x.xml" /> --></map>"#),
            Some(None)
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
            text.contains("your installed DLCs 3, and your active mods 16 more: 67 in all"),
            "{text}"
        );
        assert!(text.contains("some mods' lists may not load"), "{text}");
        assert!(
            text.contains("a DLC that's off in your save takes none"),
            "{text}"
        );
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
            list: MapList::Own {
                names: many("M", 40),
                unsure: vec![],
                channels: 7,
            },
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
            list: MapList::Own {
                names: many("M", 10),
                unsure: vec![],
                channels: 6,
            },
            extra: None,
        };
        let stock = MapTypes {
            label: "Stock-list map".into(),
            list: MapList::Base,
            extra: None,
        };
        let mods = [mm("Fills", many("F", 20), true)];
        let b = worst(&game(48, &[]), &[roomy, stock], &mods);
        assert_eq!(b.map.as_deref(), Some("Stock-list map"));
        assert_eq!(b.total, 68);
        let base = worst(&game(48, &[]), &[], &mods);
        assert_eq!(base.map, None);
        assert!(explain(&base).starts_with("The base-game maps have room"));
    }

    /// An active mod map doesn't mean the save uses it: the base-game maps stay a candidate.
    #[test]
    fn a_roomy_mod_map_does_not_hide_the_base_maps() {
        let roomy = MapTypes {
            label: "Roomy".into(),
            list: MapList::Own {
                names: many("M", 40),
                unsure: vec![],
                channels: 7,
            },
            extra: None,
        };
        let b = worst(
            &game(48, &[]),
            &[roomy],
            &[mm("Fills", many("F", 20), true)],
        );
        assert_eq!((b.map, b.cap, b.total), (None, 63, 68));
    }

    /// A Problem on one map outranks a bigger Review on another.
    /// A map list that names nothing but sets 7 channels keeps a 127 cap; the base types count
    /// as an estimate, since what loads instead is unknown.
    #[test]
    fn an_empty_map_list_still_sets_the_channels() {
        let map = MapTypes {
            label: "Wide".into(),
            list: MapList::Unsure { channels: Some(7) },
            extra: None,
        };
        let b = evaluate(
            &game(48, &[]),
            Some(&map),
            &[mm("Fills", many("F", 16), true)],
        );
        assert_eq!((b.base, b.cap, b.total), (48, 127, 64));
        // What loads is unknown, so the base types are only an estimate.
        assert_eq!(b.exact_total, 16);
        assert_eq!(severity(&b), None);
        assert!(explain(&b).contains("Silo couldn't fully read the map's own list"));
    }

    /// An unreadable map list can't make a Problem: its base types are estimates, and its cap
    /// is a guess, so even 64 exactly-declared extras are a Review (the map may have 7 channels).
    #[test]
    fn an_unreadable_map_list_is_never_a_problem() {
        let wide = MapTypes {
            label: "Unknown".into(),
            list: MapList::Unsure { channels: None },
            extra: None,
        };
        let b = evaluate(
            &game(48, &[]),
            Some(&wide),
            &[mm("Fills", many("F", 64), true)],
        );
        assert_eq!((b.exact_total, b.cap, b.cap_unsure), (64, 63, true));
        assert_eq!(severity(&b), Some("warning"));
        let map = MapTypes {
            label: "Broken".into(),
            list: MapList::Unsure { channels: None },
            extra: None,
        };
        let b = evaluate(
            &game(48, &[]),
            Some(&map),
            &[mm("Fills", many("F", 16), true)],
        );
        assert_eq!((b.exact_total, b.total), (16, 64));
        assert_eq!(severity(&b), Some("warning"));
    }

    /// Entries in a map's own list whose fill type Silo can't find become estimates too.
    #[test]
    fn a_map_lists_unknown_entries_become_estimates() {
        let mut map = MapTypes {
            label: "Own".into(),
            list: MapList::Own {
                names: many("M", 64),
                unsure: vec![],
                channels: 6,
            },
            extra: None,
        };
        let known: std::collections::HashSet<String> = many("M", 62).into_iter().collect();
        map.downgrade_unknown(&known);
        let b = evaluate(&game(48, &[]), Some(&map), &[]);
        assert_eq!((b.exact_total, b.total), (62, 64));
        assert_eq!(severity(&b), Some("warning"));
    }

    #[test]
    fn worst_ranks_by_severity_first() {
        // Base maps: 48 exact + 5 DLC + 12 estimated = 65, a warning.
        let g = game(48, &["D0", "D1", "D2", "D3", "D4"]);
        // Mod map: its own 64-name list, already over 63 exactly, holds the DLC and script names
        // too, so it totals only 64: critical, with less overflow than the base maps' 65.
        let mut own = many("D", 5);
        own.extend(many("S", 12));
        own.extend(many("M", 47));
        let tight = MapTypes {
            label: "Tight".into(),
            list: MapList::Own {
                names: own,
                unsure: vec![],
                channels: 6,
            },
            extra: None,
        };
        let mods = [mm("Script", many("S", 12), false)];
        assert_eq!(evaluate(&g, None, &mods).total, 65);
        let b = worst(&g, &[tight], &mods);
        assert_eq!(b.map.as_deref(), Some("Tight"));
        assert_eq!(b.total, 64);
        assert_eq!(severity(&b), Some("critical"));
    }

    #[test]
    fn mod_types_counts_declared_and_shipped_lists() {
        let list = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="A"/></densityMapHeightTypes></map>"#.to_vec();
        let other = br#"<map><densityMapHeightTypes><densityMapHeightType fillTypeName="A"/><densityMapHeightType fillTypeName="B"/></densityMapHeightTypes></map>"#.to_vec();
        let read = |name: &str| match name {
            "xml/declared.xml" => Some(list.clone()),
            "xml/densityMapHeightTypes.xml" => Some(other.clone()),
            _ => None,
        };
        let candidates = || {
            vec![
                "xml/declared.xml".to_string(),
                "xml/densityMapHeightTypes.xml".to_string(),
            ]
        };

        // The declared file is exact; another list it ships may also be loaded by a script.
        let declared = ModDesc {
            height_types_file: Some("xml/declared.xml".into()),
            ..Default::default()
        };
        let t = mod_types("M", "FS25_M", &declared, &read, &candidates).unwrap();
        assert_eq!(
            (t.exact, t.estimate),
            (vec!["A".to_string()], vec!["B".to_string()])
        );

        // No declaration: every shipped list is an estimate.
        let t = mod_types("M", "FS25_M", &ModDesc::default(), &read, &candidates).unwrap();
        assert!(t.exact.is_empty());
        assert_eq!(t.estimate, ["A", "B"]);

        // A map's shipped lists are the map's business, never a mod estimate.
        let map = ModDesc {
            is_map: true,
            ..Default::default()
        };
        assert!(mod_types("M", "FS25_M", &map, &read, &candidates).is_none());
        // Nothing found: no entry.
        assert!(mod_types("M", "FS25_M", &ModDesc::default(), &read, &Vec::new).is_none());
    }

    #[test]
    fn candidates_are_height_type_xml_by_name() {
        assert!(is_candidate("xml/densityMapHeightTypes.xml"));
        assert!(is_candidate("xml/BSU_heightTypes.XML"));
        assert!(!is_candidate("xml/heightTypes.lua"));
        assert!(!is_candidate("heighttypes/fillTypes.xml"));
    }

    /// Not finding a fill type is no proof it won't exist, so the entry stays, as an estimate.
    #[test]
    fn unknown_fill_types_become_estimates() {
        let mut t = mm("Fills", vec!["REAL".into(), "GHOST".into()], true);
        t.estimate = vec!["MAYBE".into()];
        let known: std::collections::HashSet<String> =
            ["REAL"].iter().map(|s| s.to_string()).collect();
        t.downgrade_unknown(&known);
        assert_eq!(t.exact, ["REAL"]);
        assert_eq!(t.estimate, ["MAYBE", "GHOST"]);
        // So 48 + 14 declared + 2 unknown is a Review at most, never a Problem.
        let mut t = mm("Big", many("B", 16), true);
        let known: std::collections::HashSet<String> = many("B", 14).into_iter().collect();
        t.downgrade_unknown(&known);
        let b = evaluate(&game(48, &[]), None, &[t]);
        assert_eq!((b.exact_total, b.total), (62, 64));
        assert_eq!(severity(&b), Some("warning"));
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
            list: MapList::Own {
                names: many("M", 10),
                unsure: vec![],
                channels: 6,
            },
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
        assert_eq!(g.at_risk, ["Straw Harvest"]);
        assert_eq!(g.base_channels, 6);

        // No install: the built-in 1.25 lists, 51 slots, and no DLCs.
        let none = GameData::load(None);
        assert_eq!(none.base.len() + none.foliage.len(), 51);
        assert!(none.dlc.is_empty());
    }

    /// The shutdown is only named for DLCs the player has installed.
    #[test]
    fn the_shutdown_is_named_only_for_installed_dlcs() {
        let over = |at_risk: &[&str]| {
            let mut g = game(48, &[]);
            g.at_risk = at_risk.iter().map(|s| s.to_string()).collect();
            explain(&evaluate(&g, None, &[mm("Fills", many("F", 20), true)]))
        };
        let none = over(&[]);
        assert!(
            none.contains("those materials can't lie on the ground as piles"),
            "{none}"
        );
        assert!(!none.contains("Straw Harvest"), "{none}");
        let one = over(&["Straw Harvest"]);
        assert!(
            one.contains("so Straw Harvest may then turn itself off and close the save"),
            "{one}"
        );
        let two = over(&["Straw Harvest", "Pumps n' Hoses"]);
        assert!(
            two.contains("Straw Harvest and Pumps n' Hoses may then turn themselves off"),
            "{two}"
        );
        let mut g = game(48, &[]);
        g.at_risk = vec!["Pumps n' Hoses".into()];
        let near = explain(&evaluate(&g, None, &[mm("Fills", many("F", 13), true)]));
        assert!(
            near.contains("could stop Pumps n' Hoses from loading"),
            "{near}"
        );
        let near_none = explain(&evaluate(
            &game(48, &[]),
            None,
            &[mm("Fills", many("F", 13), true)],
        ));
        assert!(
            near_none.contains("left before the game starts rejecting them"),
            "{near_none}"
        );
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
