//! Conflict detection over an active set of mods — the thing no other FS tool does
//! well. We re-parse each active mod's `modDesc.xml` and flag collisions across the
//! GIANTS namespaces that actually clash at load time:
//!
//! * `<uniqueType>` — GIANTS' own primitive: only one mod of a type loads (critical)
//! * input actions, brands and shop categories registered under the same name (critical)
//! * `<extraSourceFiles>` global Lua with the same basename — they can overwrite
//!   each other (warning)
//! * more than one active map (info only: FS25 loads just the savegame's map)
//!
//! Signal refinement: a clash among mods by the **same author** is usually
//! intentional (shared packs/utilities), and two mods shipping the **byte-identical
//! script** aren't really fighting — both are down-ranked (critical→warning,
//! warning→info) with a note, so the critical list stays trustworthy.
//!
//! Not flagged: the six specialization / type surfaces (`<specializations>`, `<vehicleTypes>`
//! and their placeable / hand-tool twins). The game registers a mod's types as
//! `<modName>.<type>` (`TypeManager.lua` in gameSource, `typeName = modName.."."..typeName`)
//! and resolves a type's specializations with a `<modName>.<spec>` fallback, so two mods using
//! the same name both load. Flagging them made every common name ("myTractor") a critical.
//!
//! Runs on demand for the active set only, so it stays cheap.

use crate::moddesc;
use crate::scan;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
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
        std::fs::read(Path::new(path).join(&member)).ok()
    }
}

fn hash_bytes(b: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    b.hash(&mut h);
    h.finish()
}

/// True if every involved mod ships a byte-identical file for `basename`.
fn identical_script(parsed: &[Parsed], idxs: &[usize], basename: &str) -> bool {
    let mut prev: Option<u64> = None;
    for &i in idxs {
        let p = &parsed[i];
        let full =
            p.md.scripts
                .iter()
                .find(|s| s.rsplit('/').next().unwrap_or(s.as_str()) == basename);
        let Some(full) = full else { return false };
        let Some(bytes) = read_member(&p.path, &p.kind, full) else {
            return false;
        };
        let h = hash_bytes(&bytes);
        match prev {
            Some(prev) if prev != h => return false,
            _ => prev = Some(h),
        }
    }
    prev.is_some()
}

/// Extract declared `<fillType name>` values (UPPER-cased, as the game keys them) from
/// a mod's fillTypes XML. Pure — unit-testable. The registry match is case-insensitive
/// (FillTypeManager string.upper's the name), so we normalize here too.
fn fill_type_names(xml: &[u8]) -> Vec<String> {
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

/// Detect conflicts within the given active set.
pub fn detect(inputs: &[ConflictInput]) -> Vec<Conflict> {
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
        let involved = labels(&parsed, &idxs);
        if involved.0.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        out.push(conflict(
            if sa { "warning" } else { "critical" },
            "uniqueType",
            &ut,
            format!(
                "These mods use the same exclusive slot (\"{ut}\"). The game loads only one mod per slot, so {} won't take effect.{}",
                if involved.0.len() == 2 { "the other" } else { "the others" },
                if sa { note_same_author } else { "" }
            ),
            involved,
        ));
    }

    for ((kind, name), idxs) in registrations {
        let involved = labels(&parsed, &idxs);
        if involved.0.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        let kl = kind_label(&kind).to_string();
        out.push(conflict(
            if sa { "warning" } else { "critical" },
            &kind,
            &name,
            format!(
                "These mods each add the {kl} \"{name}\". Only one of them takes effect when the game loads, so {} may not work as intended.{}",
                if involved.0.len() == 2 { "the other" } else { "the others" },
                if sa { note_same_author } else { "" }
            ),
            involved,
        ));
    }

    for (base, idxs) in scripts {
        let involved = labels(&parsed, &idxs);
        if involved.0.len() < 2 {
            continue;
        }
        let identical = identical_script(&parsed, &idxs, &base);
        let sa = same_author(&parsed, &idxs);
        let (severity, extra) = if identical {
            (
                "info",
                " The file is the same in each, so it's shared code, not a real clash.",
            )
        } else if sa {
            ("info", note_same_author)
        } else {
            ("warning", "")
        };
        out.push(conflict(
            severity,
            "script",
            &base,
            format!(
                "These mods each load a script file named \"{base}\". Depending on which loads last, one may replace the other.{extra}"
            ),
            involved,
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
        let involved = labels(&parsed, &idxs);
        if involved.0.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        out.push(conflict(
            if sa { "info" } else { "warning" },
            "fillType",
            &name,
            format!(
                "These mods each define the fill type \"{name}\". The game keeps only one definition: whichever loads last replaces {} settings (capacity, price and so on), so one mod may misbehave.{}",
                if involved.0.len() == 2 { "the other's" } else { "the others'" },
                if sa { " They share an author, so this is likely coordinated." } else { "" }
            ),
            involved,
        ));
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

    #[test]
    fn empty_fill_types_is_safe() {
        assert!(fill_type_names(b"<map></map>").is_empty());
        assert!(fill_type_names(b"not xml").is_empty());
    }
}
