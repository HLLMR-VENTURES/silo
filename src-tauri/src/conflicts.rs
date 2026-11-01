//! Conflict detection over an active set of mods — the thing no other FS tool does
//! well. We re-parse each active mod's `modDesc.xml` and flag collisions across the
//! GIANTS namespaces that actually clash at load time:
//!
//! * `<uniqueType>` — GIANTS' own primitive: only one mod of a type loads (critical)
//! * input actions, brands and shop categories registered under the same name (critical)
//! * `<extraSourceFiles>` with the same basename (info only: the game loads each mod's
//!   scripts by full path into that mod's own environment, so names don't collide)
//! * more than one active map (info only: FS25 loads just the savegame's map)
//! * more ground-pile (height) types than the map has slots for (#339, see `heighttypes`)
//! * more fill types than the game's 255 (#341, see `filltypes`)
//!
//! Signal refinement: a clash among mods by the **same author** is usually
//! intentional (shared packs/utilities), so it's down-ranked (critical→warning,
//! warning→info) with a note, so the critical list stays trustworthy. And because only the
//! savegame's map loads, the active maps count as **one participant** in every collision
//! (#352): two maps never clash with each other, only with a non-map mod.
//!
//! Not flagged: the six specialization / type surfaces (`<specializations>`, `<vehicleTypes>`
//! and their placeable / hand-tool twins). The game registers a mod's types as
//! `<modName>.<type>` (`TypeManager.lua` in gameSource, `typeName = modName.."."..typeName`)
//! and resolves a type's specializations with a `<modName>.<spec>` fallback, so two mods using
//! the same name both load. Flagging them made every common name ("myTractor") a critical.
//!
//! Runs on demand for the active set only, so it stays cheap.

use crate::filltypes::{self, fill_type_names};
use crate::heighttypes;
use crate::moddesc;
use crate::scan;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConflictInput {
    pub tech_name: String,
    pub title: Option<String>,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    /// Stable identity (kind, name and the sorted tech names), for list keys.
    pub id: String,
    pub severity: String, // "critical" | "warning" | "info"
    pub kind: String,     // "uniqueType" | "specialization" | "vehicleType" | "script" | …
    pub name: String,
    pub explanation: String,
    pub mods: Vec<String>, // display labels of the mods involved
    /// The involved mods' tech names, in the same order as `mods`.
    pub tech_names: Vec<String>,
}

/// `kind:name:techA,techB` with the tech names sorted, so the id doesn't depend on load order.
fn conflict_id(kind: &str, name: &str, tech_names: &[String]) -> String {
    let mut t = tech_names.to_vec();
    t.sort();
    format!("{kind}:{name}:{}", t.join(","))
}

/// The involved mods: display labels and tech names, deduplicated by label, in step.
type Involved = (Vec<String>, Vec<String>);

/// Build a conflict and its id.
fn conflict(
    severity: &str,
    kind: &str,
    name: &str,
    explanation: String,
    (mods, tech_names): Involved,
) -> Conflict {
    Conflict {
        id: conflict_id(kind, name, &tech_names),
        severity: severity.into(),
        kind: kind.into(),
        name: name.into(),
        explanation,
        mods,
        tech_names,
    }
}

/// Registrations that share one game-wide namespace, so the same name from two mods clashes.
/// Types and specializations don't: the game prefixes them with the mod's name (see the
/// module docs).
fn clashes_globally(kind: &str) -> bool {
    !matches!(
        kind,
        "specialization"
            | "placeableSpecialization"
            | "handToolSpecialization"
            | "vehicleType"
            | "placeableType"
            | "handToolType"
    )
}

struct Parsed {
    label: String,
    tech_name: String,
    author: Option<String>,
    path: String,
    kind: String,
    md: moddesc::ModDesc,
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "action" => "control action",
        "brand" => "brand",
        "storeCategory" => "shop category",
        other => other,
    }
}

/// All involved mods share one non-empty author.
fn same_author(parsed: &[Parsed], idxs: &[usize]) -> bool {
    let mut it = idxs
        .iter()
        .map(|&i| parsed[i].author.as_deref().unwrap_or(""));
    match it.next() {
        Some(first) if !first.is_empty() => it.all(|a| a.eq_ignore_ascii_case(first)),
        _ => false,
    }
}

/// The members of a zip or unpacked dir whose `/`-separated path passes `keep`. Filtering while
/// listing keeps the lists small (most mods have no candidates), though every mod is still
/// listed on each scan.
fn candidate_members(path: &str, kind: &str, keep: fn(&str) -> bool) -> Vec<String> {
    if kind == "zip" {
        let Ok(f) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let Ok(ar) = zip::ZipArchive::new(f) else {
            return Vec::new();
        };
        ar.file_names()
            .flatten()
            .filter(|n| keep(n))
            .map(|n| n.replace('\\', "/"))
            .collect()
    } else {
        let root = Path::new(path);
        walkdir::WalkDir::new(root)
            .into_iter()
            .flatten()
            .filter(|e| e.file_type().is_file())
            .filter_map(|e| {
                let rel = e.path().strip_prefix(root).ok()?;
                Some(rel.to_string_lossy().replace('\\', "/"))
            })
            .filter(|n| keep(n))
            .collect()
    }
}

/// Read a member file's bytes from a zip or unpacked dir.
fn read_member(path: &str, kind: &str, member: &str) -> Option<Vec<u8>> {
    let member = member.replace('\\', "/");
    if kind == "zip" {
        let f = std::fs::File::open(path).ok()?;
        let mut ar = zip::ZipArchive::new(f).ok()?;
        let e = ar.by_name(&member).ok()?;
        // Cap the read — a compared member could be a zip-bombed script.
        crate::scan::read_capped(e, 32 * 1024 * 1024, &member).ok()
    } else {
        let f = crate::paths::open_inside(Path::new(path), &member)?;
        crate::scan::read_capped(f, 32 * 1024 * 1024, &member).ok()
    }
}

/// The involved mods, deduplicated by tech name (two different mods may share a title).
fn labels(parsed: &[Parsed], idxs: &[usize]) -> Involved {
    let mut out: Vec<String> = Vec::new();
    let mut techs: Vec<String> = Vec::new();
    for &i in idxs {
        if !techs.contains(&parsed[i].tech_name) {
            out.push(parsed[i].label.clone());
            techs.push(parsed[i].tech_name.clone());
        }
    }
    (out, techs)
}

