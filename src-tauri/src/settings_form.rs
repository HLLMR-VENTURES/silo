//! Turn a mod's settings XML into an editable form and write edits back safely.
//!
//! Handles the two FS settings conventions seen in the wild: the short form
//! (`boolean`/`integer`/`float`/`string`, or `value`) e.g. `<setting name="FOO" boolean="true"/>`,
//! and the `*Value` form real mods like Easy Dev Controls use, e.g.
//! `<setting name="runSpeedIndex" intValue="3" isSaved="true"/>`. Each such value attribute
//! becomes a form field; the element's `name` attribute (or tag) is the label. Metadata attrs
//! (`isSaved`, …) are preserved verbatim, and anything the form can't model stays editable via
//! the raw-XML escape hatch in the UI.
//!
//! Field ids are assigned in a single deterministic document-order walk, and the
//! exact same walk drives write-back — so a field always maps to the same value,
//! and only edited values change (the rest of the file is re-emitted as read).

use quick_xml::events::{BytesStart, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Field {
    pub id: usize,
    pub label: String,
    pub kind: String, // "bool" | "int" | "float" | "string"
    pub value: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsFile {
    pub path: String,
    pub name: String,
    pub fields: Vec<Field>,
    pub raw: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub id: usize,
    pub value: String,
}

struct Slot {
    attr: String,
    kind: String,
    value: String,
    label: String,
}

fn infer_kind(v: &str) -> &'static str {
    let t = v.trim();
    if t == "true" || t == "false" {
        "bool"
    } else if t.parse::<i64>().is_ok() {
        "int"
    } else if t.parse::<f64>().is_ok() {
        "float"
    } else {
        "string"
    }
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

/// The editable attribute "slots" of one element, in attribute order.
fn element_slots(e: &BytesStart) -> Vec<Slot> {
    let mut name_label: Option<String> = None;
    let mut raw: Vec<(String, String)> = Vec::new();
    for a in e.attributes().flatten() {
        let key = local(a.key.as_ref());
        // A value that can't be resolved (an undeclared entity) shows as written, not blank.
        let val = crate::xmltext::attr_value(&a)
            .unwrap_or_else(|| String::from_utf8_lossy(&a.value).into_owned());
        if key == "name" {
            name_label = Some(val.clone());
        }
        raw.push((key, val));
    }
    let base = name_label.unwrap_or_else(|| local(e.name().as_ref()));

    let mut slots: Vec<Slot> = Vec::new();
    for (key, val) in &raw {
        // Two attribute conventions coexist in the wild. The short form
        // (`boolean`/`integer`/`float`/`string`/`value`) and the `*Value` form that real mods
        // like Easy Dev Controls use (`boolValue="true"`, `intValue="3"`). Recognize both, or
        // those mods' settings come up as an empty form. Metadata attrs (name, isSaved, …) are
        // deliberately NOT slots — they're preserved verbatim on write.
        let kind = match key.as_str() {
            "boolean" | "boolValue" => Some("bool"),
            "integer" | "intValue" => Some("int"),
            "float" | "floatValue" => Some("float"),
            "string" | "stringValue" => Some("string"),
            "value" => Some(infer_kind(val)),
            _ => None,
        };
        if let Some(kind) = kind {
            slots.push(Slot {
                attr: key.clone(),
                kind: kind.to_string(),
                value: val.clone(),
                label: base.clone(),
            });
        }
    }
    // Disambiguate labels when an element holds more than one field.
    if slots.len() > 1 {
        for s in &mut slots {
            s.label = format!("{} · {}", s.label, s.attr);
        }
    }
    slots
}

/// Parse settings XML into form fields.
pub fn parse(xml: &str) -> Vec<Field> {
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut fields = Vec::new();
    let mut id = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                for s in element_slots(&e) {
                    fields.push(Field {
                        id,
                        label: s.label,
                        kind: s.kind,
                        value: s.value,
                    });
                    id += 1;
                }
            }
            _ => {}
        }
        buf.clear();
    }
    fields
}

/// `e` with the edits for its slots (field ids `base_id..`) applied. Only edited values
/// change; every other attribute keeps its raw bytes (#354). Records each edit id it used.
fn edited_start(
    e: &BytesStart,
    base_id: usize,
    edits: &[Edit],
    applied: &mut Vec<usize>,
) -> Result<BytesStart<'static>, String> {
    // Which attrs are slots, in order, so we can map to ids.
    let slots = element_slots(e);
    let mut slot_i = 0usize;
    crate::xmltext::rebuild_attributes(e, |a| {
        let key = local(a.key.as_ref());
        if slots.get(slot_i).map(|s| s.attr != key).unwrap_or(true) {
            return None;
        }
        let this_id = base_id + slot_i;
        slot_i += 1;
        let ed = edits.iter().find(|ed| ed.id == this_id)?;
        applied.push(this_id);
        Some(ed.value.clone())
    })
}

