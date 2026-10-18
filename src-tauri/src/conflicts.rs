//! Conflict detection over an active set of mods — the thing no other FS tool does
//! well. We re-parse each active mod's `modDesc.xml` and flag collisions across the
//! GIANTS namespaces that actually clash at load time:
//!
//! * `<uniqueType>` — GIANTS' own primitive: only one mod of a type loads (critical)
//! * the six specialization/type registration surfaces — same name, same surface,
//!   means one silently wins (critical)
//! * `<extraSourceFiles>` global Lua with the same basename — they can overwrite
//!   each other (warning)
//!
//! Signal refinement: a clash among mods by the **same author** is usually
//! intentional (shared packs/utilities), and two mods shipping the **byte-identical
//! script** aren't really fighting — both are down-ranked (critical→warning,
//! warning→info) with a note, so the critical list stays trustworthy.
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
    pub severity: String, // "critical" | "warning" | "info"
    pub kind: String,     // "uniqueType" | "specialization" | "vehicleType" | "script" | …
    pub name: String,
    pub explanation: String,
    pub mods: Vec<String>, // display labels of the mods involved
}

struct Parsed {
    label: String,
    author: Option<String>,
    path: String,
    kind: String,
    md: moddesc::ModDesc,
}

fn kind_label(kind: &str) -> &str {
    match kind {
        "specialization" => "specialization",
        "placeableSpecialization" => "placeable specialization",
        "handToolSpecialization" => "hand-tool specialization",
        "vehicleType" => "vehicle type",
        "placeableType" => "placeable type",
        "handToolType" => "hand-tool type",
        "action" => "input action",
        "brand" => "brand",
        "storeCategory" => "store category",
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

fn labels(parsed: &[Parsed], idxs: &[usize]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for &i in idxs {
        if !out.contains(&parsed[i].label) {
            out.push(parsed[i].label.clone());
        }
    }
    out
}

/// Flag when more than one map is active (a reliable instant-crash cause). Pure over
/// the active maps' labels, so it's testable without fixture mods.
fn map_conflict(map_labels: Vec<String>) -> Option<Conflict> {
    if map_labels.len() < 2 {
        return None;
    }
    Some(Conflict {
        severity: "critical".into(),
        kind: "map".into(),
        name: format!("{} maps enabled", map_labels.len()),
        explanation:
            "More than one map is active. FS loads a second map's scripts even when it isn't the one you're playing, which typically crashes on load. Keep exactly one map enabled."
                .into(),
        mods: map_labels,
    })
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
        for r in &p.md.registrations {
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
        let labels = labels(&parsed, &idxs);
        if labels.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        out.push(Conflict {
            severity: if sa { "warning" } else { "critical" }.into(),
            kind: "uniqueType".into(),
            name: ut.clone(),
            explanation: format!(
                "These mods share the uniqueType \"{ut}\". FS loads only one mod of a given uniqueType, so the others will not take effect.{}",
                if sa { note_same_author } else { "" }
            ),
            mods: labels,
        });
    }

    for ((kind, name), idxs) in registrations {
        let labels = labels(&parsed, &idxs);
        if labels.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        let kl = kind_label(&kind).to_string();
        out.push(Conflict {
            severity: if sa { "warning" } else { "critical" }.into(),
            kind,
            name: name.clone(),
            explanation: format!(
                "Multiple mods register the {kl} \"{name}\". Only one registration wins at load. The others' behavior may break.{}",
                if sa { note_same_author } else { "" }
            ),
            mods: labels,
        });
    }

    for (base, idxs) in scripts {
        let labels = labels(&parsed, &idxs);
        if labels.len() < 2 {
            continue;
        }
        let identical = identical_script(&parsed, &idxs, &base);
        let sa = same_author(&parsed, &idxs);
        let (severity, extra) = if identical {
            (
                "info",
                " The file is byte-identical in each, so it's a shared library, not a real clash.",
            )
        } else if sa {
            ("info", note_same_author)
        } else {
            ("warning", "")
        };
        out.push(Conflict {
            severity: severity.into(),
            kind: "script".into(),
            name: base.clone(),
            explanation: format!(
                "These mods each inject a global Lua script named \"{base}\" via extraSourceFiles. They may override one another depending on load order.{extra}"
            ),
            mods: labels,
        });
    }

    // Two active maps = instant crash on load: a second map's scripts still load even
    // if unused. Canonical FS folklore ("never have more than one map enabled"); no
    // manager surfaces it. Zero false positives — a mod either declares <maps> or not.
    let map_labels: Vec<String> = parsed
        .iter()
        .filter(|p| p.md.is_map)
        .map(|p| p.label.clone())
        .collect();
    if let Some(c) = map_conflict(map_labels) {
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
        let labels = labels(&parsed, &idxs);
        if labels.len() < 2 {
            continue;
        }
        let sa = same_author(&parsed, &idxs);
        out.push(Conflict {
            severity: if sa { "info" } else { "warning" }.into(),
            kind: "fillType".into(),
            name: name.clone(),
            explanation: format!(
                "These mods each define the fillType \"{name}\". FS keeps only one definition. Whichever loads last silently overrides the others' settings (capacity, price, etc.), so one mod may misbehave.{}",
                if sa { " Same author, so likely coordinated." } else { "" }
            ),
            mods: labels,
        });
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

    #[test]
    fn one_or_zero_maps_is_fine() {
        assert!(map_conflict(vec![]).is_none());
        assert!(map_conflict(vec!["FS25_LoneStar".into()]).is_none());
    }

    #[test]
    fn two_plus_maps_is_critical() {
        let c = map_conflict(vec!["Lone Star Plains".into(), "Frankenmuth".into()]).unwrap();
        assert_eq!(c.severity, "critical");
        assert_eq!(c.kind, "map");
        assert_eq!(c.mods.len(), 2);
        assert!(c.name.contains('2'));
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