/// The distinct mods in a collision, split into (maps, non-map mods). Distinct by tech name, as
/// [`labels`] counts them: a tech name seen twice (say a zip and a folder) counts once, by its
/// first entry.
fn maps_and_others(parsed: &[Parsed], idxs: &[usize]) -> (usize, usize) {
    let mut seen: Vec<&str> = Vec::new();
    let (mut maps, mut others) = (0, 0);
    for &i in idxs {
        let Some(p) = parsed.get(i) else { continue };
        if seen.contains(&p.tech_name.as_str()) {
            continue;
        }
        seen.push(&p.tech_name);
        if p.md.is_map {
            maps += 1;
        } else {
            others += 1;
        }
    }
    (maps, others)
}

/// How many mods take part in a collision once it loads: distinct non-map mods, plus one for
/// the maps. FS25 loads only the savegame's map (in-game test 2026-10-18), so two maps never
/// load together and can't clash with each other, only with a non-map mod (#352). Below 2,
/// there's no collision to report.
fn participants(parsed: &[Parsed], idxs: &[usize]) -> usize {
    let (maps, others) = maps_and_others(parsed, idxs);
    others + usize::from(maps > 0)
}

/// When maps take part in a collision, when they count: only on a save on that map, since no
/// other map loads (#352). With one non-map mod the whole clash depends on it; with more, the
/// non-map mods clash anyway and a map only joins in. Empty when no map is involved.
fn map_condition(parsed: &[Parsed], idxs: &[usize]) -> &'static str {
    match maps_and_others(parsed, idxs) {
        (0, _) => "",
        (1, 0 | 1) => " A map loads only when your savegame is on it, so this matters only when you play on that map.",
        (_, 0 | 1) => " Only the map your savegame is on loads, so this matters only when you play on one of these maps.",
        (1, _) => " The map takes part only when your savegame is on it.",
        _ => " Each map takes part only when your savegame is on it, and only one loads.",
    }
}

/// "the other" for two participants, "the others" for more.
fn the_others(n: usize) -> &'static str {
    if n == 2 {
        "the other"
    } else {
        "the others"
    }
}

/// Note when more than one map is active. FS25 loads only the map the savegame uses and lists
/// the others as "Available mod" in log.txt without loading them (in-game test 2026-10-18: four
/// maps active, a base-map save loaded cleanly), so this is information, never a problem. Pure
/// over the active maps, so it's testable without fixture mods.
fn map_conflict(maps: Involved) -> Option<Conflict> {
    let n = maps.0.len();
    if n < 2 {
        return None;
    }
    Some(conflict(
        "info",
        "map",
        &format!("{n} maps active"),
        "Only the map your savegame uses loads. The others wait until you start a save on them."
            .into(),
        maps,
    ))
}

/// Where a map's height types come from: no `map.xml` reference means the base list; a reference
/// Silo can't follow (an unreadable or malformed `map.xml` or list), or a list that names nothing
/// (it may only set the channels), is unsure.
fn map_list(p: &Parsed, config: Option<&str>) -> heighttypes::MapList {
    use heighttypes::MapList;
    let read = |m: &str| read_member(&p.path, &p.kind, m);
    let Some(config) = config else {
        return MapList::Base;
    };
    let unsure = MapList::Unsure { channels: None };
    let Some(reference) = read(config).and_then(|xml| heighttypes::map_list_file(&xml)) else {
        return unsure;
    };
    let Some(file) = reference else {
        return MapList::Base;
    };
    let Some((names, channels)) = read(&file).and_then(|b| heighttypes::parse_list_checked(&b))
    else {
        return unsure;
    };
    if names.is_empty() {
        MapList::Unsure { channels }
    } else {
        MapList::Own {
            names,
            unsure: vec![],
            channels: channels.unwrap_or(heighttypes::DEFAULT_CHANNELS),
        }
    }
}

/// Either budget's candidate lists: `*heightType*.xml` or `*fillTypes*.xml`.
fn budget_candidate(member: &str) -> bool {
    heighttypes::is_candidate(member) || filltypes::is_candidate(member)
}