/// The element as it should be written: rebuilt when one of its fields (ids `id..id + n`)
/// is edited, otherwise exactly as read.
fn element_out<'a>(
    e: BytesStart<'a>,
    id: usize,
    n: usize,
    edits: &[Edit],
    applied: &mut Vec<usize>,
) -> Result<BytesStart<'a>, String> {
    if edits.iter().any(|ed| (id..id + n).contains(&ed.id)) {
        edited_start(&e, id, edits, applied)
    } else {
        Ok(e)
    }
}

/// Apply edits and return the new XML (values changed, structure preserved). An element
/// with no edited field is written exactly as read. An edit whose field isn't in the file
/// is an error, so a save never reports success for a value it didn't write.
pub fn apply_edits(xml: &str, edits: &[Edit]) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut id = 0usize;
    let mut applied: Vec<usize> = Vec::new();
    let mut depth = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) if depth == 0 => break,
            Ok(Event::Eof) => {
                return Err(crate::xmltext::damaged(
                    "it ends before every tag is closed",
                ))
            }
            Err(e) => return Err(crate::xmltext::damaged(e)),
            Ok(Event::Start(e)) => {
                depth += 1;
                let n = element_slots(&e).len();
                let out = element_out(e, id, n, edits, &mut applied)?;
                id += n;
                writer
                    .write_event(Event::Start(out))
                    .map_err(|e| e.to_string())?;
            }
            Ok(ev @ Event::End(_)) => {
                depth = depth.saturating_sub(1);
                writer.write_event(ev).map_err(|e| e.to_string())?;
            }
            Ok(Event::Empty(e)) => {
                let n = element_slots(&e).len();
                let out = element_out(e, id, n, edits, &mut applied)?;
                id += n;
                writer
                    .write_event(Event::Empty(out))
                    .map_err(|e| e.to_string())?;
            }
            Ok(ev) => writer.write_event(ev).map_err(|e| e.to_string())?,
        }
        buf.clear();
    }
    if edits.iter().any(|ed| !applied.contains(&ed.id)) {
        return Err("The settings file has changed since Silo read it, so nothing was saved. Close and reopen the settings, then try again.".into());
    }
    let bytes = writer.into_inner().into_inner();
    let mut s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    // Preserve a leading UTF-8 BOM if the original had one.
    if xml.starts_with('\u{feff}') && !s.starts_with('\u{feff}') {
        s.insert(0, '\u{feff}');
    }
    Ok(s)
}

/// Find a mod's settings files under `modSettings/<modName>/`.
pub fn find_files(user_dir: &Path, mod_name: &str) -> Vec<PathBuf> {
    let dir = user_dir.join("modSettings").join(mod_name);
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            let p = e.path();
            if p.extension()
                .map(|x| x.eq_ignore_ascii_case("xml"))
                .unwrap_or(false)
            {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Names of mods that currently have a settings folder with XML in it.
pub fn mods_with_settings(user_dir: &Path) -> Vec<String> {
    let dir = user_dir.join("modSettings");
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&dir) {
        for e in rd.flatten() {
            if e.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = e.file_name().to_string_lossy().into_owned();
                if !find_files(user_dir, &name).is_empty() {
                    out.push(name);
                }
            }
        }
    }
    out
}