/// The ground-pile slot and fill-type budgets for the active set, as conflicts when they're tight
/// or over. Fill types come first: a height type for a fill type that won't exist takes no slot.
fn budget_conflicts(parsed: &[Parsed], game: GameLists) -> Vec<Conflict> {
    // One listing per mod serves both budgets.
    let per_mod: Vec<(Option<heighttypes::ModTypes>, Option<filltypes::ModFills>)> = parsed
        .par_iter()
        .filter(|p| !p.md.is_map)
        .map(|p| {
            let read = |m: &str| read_member(&p.path, &p.kind, m);
            let members = candidate_members(&p.path, &p.kind, budget_candidate);
            let pick =
                |keep: fn(&str) -> bool| members.iter().filter(|m| keep(m)).cloned().collect();
            (
                heighttypes::mod_types(&p.label, &p.tech_name, &p.md, &read, &|| {
                    pick(heighttypes::is_candidate)
                }),
                filltypes::mod_fills(&p.label, &p.tech_name, &p.md, &read, &|| {
                    pick(filltypes::is_candidate)
                }),
            )
        })
        .collect();
    let (heights, fills): (Vec<_>, Vec<_>) = per_mod.into_iter().unzip();
    let mut mod_heights: Vec<heighttypes::ModTypes> = heights.into_iter().flatten().collect();
    let mut mod_fills: Vec<filltypes::ModFills> = fills.into_iter().flatten().collect();
    // A mod's `mod_support/<dir>/` lists load with the mod `<dir>` names (#385). For an active
    // non-map mod that's every scenario; for a map, only its own (`worst` adds those). A non-map
    // mod matches by its exact tech name, as a script looks it up: its title could name a map
    // that isn't loaded.
    let active_techs: Vec<String> = parsed
        .iter()
        .filter(|p| !p.md.is_map)
        .map(|p| p.tech_name.clone())
        .collect();
    for m in &mut mod_heights {
        m.absorb_scoped(heighttypes::Loads::Mods(&active_techs));
    }
    for m in &mut mod_fills {
        m.absorb_scoped(heighttypes::Loads::Mods(&active_techs));
    }

    // One candidate per map: a mod can ship several, each with its own map.xml.
    let maps: Vec<(&Parsed, String, Option<&str>)> = parsed
        .iter()
        .filter(|p| p.md.is_map)
        .flat_map(|p| {
            let n = p.md.map_config_files.len();
            let configs: Vec<Option<&str>> = if n == 0 {
                vec![None]
            } else {
                p.md.map_config_files
                    .iter()
                    .map(|c| Some(c.as_str()))
                    .collect()
            };
            configs.into_iter().enumerate().map(move |(i, c)| {
                let label = if n > 1 {
                    format!("{} (map {})", p.label, i + 1)
                } else {
                    p.label.clone()
                };
                (p, label, c)
            })
        })
        .collect();
    let map_fills: Vec<filltypes::MapFills> = maps
        .iter()
        .map(|(p, label, config)| {
            let read = |m: &str| read_member(&p.path, &p.kind, m);
            let mut types: Vec<String> = config
                .and_then(read)
                .and_then(|xml| filltypes::map_fill_file(&xml).flatten())
                .and_then(|f| read(&f))
                .map(|b| fill_type_names(&b))
                .unwrap_or_default();
            let own = filltypes::mod_fills(&p.label, &p.tech_name, &p.md, &read, &Vec::new);
            for t in own.map(|f| f.exact).unwrap_or_default() {
                if !types.contains(&t) {
                    types.push(t);
                }
            }
            filltypes::MapFills {
                label: label.clone(),
                keys: heighttypes::map_keys(&p.tech_name, &p.label),
                types,
            }
        })
        .collect();
    let map_heights: Vec<heighttypes::MapTypes> = maps
        .iter()
        .map(|(p, label, config)| heighttypes::MapTypes {
            label: label.clone(),
            keys: heighttypes::map_keys(&p.tech_name, &p.label),
            list: map_list(p, *config),
            extra: heighttypes::mod_types(
                &p.label,
                &p.tech_name,
                &p.md,
                &|m| read_member(&p.path, &p.kind, m),
                &Vec::new,
            ),
        })
        .collect();

    let mut out = Vec::new();
    if let Some(fg) = game.fills {
        let budget = filltypes::worst(fg, &map_fills, &mod_fills);
        if let Some(severity) = filltypes::severity(&budget) {
            out.push(conflict(
                severity,
                "fillTypeCap",
                "Material types",
                filltypes::explain(&budget),
                budget
                    .mods
                    .iter()
                    .map(|(l, t, _, _)| (l.clone(), t.clone()))
                    .unzip(),
            ));
        }
    }
    // A height type stays exact only if its fill type certainly exists in that scenario: the
    // base list and mods' declared lists, plus a map's own for that map. When the game may run
    // out of fill types in that scenario it rejects some declared ones, so none of them (the
    // map's own included) can vouch. Without a readable fill list nothing is certain. Each map
    // scenario (base-game maps first) is judged on its own, then the worst is shown.
    let scenarios = std::iter::once(None).chain((0..maps.len()).map(Some));
    let budget = heighttypes::worst_of(scenarios.map(|i| {
        let map_fill = i.map(|i| &map_fills[i]);
        let fills_here = filltypes::mods_on(map_fill, &mod_fills);
        let overflow = game.fills.is_some_and(|fg| {
            filltypes::evaluate(fg, map_fill, &fills_here).total > filltypes::CAP
        });
        let known = game
            .fills
            .map(|fg| fg.known_exact(if overflow { &[] } else { &fills_here }))
            .unwrap_or_default();
        let map_here = i.map(|i| {
            let mut m = map_heights[i].clone();
            let mut with_map = known.clone();
            if !overflow {
                with_map.extend(map_fills[i].types.iter().cloned());
            }
            m.downgrade_unknown(&with_map);
            m
        });
        let mut mods_here = heighttypes::mods_on(map_here.as_ref(), &mod_heights).into_owned();
        for m in &mut mods_here {
            m.downgrade_unknown(&known);
        }
        heighttypes::evaluate(game.heights, map_here.as_ref(), &mods_here)
    }));
    if let Some(severity) = heighttypes::severity(&budget) {
        out.push(conflict(
            severity,
            "heightTypes",
            "Ground-pile slots",
            heighttypes::explain(&budget),
            budget
                .mods
                .iter()
                .map(|(l, t, _, _)| (l.clone(), t.clone()))
                .unzip(),
        ));
    }
    out
}

/// What the game itself brings, for the budget checks: the base lists and installed DLCs.
#[derive(Debug, Clone, Copy)]
pub struct GameLists<'a> {
    pub heights: &'a heighttypes::GameData,
    /// `None` when the install's fill-type list can't be read: no fill-type check then.
    pub fills: Option<&'a filltypes::GameData>,
}

impl GameLists<'static> {
    /// The lists for the game Silo detects, read once per run.
    pub fn cached() -> Self {
        GameLists {
            heights: heighttypes::GameData::cached(),
            fills: filltypes::GameData::cached(),
        }
    }
}

/// Detect conflicts within the given active set, without the budget checks (they need the
/// game's own lists; see [`detect_with`]).
pub fn detect(inputs: &[ConflictInput]) -> Vec<Conflict> {
    detect_with(inputs, None)
}

/// Detect conflicts within the given active set. With `game`, also check the ground-pile slot
/// and fill-type budgets against the base game and installed DLCs.
pub fn detect_with(inputs: &[ConflictInput], game: Option<GameLists>) -> Vec<Conflict> {
    let parsed: Vec<Parsed> = inputs
        .par_iter()
        .filter_map(|i| {
            let xml = scan::read_moddesc_xml(Path::new(&i.path), &i.kind).ok()?;
            let md = moddesc::parse(&xml);
            Some(Parsed {
                label: i.title.clone().unwrap_or_else(|| i.tech_name.clone()),
                tech_name: i.tech_name.clone(),
                author: md.author.clone(),
                path: i.path.clone(),
                kind: i.kind.clone(),
                md,
            })
        })
        .collect();

    // Collision maps: key -> indices into `parsed`.
    let mut registrations: HashMap<(String, String), Vec<usize>> = HashMap::new();
    let mut unique_types: HashMap<String, Vec<usize>> = HashMap::new();
    let mut scripts: HashMap<String, Vec<usize>> = HashMap::new();

    for (i, p) in parsed.iter().enumerate() {
        for r in
            p.md.registrations
                .iter()
                .filter(|r| clashes_globally(&r.kind))
        {
            registrations
                .entry((r.kind.clone(), r.name.clone()))
                .or_default()
                .push(i);
        }
        if let Some(ut) = &p.md.unique_type {
            unique_types.entry(ut.clone()).or_default().push(i);
        }
        for s in &p.md.scripts {
            let base = s.rsplit('/').next().unwrap_or(s).to_string();
            scripts.entry(base).or_default().push(i);
        }
    }

    let mut out = Vec::new();
    let note_same_author = " These mods share an author, so this is likely intentional.";

    for (ut, idxs) in unique_types {
        let n = participants(&parsed, &idxs);
        if n < 2 {
            continue;
        }
        let involved = labels(&parsed, &idxs);
        let sa = same_author(&parsed, &idxs);
        out.push(conflict(
            if sa { "warning" } else { "critical" },
            "uniqueType",
            &ut,
            format!(
                "These mods use the same exclusive slot (\"{ut}\"). The game loads only one mod per slot, so {} won't take effect.{}{}",
                the_others(n),
                map_condition(&parsed, &idxs),
                if sa { note_same_author } else { "" }
            ),
            involved,
        ));
    }

    for ((kind, name), idxs) in registrations {
        let n = participants(&parsed, &idxs);
        if n < 2 {
            continue;
        }
        let involved = labels(&parsed, &idxs);
        let sa = same_author(&parsed, &idxs);
        let kl = kind_label(&kind).to_string();
        out.push(conflict(
            if sa { "warning" } else { "critical" },
            &kind,
            &name,
            format!(
                "These mods each add the {kl} \"{name}\". Only one of them takes effect when the game loads, so {} may not work as intended.{}{}",
                the_others(n),
                map_condition(&parsed, &idxs),
                if sa { note_same_author } else { "" }
            ),
            involved,
        ));
    }

    // Same script file name: a note at most (#352). The game loads each mod's scripts by full
    // path into that mod's own environment, and other mods reach its classes through
    // `_G['<modName>']`, so two `main.lua` files don't replace each other.
    for (base, idxs) in scripts {
        if participants(&parsed, &idxs) < 2 {
            continue;
        }
        out.push(conflict(
            "info",
            "script",
            &base,
            format!(
                "These mods each load a script file named \"{base}\". The game keeps each mod's scripts apart, so the shared name alone doesn't make them clash."
            ),
            labels(&parsed, &idxs),
        ));
    }

    // More than one active map: a note (only the savegame's map loads), never a problem. A mod
    // either declares <maps> or not, so there are no false positives.
    let map_idxs: Vec<usize> = (0..parsed.len()).filter(|&i| parsed[i].md.is_map).collect();
    if let Some(c) = map_conflict(labels(&parsed, &map_idxs)) {
        out.push(c);
    }

    // Cross-mod fillType override: when two mods declare the same <fillType name>, the
    // later loader OVERWRITES the earlier one's properties in place (verified in
    // FillTypeManager.lua) — a silent, load-order-dependent clobber. Distinct names are
    // additive and DON'T conflict, so we flag only genuine same-name collisions.
    let mut fill_types: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, p) in parsed.iter().enumerate() {
        let Some(file) = &p.md.fill_types_file else {
            continue;
        };
        let Some(bytes) = read_member(&p.path, &p.kind, file) else {
            continue;
        };
        for name in fill_type_names(&bytes) {
            let v = fill_types.entry(name).or_default();
            if !v.contains(&i) {
                v.push(i);
            }
        }
    }
    for (name, idxs) in fill_types {
        let n = participants(&parsed, &idxs);
        if n < 2 {
            continue;
        }
        let involved = labels(&parsed, &idxs);
        let sa = same_author(&parsed, &idxs);
        out.push(conflict(
            if sa { "info" } else { "warning" },
            "fillType",
            &name,
            format!(
                "These mods each define the fill type \"{name}\". The game keeps only one definition: whichever loads last replaces {} settings (capacity, price and so on), so one mod may misbehave.{}{}",
                if n == 2 { "the other's" } else { "the others'" },
                map_condition(&parsed, &idxs),
                if sa { " They share an author, so this is likely coordinated." } else { "" }
            ),
            involved,
        ));
    }

    if let Some(g) = game {
        out.extend(budget_conflicts(&parsed, g));
    }

    // critical → warning → info, then by name.
    let rank = |s: &str| match s {
        "critical" => 0,
        "warning" => 1,
        _ => 2,
    };
    out.sort_by(|a, b| {
        rank(&a.severity)
            .cmp(&rank(&b.severity))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn maps(list: &[(&str, &str)]) -> Involved {
        (
            list.iter().map(|(l, _)| l.to_string()).collect(),
            list.iter().map(|(_, t)| t.to_string()).collect(),
        )
    }

    #[test]
    fn one_or_zero_maps_is_fine() {
        assert!(map_conflict(maps(&[])).is_none());
        assert!(map_conflict(maps(&[("Lone Star", "FS25_LoneStar")])).is_none());
    }

    #[test]
    fn two_plus_maps_is_only_a_note() {
        // FS25 loads only the savegame's map (in-game test 2026-10-18), so never critical.
        let c = map_conflict(maps(&[
            ("Lone Star Plains", "FS25_LoneStar"),
            ("Frankenmuth", "FS25_Frankenmuth"),
            ("Elmcreek", "FS25_Elmcreek"),
            ("Hof Bergmann", "FS25_HofBergmann"),
        ]))
        .unwrap();
        assert_eq!(c.severity, "info");
        assert_eq!(c.kind, "map");
        assert_eq!(c.mods.len(), 4);
        assert_eq!(c.tech_names[1], "FS25_Frankenmuth");
        assert_eq!(c.name, "4 maps active");
        assert!(c
            .explanation
            .contains("Only the map your savegame uses loads"));
        assert_eq!(
            c.id,
            "map:4 maps active:FS25_Elmcreek,FS25_Frankenmuth,FS25_HofBergmann,FS25_LoneStar"
        );
    }

    fn parsed(label: &str, tech: &str) -> Parsed {
        Parsed {
            label: label.into(),
            tech_name: tech.into(),
            author: None,
            path: String::new(),
            kind: "zip".into(),
            md: moddesc::ModDesc::default(),
        }
    }

    #[test]
    fn involved_mods_dedupe_by_tech_name_not_title() {
        // Two different mods with the same title are still two mods; one mod twice is one.
        let p = vec![
            parsed("Seeder", "FS25_SeederA"),
            parsed("Seeder", "FS25_SeederB"),
            parsed("Seeder", "FS25_SeederA"),
        ];
        let (mods, techs) = labels(&p, &[0, 1, 2]);
        assert_eq!(techs, vec!["FS25_SeederA", "FS25_SeederB"]);
        assert_eq!(mods, vec!["Seeder", "Seeder"]);
    }

    #[test]
    fn explanations_are_plain_words() {
        // Two unpacked mods by different authors that share a slot, a feature name and a
        // script name. What users read must not use modDesc jargon.
        let base = std::env::temp_dir().join(format!("silo_conflict_words_{}", std::process::id()));
        let mut inputs = Vec::new();
        for (i, author) in ["Ann", "Bob"].iter().enumerate() {
            let dir = base.join(format!("FS25_Mod{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("modDesc.xml"),
                format!(
                    r#"<modDesc descVersion="92"><author>{author}</author><uniqueType>seedSlot</uniqueType>
                    <actions><action name="MULCH_TOGGLE"/></actions>
                    <extraSourceFiles><sourceFile filename="scripts/util.lua"/></extraSourceFiles></modDesc>"#
                ),
            )
            .unwrap();
            std::fs::create_dir_all(dir.join("scripts")).unwrap();
            std::fs::write(dir.join("scripts/util.lua"), format!("-- {author}")).unwrap();
            inputs.push(ConflictInput {
                tech_name: format!("FS25_Mod{i}"),
                title: None,
                path: dir.to_string_lossy().into_owned(),
                kind: "dir".into(),
            });
        }
        let found = detect(&inputs);
        let _ = std::fs::remove_dir_all(&base);
        let kinds: Vec<&str> = found.iter().map(|c| c.kind.as_str()).collect();
        assert!(kinds.contains(&"uniqueType"), "{kinds:?}");
        assert!(kinds.contains(&"action"), "{kinds:?}");
        assert!(kinds.contains(&"script"), "{kinds:?}");
        // #352: a shared script name is only a note (each mod's scripts load apart).
        let script = found.iter().find(|c| c.kind == "script").unwrap();
        assert_eq!(script.severity, "info");
        for c in &found {
            for jargon in [
                "uniqueType",
                "extraSourceFiles",
                "fillType",
                "specialization",
                "FS ",
            ] {
                assert!(
                    !c.explanation.contains(jargon),
                    "{jargon} in {:?}",
                    c.explanation
                );
            }
            assert!(
                !c.explanation.contains('—'),
                "em dash in {:?}",
                c.explanation
            );
            assert_eq!(c.tech_names, vec!["FS25_Mod0", "FS25_Mod1"]);
        }
        // Two mods: "the other", never "the others".
        let slot = found.iter().find(|c| c.kind == "uniqueType").unwrap();
        assert!(
            slot.explanation.contains("so the other won't take effect"),
            "{:?}",
            slot.explanation
        );
        let action = found.iter().find(|c| c.kind == "action").unwrap();
        assert!(
            action.explanation.contains("so the other may not work"),
            "{:?}",
            action.explanation
        );
    }

    /// The game registers types and specializations as `<modName>.<name>`, so two mods by
    /// different authors using the same name both load: no conflict at all.
    #[test]
    fn same_type_or_specialization_name_in_two_mods_is_not_a_conflict() {
        let base = std::env::temp_dir().join(format!("silo_conflict_ns_{}", std::process::id()));
        let mut inputs = Vec::new();
        for (i, author) in ["Ann", "Bob"].iter().enumerate() {
            let dir = base.join(format!("FS25_Tractor{i}"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("modDesc.xml"),
                format!(
                    r#"<modDesc descVersion="92"><author>{author}</author>
                    <specializations><specialization name="mulcher" className="M" filename="m.lua"/></specializations>
                    <placeableSpecializations><specialization name="silo" className="S" filename="s.lua"/></placeableSpecializations>
                    <vehicleTypes><type name="myTractor" parent="baseDrivable" filename="t.lua"/></vehicleTypes>
                    <placeableTypes><type name="myShed" filename="p.lua"/></placeableTypes>
                    <handToolTypes><type name="myTool" filename="h.lua"/></handToolTypes>
                    <brands><brand name="ACME" title="Acme"/></brands></modDesc>"#
                ),
            )
            .unwrap();
            inputs.push(ConflictInput {
                tech_name: format!("FS25_Tractor{i}"),
                title: None,
                path: dir.to_string_lossy().into_owned(),
                kind: "dir".into(),
            });
        }
        let found = detect(&inputs);
        let _ = std::fs::remove_dir_all(&base);
        let kinds: Vec<&str> = found.iter().map(|c| c.kind.as_str()).collect();
        // The brand is one game-wide name, so it still clashes; nothing else does.
        assert_eq!(kinds, ["brand"], "{found:?}");
    }

    /// #392: a fill-type overflow on one map doesn't turn the ground-pile count into an estimate
    /// on a map where the fill types fit: here the base-game maps run out of ground-pile slots
    /// for certain, while only the mod map runs out of fill types.
    #[test]
    fn a_fill_overflow_on_one_map_leaves_other_maps_exact() {
        let base = std::env::temp_dir().join(format!("silo_conflict_ft2_{}", std::process::id()));
        let map = base.join("FS25_BigMap");
        let piles_mod = base.join("FS25_Piles");
        std::fs::create_dir_all(map.join("xml")).unwrap();
        std::fs::create_dir_all(piles_mod.join("xml")).unwrap();
        std::fs::write(
            map.join("modDesc.xml"),
            r#"<modDesc descVersion="92"><maps><map id="M"/></maps><fillTypes filename="xml/fills.xml"/></modDesc>"#,
        )
        .unwrap();
        let map_fills: String = (0..80)
            .map(|n| format!(r#"<fillType name="M{n}"/>"#))
            .collect();
        std::fs::write(
            map.join("xml/fills.xml"),
            format!("<map><fillTypes>{map_fills}</fillTypes></map>"),
        )
        .unwrap();
        std::fs::write(
            piles_mod.join("modDesc.xml"),
            r#"<modDesc descVersion="92"><fillTypes filename="xml/fills.xml"/><densityMapHeightTypes filename="xml/piles.xml"/></modDesc>"#,
        )
        .unwrap();
        let fills: String = (0..16)
            .map(|n| format!(r#"<fillType name="F{n}"/>"#))
            .collect();
        std::fs::write(
            piles_mod.join("xml/fills.xml"),
            format!("<map><fillTypes>{fills}</fillTypes></map>"),
        )
        .unwrap();
        let piles: String = (0..16)
            .map(|n| format!(r#"<densityMapHeightType fillTypeName="F{n}"/>"#))
            .collect();
        std::fs::write(
            piles_mod.join("xml/piles.xml"),
            format!("<map><densityMapHeightTypes>{piles}</densityMapHeightTypes></map>"),
        )
        .unwrap();
        let input = |dir: &std::path::Path, tech: &str| ConflictInput {
            tech_name: tech.into(),
            title: Some(tech.into()),
            path: dir.to_string_lossy().into_owned(),
            kind: "dir".into(),
        };
        let inputs = vec![input(&map, "FS25_BigMap"), input(&piles_mod, "FS25_Piles")];
        let heights = heighttypes::GameData {
            base: (0..48).map(|n| format!("BASE{n}")).collect(),
            base_channels: 6,
            ..Default::default()
        };
        let fill_base = filltypes::GameData {
            base: (0..178).map(|n| format!("BASE{n}")).collect(),
            ..Default::default()
        };
        let found = detect_with(
            &inputs,
            Some(GameLists {
                heights: &heights,
                fills: Some(&fill_base),
            }),
        );
        let _ = std::fs::remove_dir_all(&base);
        // On the mod map: 178 + 80 + 16 = 274 fill types.
        let fills = found
            .iter()
            .find(|c| c.kind == "fillTypeCap")
            .expect("fills");
        assert_eq!(fills.severity, "critical");
        // On the base-game maps 178 + 16 fill types fit, so 48 + 16 = 64 piles is certain.
        let piles = found
            .iter()
            .find(|c| c.kind == "heightTypes")
            .expect("piles");
        assert_eq!(piles.severity, "critical");
    }

    /// #341: when the game may run out of fill types, a mod's declared fill types may be the
    /// ones rejected, so they can't make its ground-pile types exact: a Review, not a Problem.
    #[test]
    fn a_fill_overflow_keeps_dependent_piles_an_estimate() {
        let base = std::env::temp_dir().join(format!("silo_conflict_ft_{}", std::process::id()));
        let dir = base.join("FS25_BigPack");
        std::fs::create_dir_all(dir.join("xml")).unwrap();
        std::fs::write(
            dir.join("modDesc.xml"),
            r#"<modDesc descVersion="92"><fillTypes filename="xml/fills.xml"/><densityMapHeightTypes filename="xml/piles.xml"/></modDesc>"#,
        )
        .unwrap();
        let fills: String = (0..80)
            .map(|n| format!(r#"<fillType name="F{n}"/>"#))
            .collect();
        std::fs::write(
            dir.join("xml/fills.xml"),
            format!("<map><fillTypes>{fills}</fillTypes></map>"),
        )
        .unwrap();
        let piles: String = (0..16)
            .map(|n| format!(r#"<densityMapHeightType fillTypeName="F{n}"/>"#))
            .collect();
        std::fs::write(
            dir.join("xml/piles.xml"),
            format!("<map><densityMapHeightTypes>{piles}</densityMapHeightTypes></map>"),
        )
        .unwrap();
        let inputs = vec![ConflictInput {
            tech_name: "FS25_BigPack".into(),
            title: Some("Big Pack".into()),
            path: dir.to_string_lossy().into_owned(),
            kind: "dir".into(),
        }];
        let heights = heighttypes::GameData {
            base: (0..48).map(|n| format!("BASE{n}")).collect(),
            base_channels: 6,
            ..Default::default()
        };
        let fill_base = filltypes::GameData {
            base: (0..178).map(|n| format!("BASE{n}")).collect(),
            ..Default::default()
        };
        let found = detect_with(
            &inputs,
            Some(GameLists {
                heights: &heights,
                fills: Some(&fill_base),
            }),
        );
        let _ = std::fs::remove_dir_all(&base);
        // 178 + 80 = 258 fill types: a Problem on its own.
        let fills = found
            .iter()
            .find(|c| c.kind == "fillTypeCap")
            .expect("fills");
        assert_eq!(fills.severity, "critical");
        // 48 + 16 = 64 piles, but their fill types may be the rejected ones.
        let piles = found
            .iter()
            .find(|c| c.kind == "heightTypes")
            .expect("piles");
        assert_eq!(piles.severity, "warning");
    }

    /// #385: a mod's `mod_support/<map>/fillTypes.xml` (Realistic Livestock's per-map lists)
    /// counts only when that map is active; with no map, or another one, it doesn't.
    #[test]
    fn map_specific_fill_lists_count_only_with_their_map() {
        let base = std::env::temp_dir().join(format!("silo_conflict_ms_{}", std::process::id()));
        let livestock = base.join("FS25_Livestock");
        let support = livestock.join("mod_support/FS25_HofBergmann/v1.4");
        std::fs::create_dir_all(&support).unwrap();
        std::fs::write(
            livestock.join("modDesc.xml"),
            r#"<modDesc descVersion="92"></modDesc>"#,
        )
        .unwrap();
        let fills: String = (0..80)
            .map(|n| format!(r#"<fillType name="RL{n}"/>"#))
            .collect();
        std::fs::write(
            support.join("fillTypes.xml"),
            format!("<map><fillTypes>{fills}</fillTypes></map>"),
        )
        .unwrap();
        let mut maps = Vec::new();
        for (tech, title) in [
            ("FS25_HofBergmann", "Hof Bergmann"),
            ("FS25_Valley", "Valley"),
        ] {
            let dir = base.join(tech);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("modDesc.xml"),
                r#"<modDesc descVersion="92"><maps><map id="M"/></maps></modDesc>"#,
            )
            .unwrap();
            maps.push(ConflictInput {
                tech_name: tech.into(),
                title: Some(title.into()),
                path: dir.to_string_lossy().into_owned(),
                kind: "dir".into(),
            });
        }
        let rl = ConflictInput {
            tech_name: "FS25_Livestock".into(),
            title: Some("Livestock".into()),
            path: livestock.to_string_lossy().into_owned(),
            kind: "dir".into(),
        };
        // A non-map mod titled like the map doesn't load the map's lists; one with its tech
        // name stands for that mod, so they count everywhere.
        let mut namesakes = Vec::new();
        for (tech, title) in [
            ("FS25_HofPack", "Hof Bergmann"),
            ("FS25_HofBergmann", "Pack"),
        ] {
            let dir = base.join("namesakes").join(tech);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(
                dir.join("modDesc.xml"),
                r#"<modDesc descVersion="92"></modDesc>"#,
            )
            .unwrap();
            namesakes.push(ConflictInput {
                tech_name: tech.into(),
                title: Some(title.into()),
                path: dir.to_string_lossy().into_owned(),
                kind: "dir".into(),
            });
        }
        let heights = heighttypes::GameData {
            base: (0..48).map(|n| format!("BASE{n}")).collect(),
            base_channels: 6,
            ..Default::default()
        };
        let fill_base = filltypes::GameData {
            base: (0..178).map(|n| format!("BASE{n}")).collect(),
            ..Default::default()
        };
        let game = GameLists {
            heights: &heights,
            fills: Some(&fill_base),
        };
        let fill_cap = |inputs: &[ConflictInput]| {
            detect_with(inputs, Some(game))
                .into_iter()
                .find(|c| c.kind == "fillTypeCap")
        };
        let alone = fill_cap(std::slice::from_ref(&rl));
        let other_map = fill_cap(&[rl.clone(), maps[1].clone()]);
        let its_map = fill_cap(&[rl.clone(), maps[0].clone()]);
        let by_title = fill_cap(&[rl.clone(), namesakes[0].clone()]);
        let by_tech = fill_cap(&[rl.clone(), namesakes[1].clone()]);
        let _ = std::fs::remove_dir_all(&base);
        assert!(alone.is_none(), "{alone:?}");
        assert!(other_map.is_none(), "{other_map:?}");
        assert!(by_title.is_none(), "{by_title:?}");
        let c = by_tech.expect("budget with the mod the folder names");
        assert_eq!(c.severity, "warning");
        assert!(
            c.explanation.contains("the base game uses"),
            "{}",
            c.explanation
        );
        // 178 + 80 = 258 on Hof Bergmann, from a script-loaded list: a Review.
        let c = its_map.expect("budget on Hof Bergmann");
        assert_eq!(c.severity, "warning");
        assert!(c.explanation.contains("Hof Bergmann"), "{}", c.explanation);
        assert_eq!(c.tech_names, ["FS25_Livestock"]);
    }

    fn parsed_map(label: &str, tech: &str) -> Parsed {
        let mut p = parsed(label, tech);
        p.md.is_map = true;
        p
    }

    /// #352: only the savegame's map loads, so the maps count as one participant.
    #[test]
    fn maps_count_as_one_participant() {
        let p = vec![
            parsed_map("Map A", "FS25_MapA"),
            parsed_map("Map B", "FS25_MapB"),
            parsed("Tractor", "FS25_Tractor"),
            parsed("Seeder", "FS25_Seeder"),
            parsed_map("Map A", "FS25_MapA"),
        ];
        // Maps only: they never load together.
        assert_eq!(participants(&p, &[0, 1]), 1);
        assert_eq!(participants(&p, &[0, 4]), 1);
        // One map and one mod: a real clash with whichever map loads.
        assert_eq!(participants(&p, &[0, 2]), 2);
        // Two maps and one mod: still just two in any one game.
        assert_eq!(participants(&p, &[0, 1, 2]), 2);
        // Two mods (and a map): three.
        assert_eq!(participants(&p, &[0, 2, 3]), 3);
        assert_eq!(participants(&p, &[2, 3]), 2);
        assert_eq!(participants(&p, &[2, 2]), 1);
        assert_eq!(participants(&p, &[]), 0);
        // One tech name both as a map and not (two copies of a mod) is still one mod.
        let twin = vec![
            parsed_map("Map A", "FS25_MapA"),
            parsed("Map A", "FS25_MapA"),
        ];
        assert_eq!(participants(&twin, &[0, 1]), 1);
        assert_eq!(participants(&twin, &[1, 0]), 1);
        // Only collisions involving a map say when they matter.
        assert_eq!(map_condition(&p, &[2, 3]), "");
        assert!(map_condition(&p, &[0, 2, 4]).contains("play on that map"));
        assert!(map_condition(&p, &[0, 1, 2]).contains("one of these maps"));
        // Two non-map mods clash on any map; a map only joins in.
        assert_eq!(
            map_condition(&p, &[0, 2, 3]),
            " The map takes part only when your savegame is on it."
        );
        assert!(map_condition(&p, &[0, 1, 2, 3]).starts_with(" Each map takes part"));
    }

    /// Write an unpacked mod for `detect` with `body` inside `<modDesc>`, plus a fill-type list
    /// naming `fills` and a `scripts/main.lua` unique to the mod.
    fn write_mod(base: &Path, tech: &str, body: &str, fills: &[&str]) -> ConflictInput {
        let dir = base.join(tech);
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(
            dir.join("modDesc.xml"),
            format!(
                r#"<modDesc descVersion="92"><author>{tech}</author>{body}
                <fillTypes filename="fills.xml"/>
                <extraSourceFiles><sourceFile filename="scripts/main.lua"/></extraSourceFiles></modDesc>"#
            ),
        )
        .unwrap();
        let entries: String = fills
            .iter()
            .map(|f| format!(r#"<fillType name="{f}"/>"#))
            .collect();
        std::fs::write(
            dir.join("fills.xml"),
            format!("<map><fillTypes>{entries}</fillTypes></map>"),
        )
        .unwrap();
        std::fs::write(dir.join("scripts/main.lua"), format!("-- {tech}")).unwrap();
        ConflictInput {
            tech_name: tech.into(),
            title: None,
            path: dir.to_string_lossy().into_owned(),
            kind: "dir".into(),
        }
    }

    /// #352: two maps by different authors that add the same brand, shop category, action,
    /// exclusive slot, fill type and script name never load together, so only the
    /// "maps active" note shows. A non-map mod that adds the same things does clash, with
    /// whichever map loads.
    #[test]
    fn maps_never_clash_with_each_other() {
        let base = std::env::temp_dir().join(format!("silo_conflict_maps_{}", std::process::id()));
        let shared = r#"<uniqueType>terrainSlot</uniqueType>
            <brands><brand name="ACME" title="Acme"/></brands>
            <storeCategories><storeCategory name="ACME_SHOP" title="Acme"/></storeCategories>
            <actions><action name="MAP_TOGGLE"/></actions>"#;
        let map = format!(r#"<maps><map id="M" configFilename="map.xml"/></maps>{shared}"#);
        let maps = vec![
            write_mod(&base, "FS25_MapA", &map, &["ROCKS"]),
            write_mod(&base, "FS25_MapB", &map, &["ROCKS"]),
        ];
        let only_maps = detect(&maps);

        let mut with_mod = maps.clone();
        with_mod.push(write_mod(
            &base,
            "FS25_AcmePack",
            r#"<brands><brand name="ACME" title="Acme"/></brands>"#,
            &["ROCKS"],
        ));
        let mixed = detect(&with_mod);
        // A second pack: the two packs clash on any map, the maps only join in.
        let mut two_packs = with_mod.clone();
        two_packs.push(write_mod(
            &base,
            "FS25_AcmeToo",
            r#"<brands><brand name="ACME" title="Acme"/></brands>"#,
            &["ROCKS"],
        ));
        let both = detect(&two_packs);
        let _ = std::fs::remove_dir_all(&base);

        let kinds: Vec<&str> = only_maps.iter().map(|c| c.kind.as_str()).collect();
        assert_eq!(kinds, ["map"], "{only_maps:?}");
        assert_eq!(only_maps[0].name, "2 maps active");

        // The pack clashes with whichever map loads: one other, not two.
        let brand = mixed.iter().find(|c| c.kind == "brand").expect("brand");
        assert_eq!(brand.severity, "critical");
        assert_eq!(brand.tech_names.len(), 3);
        assert!(
            brand.explanation.contains("so the other may not work"),
            "{}",
            brand.explanation
        );
        // It only matters on a save on one of those maps.
        assert!(
            brand
                .explanation
                .contains("matters only when you play on one of these maps"),
            "{}",
            brand.explanation
        );
        let fill = mixed.iter().find(|c| c.kind == "fillType").expect("fill");
        assert_eq!(fill.severity, "warning");
        assert!(
            fill.explanation.contains("replaces the other's settings"),
            "{}",
            fill.explanation
        );
        // The pack shares no shop category, action or slot with the maps.
        for kind in ["storeCategory", "action", "uniqueType"] {
            assert!(mixed.iter().all(|c| c.kind != kind), "{kind}: {mixed:?}");
        }
        let script = mixed.iter().find(|c| c.kind == "script").expect("script");
        assert_eq!(script.severity, "info");
        assert!(mixed.iter().any(|c| c.kind == "map"));

        for kind in ["brand", "fillType"] {
            let c = both.iter().find(|c| c.kind == kind).expect(kind);
            assert_eq!(c.tech_names.len(), 4);
            assert!(c.explanation.contains("the others"), "{}", c.explanation);
            assert!(!c.explanation.contains("matters only"), "{}", c.explanation);
            assert!(
                c.explanation.contains("Each map takes part only when"),
                "{}",
                c.explanation
            );
        }
    }

    /// #352: the game loads each mod's scripts by full path into that mod's own environment
    /// (other mods reach its classes through `_G['<modName>']`), so two different `main.lua`
    /// files by different authors are a note, never something to review.
    #[test]
    fn same_script_name_is_only_a_note() {
        let base = std::env::temp_dir().join(format!("silo_conflict_lua_{}", std::process::id()));
        let inputs = vec![
            write_mod(&base, "FS25_One", "", &[]),
            write_mod(&base, "FS25_Two", "", &[]),
        ];
        let found = detect(&inputs);
        let _ = std::fs::remove_dir_all(&base);
        assert_eq!(found.len(), 1, "{found:?}");
        let c = &found[0];
        assert_eq!((c.kind.as_str(), c.name.as_str()), ("script", "main.lua"));
        assert_eq!(c.severity, "info");
        assert!(
            c.explanation.contains("doesn't make them clash"),
            "{}",
            c.explanation
        );
    }

    #[test]
    fn conflict_id_ignores_mod_order() {
        let a = conflict_id("script", "x.lua", &["B".into(), "A".into()]);
        let b = conflict_id("script", "x.lua", &["A".into(), "B".into()]);
        assert_eq!(a, b);
        assert_eq!(a, "script:x.lua:A,B");
        assert_ne!(
            a,
            conflict_id("fillType", "x.lua", &["A".into(), "B".into()])
        );
    }

    #[test]
    fn fill_type_names_upper_and_dedup() {
        let xml = br#"<map><fillTypes>
            <fillType name="Limestone" title="Limestone"/>
            <fillType name="COMPOST"/>
            <fillType name="limestone"/>
        </fillTypes></map>"#;
        let names = fill_type_names(xml);
        // Case-folded to match the game's registry key, and de-duplicated.
        assert!(names.contains(&"LIMESTONE".to_string()));
        assert!(names.contains(&"COMPOST".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "LIMESTONE").count(), 1);
    }

    /// #339 / #341: the ground-pile budget runs over the active set, only when the game's lists
    /// are known, names the mods that use the slots, and counts entries for fill types Silo can't
    /// find as estimates. Unpacked mods are searched at any depth.
    #[test]
    fn ground_pile_budget_names_the_mods_using_slots() {
        let base = std::env::temp_dir().join(format!("silo_conflict_ht_{}", std::process::id()));
        let mut inputs = Vec::new();
        // One mod declares its piles in modDesc, one ships them for a script to load.
        for (i, declared) in [true, false].iter().enumerate() {
            let dir = base.join(format!("FS25_Piles{i}"));
            // The script-loaded list sits deep, as some unpacked dev mods have it.
            let sub = if *declared { "xml" } else { "a/b/c/d/e/f/g" };
            std::fs::create_dir_all(dir.join(sub)).unwrap();
            let decl = if *declared {
                r#"<densityMapHeightTypes filename="xml/types.xml"/>"#
            } else {
                ""
            };
            std::fs::write(
                dir.join("modDesc.xml"),
                format!(r#"<modDesc descVersion="92"><author>A{i}</author>{decl}</modDesc>"#),
            )
            .unwrap();
            let entries: String = (0..10)
                .map(|n| format!(r#"<densityMapHeightType fillTypeName="P{i}_{n}"/>"#))
                .collect();
            let file = if *declared {
                "types.xml"
            } else {
                "densityMapHeightTypes.xml"
            };
            std::fs::write(
                dir.join(sub).join(file),
                format!("<map><densityMapHeightTypes>{entries}</densityMapHeightTypes></map>"),
            )
            .unwrap();
            inputs.push(ConflictInput {
                tech_name: format!("FS25_Piles{i}"),
                title: Some(format!("Piles {i}")),
                path: dir.to_string_lossy().into_owned(),
                kind: "dir".into(),
            });
        }
        let game = heighttypes::GameData {
            base: (0..48).map(|n| format!("BASE{n}")).collect(),
            base_channels: 6,
            foliage: vec![],
            dlc: vec![],
            at_risk: vec![],
        };
        // Every pile's fill type exists except one of the declared mod's.
        let fills = filltypes::GameData {
            base: (0..9)
                .map(|n| format!("P0_{n}"))
                .chain((0..10).map(|n| format!("P1_{n}")))
                .collect(),
            ..Default::default()
        };
        let found = detect_with(
            &inputs,
            Some(GameLists {
                heights: &game,
                fills: Some(&fills),
            }),
        );
        let without = detect(&inputs);
        let _ = std::fs::remove_dir_all(&base);

        // 48 + 9 exact = 57 fits; the unknown one and the script mod's 10 push it to 68: a
        // warning, never a Problem.
        let c = found
            .iter()
            .find(|c| c.kind == "heightTypes")
            .expect("budget");
        assert_eq!(c.severity, "warning");
        assert!(c.explanation.contains("68 in all"), "{}", c.explanation);
        assert_eq!(c.tech_names, vec!["FS25_Piles0", "FS25_Piles1"]);
        assert!(without.iter().all(|c| c.kind != "heightTypes"));
    }

    #[test]
    fn empty_fill_types_is_safe() {
        assert!(fill_type_names(b"<map></map>").is_empty());
        assert!(fill_type_names(b"not xml").is_empty());
    }
}