pub fn load_file(path: &Path) -> Result<SettingsFile, String> {
    let raw = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(SettingsFile {
        path: path.to_string_lossy().into_owned(),
        name: path
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default(),
        fields: parse(&raw),
        raw,
    })
}
// NOTE: writing settings back is done via `paths::guarded_xml_write` at the command layer
// (containment under the FS25 user dir + required backup + atomic swap). `apply_edits` above
// stays the pure edit-rendering step.

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "<settings>\n    <setting name=\"MAX\" integer=\"1\"/>\n    <setting name=\"ON\" boolean=\"true\"/>\n</settings>";

    #[test]
    fn parses_typed_fields() {
        let f = parse(SAMPLE);
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].label, "MAX");
        assert_eq!(f[0].kind, "int");
        assert_eq!(f[0].value, "1");
        assert_eq!(f[1].kind, "bool");
        assert_eq!(f[1].value, "true");
    }

    // Real mods (Easy Dev Controls) use the `*Value` convention with metadata attrs alongside.
    const EDC: &str = "<settings>\n    <setting name=\"runSpeedIndex\" intValue=\"3\" isSaved=\"true\"/>\n    <setting name=\"superStrength\" boolValue=\"false\" isSaved=\"false\"/>\n</settings>";

    #[test]
    fn parses_the_star_value_convention() {
        let f = parse(EDC);
        assert_eq!(f.len(), 2, "both settings should become fields");
        assert_eq!(f[0].label, "runSpeedIndex");
        assert_eq!(f[0].kind, "int");
        assert_eq!(f[0].value, "3");
        assert_eq!(f[1].label, "superStrength");
        assert_eq!(f[1].kind, "bool");
        assert_eq!(f[1].value, "false");
    }

    #[test]
    fn edits_star_value_and_preserves_metadata() {
        let edits = vec![Edit {
            id: 0,
            value: "8".into(),
        }];
        let out = apply_edits(EDC, &edits).unwrap();
        assert!(out.contains("intValue=\"8\""));
        // Metadata attrs (name, isSaved) are untouched.
        assert!(out.contains("name=\"runSpeedIndex\""));
        assert!(out.contains("isSaved=\"true\""));
        // The other setting is unchanged.
        assert!(out.contains("boolValue=\"false\""));
    }

    #[test]
    fn edits_only_change_targeted_values() {
        let edits = vec![
            Edit {
                id: 0,
                value: "5".into(),
            },
            Edit {
                id: 1,
                value: "false".into(),
            },
        ];
        let out = apply_edits(SAMPLE, &edits).unwrap();
        assert!(out.contains("integer=\"5\""));
        assert!(out.contains("boolean=\"false\""));
        assert!(out.contains("name=\"MAX\""));
        // Re-parsing reflects the edits.
        let f = parse(&out);
        assert_eq!(f[0].value, "5");
        assert_eq!(f[1].value, "false");
    }

    /// A real-shaped modSettings file (BOM, declaration, comment, single quotes, character
    /// references, an entity nothing declares).
    const REAL: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\"?>\n<settings>\n    <!-- written by the mod -->\n    <setting name=\"ROLEPLAY_PAGER_ACTIVATED\" boolean=\"true\"/>\n    <setting name=\"PAGER_TEXT\" string=\"Line 1&#10;Line 2&#9;end\" note='say \"hi\"'/>\n    <setting name=\"ODD\" string=\"&custom;\" isSaved=\"true\"/>\n    <setting name=\"ROLEPLAY_PAGER_MAX_INTERVAL\" integer=\"1\"></setting>\n</settings>\n";

    /// #354: editing one value leaves every other attribute's bytes alone: character
    /// references stay references (not literal newlines that read back as spaces), and a
    /// value that can't be resolved is kept, not blanked.
    #[test]
    fn editing_one_value_leaves_the_rest_of_the_file_byte_identical() {
        let f = parse(REAL);
        let max = f
            .iter()
            .find(|f| f.label == "ROLEPLAY_PAGER_MAX_INTERVAL")
            .unwrap();
        let out = apply_edits(
            REAL,
            &[Edit {
                id: max.id,
                value: "5".into(),
            }],
        )
        .unwrap();
        assert_eq!(
            out,
            REAL.replace("integer=\"1\"", "integer=\"5\""),
            "only the edited value changed"
        );
        // And the pager text still reads back with its newline and tab.
        let f = parse(&out);
        let text = f.iter().find(|f| f.label == "PAGER_TEXT").unwrap();
        assert_eq!(text.value, "Line 1\nLine 2\tend");
        let odd = f.iter().find(|f| f.label == "ODD").unwrap();
        assert_eq!(odd.value, "&custom;", "shown as written, not blank");
    }

    /// #354: an edited value with whitespace is written as character references, so it
    /// reads back exactly; its element's other attributes keep their raw bytes.
    #[test]
    fn an_edited_value_round_trips_tabs_and_newlines() {
        let f = parse(REAL);
        let text = f.iter().find(|f| f.label == "PAGER_TEXT").unwrap();
        let out = apply_edits(
            REAL,
            &[Edit {
                id: text.id,
                value: "A & B\n\tC".into(),
            }],
        )
        .unwrap();
        assert!(
            out.contains(r#"string="A &amp; B&#10;&#9;C" note="say &quot;hi&quot;""#),
            "{out}"
        );
        let f = parse(&out);
        let text = f.iter().find(|f| f.label == "PAGER_TEXT").unwrap();
        assert_eq!(text.value, "A & B\n\tC");
    }

    /// #354: a cut-off file isn't rewritten (and reported saved); nor is a value XML can't
    /// hold.
    #[test]
    fn a_cut_off_file_or_an_impossible_value_is_refused() {
        let cut = "<settings>\n    <setting name=\"MAX\" integer=\"1\"/>\n";
        let err = apply_edits(
            cut,
            &[Edit {
                id: 0,
                value: "2".into(),
            }],
        )
        .unwrap_err();
        assert!(err.contains("cut off or damaged"), "{err}");
        let err = apply_edits(
            SAMPLE,
            &[Edit {
                id: 0,
                value: "1\u{1}".into(),
            }],
        )
        .unwrap_err();
        assert!(err.contains("can't be saved"), "{err}");
    }

    /// An edit for a field that isn't in the file is an error, not a silent "saved".
    #[test]
    fn an_edit_that_matches_no_field_is_an_error() {
        let err = apply_edits(
            SAMPLE,
            &[Edit {
                id: 99,
                value: "1".into(),
            }],
        )
        .unwrap_err();
        assert!(err.contains("nothing was saved"), "{err}");
    }
}
