//! Path-addressable read/write for structured config XML (game.xml,
//! careerSavegame.xml). Unlike `settings_form` (which auto-discovers typed
//! attributes), this edits *specific known* values chosen by a curated schema in
//! the UI — so we can present friendly labels, option lists, and presets.
//!
//! Paths are dot-separated element names from the root, with an optional `@attr`
//! suffix for an attribute value:
//!   "game.graphic.scalability.shadowMapSize"   (leaf element text)
//!   "game.graphic.display.vsync@adaptive"      (attribute)
//!
//! Write-back re-emits the document as read, changing only targeted values.

use quick_xml::events::{BytesStart, BytesText, Event};
use quick_xml::reader::Reader;
use quick_xml::writer::Writer;
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Edit {
    pub path: String,
    pub value: String,
}

fn local(name: &[u8]) -> String {
    let s = String::from_utf8_lossy(name);
    s.rsplit(':').next().unwrap_or(&s).to_string()
}

fn split_attr(path: &str) -> (&str, Option<&str>) {
    match path.split_once('@') {
        Some((el, attr)) => (el, Some(attr)),
        None => (path, None),
    }
}

/// Read the values at the given paths (missing paths are simply absent).
pub fn get_values(xml: &str, paths: &[String]) -> HashMap<String, String> {
    // Partition targets into element-text vs attribute lookups.
    let mut want_text: HashMap<String, String> = HashMap::new(); // elemPath -> resultKey
    let mut want_attr: HashMap<(String, String), String> = HashMap::new(); // (elemPath, attr) -> key
    for p in paths {
        let (el, attr) = split_attr(p);
        match attr {
            Some(a) => {
                want_attr.insert((el.to_string(), a.to_string()), p.clone());
            }
            None => {
                want_text.insert(el.to_string(), p.clone());
            }
        }
    }

    let mut out = HashMap::new();
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();

    let check_attrs = |e: &BytesStart, path: &str, out: &mut HashMap<String, String>| {
        for a in e.attributes().flatten() {
            let key = local(a.key.as_ref());
            if let Some(rk) = want_attr.get(&(path.to_string(), key)) {
                if let Ok(v) = a.normalized_value(quick_xml::XmlVersion::Implicit1_0) {
                    out.insert(rk.clone(), v.into_owned());
                }
            }
        }
    };

    // Element text arrives split at every `&amp;` (#266): gather it, then read it when
    // the next tag arrives, while `stack` still names the element it belongs to.
    let mut run = crate::xmltext::TextRun::default();
    let take_text =
        |stack: &[String], run: &mut crate::xmltext::TextRun, out: &mut HashMap<String, String>| {
            if run.is_empty() {
                return;
            }
            let v = run.take();
            if let Some(rk) = want_text.get(&stack.join(".")) {
                let v = v.trim();
                if !v.is_empty() {
                    out.insert(rk.clone(), v.to_string());
                }
            }
        };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Start(e)) => {
                take_text(&stack, &mut run, &mut out);
                stack.push(local(e.name().as_ref()));
                check_attrs(&e, &stack.join("."), &mut out);
            }
            Ok(Event::Empty(e)) => {
                take_text(&stack, &mut run, &mut out);
                stack.push(local(e.name().as_ref()));
                check_attrs(&e, &stack.join("."), &mut out);
                stack.pop();
            }
            Ok(Event::End(_)) => {
                take_text(&stack, &mut run, &mut out);
                stack.pop();
            }
            Ok(Event::Text(t)) => run.push_text(&t),
            Ok(Event::GeneralRef(r)) => run.push_ref(&r),
            Ok(Event::CData(c)) => run.push_str(&String::from_utf8_lossy(&c)),
            _ => {}
        }
        buf.clear();
    }
    out
}

/// The element at `path` as it should be written: rebuilt when one of its attributes is
/// edited, otherwise exactly as read. Untouched attributes keep their raw bytes (#354).
/// Records each edit path it applied.
fn element_out<'a>(
    e: BytesStart<'a>,
    path: &str,
    edits: &[Edit],
    applied: &mut HashSet<String>,
) -> Result<BytesStart<'a>, String> {
    let prefix = format!("{path}@");
    if !edits.iter().any(|ed| ed.path.starts_with(&prefix)) {
        return Ok(e);
    }
    crate::xmltext::rebuild_attributes(&e, |a| {
        let target = format!("{prefix}{}", local(a.key.as_ref()));
        let ed = edits.iter().find(|ed| ed.path == target)?;
        applied.insert(target);
        Some(ed.value.clone())
    })
}

fn write(writer: &mut Writer<Cursor<Vec<u8>>>, ev: Event) -> Result<(), String> {
    writer.write_event(ev).map_err(|e| e.to_string())
}

/// Apply edits and return the new XML (only targeted values change). An edit whose path
/// isn't in the file is an error naming it, so a save never reports success for a value
/// it didn't write.
pub fn set_values(xml: &str, edits: &[Edit]) -> Result<String, String> {
    let mut reader = Reader::from_str(xml);
    let mut writer = Writer::new(Cursor::new(Vec::new()));
    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut applied: HashSet<String> = HashSet::new();
    // When the open element matched a text-edit path: the replacement value, and whether
    // it has been written yet. The element's own text (split at every `&amp;` into Text
    // and GeneralRef events, #266) is dropped in favour of the one replacement; an
    // element with no text at all (`<x></x>`) gets the value written before its End.
    let mut pending_text: Option<String> = None;
    let mut replaced = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            Ok(Event::Start(e)) => {
                stack.push(local(e.name().as_ref()));
                let path = stack.join(".");
                let out = element_out(e, &path, edits, &mut applied)?;
                write(&mut writer, Event::Start(out))?;
                pending_text = edits
                    .iter()
                    .find(|ed| ed.path == path)
                    .map(|ed| ed.value.clone());
                replaced = false;
            }
            Ok(Event::Empty(e)) => {
                stack.push(local(e.name().as_ref()));
                let path = stack.join(".");
                stack.pop();
                let out = element_out(e, &path, edits, &mut applied)?;
                match edits.iter().find(|ed| ed.path == path) {
                    // `<name/>` with a text edit: write it out as `<name>value</name>`.
                    Some(ed) => {
                        let end = out.to_end().into_owned();
                        write(&mut writer, Event::Start(out))?;
                        write(&mut writer, Event::Text(BytesText::new(&ed.value)))?;
                        write(&mut writer, Event::End(end))?;
                        applied.insert(path);
                    }
                    None => write(&mut writer, Event::Empty(out))?,
                }
            }
            Ok(Event::End(e)) => {
                if let Some(v) = pending_text.take() {
                    if !replaced {
                        write(&mut writer, Event::Text(BytesText::new(&v)))?;
                    }
                    applied.insert(stack.join("."));
                }
                stack.pop();
                replaced = false;
                write(&mut writer, Event::End(e))?;
            }
            Ok(ev @ (Event::Text(_) | Event::GeneralRef(_) | Event::CData(_))) => {
                match &pending_text {
                    Some(v) if !replaced => {
                        write(&mut writer, Event::Text(BytesText::new(v)))?;
                        applied.insert(stack.join("."));
                        replaced = true;
                    }
                    Some(_) => {} // the rest of the old value: already replaced
                    None => write(&mut writer, ev)?,
                }
            }
            Ok(ev) => write(&mut writer, ev)?,
        }
        buf.clear();
    }
    let missed: Vec<&str> = edits
        .iter()
        .filter(|ed| !applied.contains(&ed.path))
        .map(|ed| ed.path.as_str())
        .collect();
    if !missed.is_empty() {
        return Err(format!(
            "Silo couldn't find where to save {} in this file, so nothing was saved: {}",
            if missed.len() == 1 {
                "this setting"
            } else {
                "these settings"
            },
            missed.join(", ")
        ));
    }
    let bytes = writer.into_inner().into_inner();
    let mut s = String::from_utf8(bytes).map_err(|e| e.to_string())?;
    if xml.starts_with('\u{feff}') && !s.starts_with('\u{feff}') {
        s.insert(0, '\u{feff}');
    }
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: &str = r#"<game>
    <graphic>
        <scalability>
            <shadowMapSize>2048</shadowMapSize>
            <softShadows>true</softShadows>
            <fsr quality="1"/>
        </scalability>
    </graphic>
</game>"#;

    #[test]
    fn gets_element_text_and_attr() {
        let v = get_values(
            G,
            &[
                "game.graphic.scalability.shadowMapSize".into(),
                "game.graphic.scalability.fsr@quality".into(),
            ],
        );
        assert_eq!(
            v.get("game.graphic.scalability.shadowMapSize")
                .map(|s| s.as_str()),
            Some("2048")
        );
        assert_eq!(
            v.get("game.graphic.scalability.fsr@quality")
                .map(|s| s.as_str()),
            Some("1")
        );
    }

    #[test]
    fn sets_element_text_and_attr() {
        let out = set_values(
            G,
            &[
                Edit {
                    path: "game.graphic.scalability.shadowMapSize".into(),
                    value: "4096".into(),
                },
                Edit {
                    path: "game.graphic.scalability.softShadows".into(),
                    value: "false".into(),
                },
                Edit {
                    path: "game.graphic.scalability.fsr@quality".into(),
                    value: "3".into(),
                },
            ],
        )
        .unwrap();
        assert!(out.contains("<shadowMapSize>4096</shadowMapSize>"));
        assert!(out.contains("<softShadows>false</softShadows>"));
        assert!(out.contains("quality=\"3\""));
        // Untouched structure preserved.
        assert!(out.contains("<graphic>"));
    }

    /// #266: a value with an entity reads whole, and replacing it drops every piece of
    /// the old value (quick-xml splits it at the `&amp;`), keeping other entities intact.
    #[test]
    fn reads_and_replaces_text_split_at_an_entity() {
        let xml =
            r#"<game><player><name>Elm &amp; Ash</name><club>A &amp; B</club></player></game>"#;
        let v = get_values(xml, &["game.player.name".into()]);
        assert_eq!(
            v.get("game.player.name").map(String::as_str),
            Some("Elm & Ash")
        );

        let out = set_values(
            xml,
            &[Edit {
                path: "game.player.name".into(),
                value: "Oak & Pine".into(),
            }],
        )
        .unwrap();
        assert!(out.contains("<name>Oak &amp; Pine</name>"), "{out}");
        assert!(out.contains("<club>A &amp; B</club>"), "{out}");
    }

    /// An empty element (`<x></x>`) has no Text event; the edit must still land.
    #[test]
    fn sets_text_of_an_empty_element() {
        let out = set_values(
            "<game><a></a></game>",
            &[Edit {
                path: "game.a".into(),
                value: "1".into(),
            }],
        )
        .unwrap();
        assert!(out.contains("<a>1</a>"), "{out}");
    }

    fn edit(path: &str, value: &str) -> Edit {
        Edit {
            path: path.into(),
            value: value.into(),
        }
    }

    /// A real-shaped game.xml: BOM, declaration, an attribute with character references on
    /// an element we don't edit, one we do, and a self-closing element.
    const GAME: &str = "\u{feff}<?xml version=\"1.0\" encoding=\"utf-8\" standalone=\"no\"?>\n<game>\n    <language>-1</language>\n    <graphic>\n        <display>\n            <fullscreenMode>windowed_fullscreen</fullscreenMode>\n            <vsync adaptive=\"true\" note=\"a&#10;b&#9;c\" mode='x \"y\"'>true</vsync>\n        </display>\n        <scalability>\n            <shadowMapSize>2048</shadowMapSize>\n            <fsr quality=\"1\" label=\"q&#10;1\"/>\n            <performanceClass/>\n        </scalability>\n    </graphic>\n</game>\n";

    /// #354: editing one attribute leaves every other attribute's raw bytes untouched,
    /// including character references that would otherwise come back as spaces.
    #[test]
    fn editing_an_attribute_keeps_the_rest_of_the_file_byte_identical() {
        let out = set_values(
            GAME,
            &[edit("game.graphic.display.vsync@adaptive", "false")],
        )
        .unwrap();
        assert_eq!(
            out,
            GAME.replace(
                r#"<vsync adaptive="true" note="a&#10;b&#9;c" mode='x "y"'>"#,
                r#"<vsync adaptive="false" note="a&#10;b&#9;c" mode="x &quot;y&quot;">"#
            )
        );
        let v = get_values(&out, &["game.graphic.display.vsync@note".into()]);
        assert_eq!(
            v.get("game.graphic.display.vsync@note").map(String::as_str),
            Some("a\nb\tc")
        );

        // A text edit elsewhere doesn't touch any attribute at all.
        let out = set_values(
            GAME,
            &[edit("game.graphic.scalability.shadowMapSize", "4096")],
        )
        .unwrap();
        assert_eq!(out, GAME.replace(">2048<", ">4096<"));
    }

    /// #354: an edited attribute value with whitespace round-trips exactly.
    #[test]
    fn an_edited_attribute_round_trips_whitespace() {
        let out = set_values(
            GAME,
            &[edit("game.graphic.scalability.fsr@label", "two\nlines")],
        )
        .unwrap();
        assert!(
            out.contains(r#"<fsr quality="1" label="two&#10;lines"/>"#),
            "{out}"
        );
        let v = get_values(&out, &["game.graphic.scalability.fsr@label".into()]);
        assert_eq!(
            v.get("game.graphic.scalability.fsr@label")
                .map(String::as_str),
            Some("two\nlines")
        );
    }

    /// #354: a text edit to a self-closing element writes it out as start, text and end.
    #[test]
    fn sets_text_of_a_self_closing_element() {
        let out = set_values(
            GAME,
            &[
                edit("game.graphic.scalability.performanceClass", "High"),
                edit("game.graphic.scalability.fsr@quality", "3"),
            ],
        )
        .unwrap();
        assert!(
            out.contains("<performanceClass>High</performanceClass>"),
            "{out}"
        );
        assert!(
            out.contains(r#"<fsr quality="3" label="q&#10;1"/>"#),
            "{out}"
        );
        let v = get_values(&out, &["game.graphic.scalability.performanceClass".into()]);
        assert_eq!(
            v.get("game.graphic.scalability.performanceClass")
                .map(String::as_str),
            Some("High")
        );
    }

    /// #354: an edit whose path matches nothing (a missing element or attribute) is an
    /// error naming it, so the editor never reports "saved" for a value it didn't write.
    #[test]
    fn an_edit_that_matches_nothing_is_an_error() {
        for missing in [
            "game.graphic.display.nope",
            "game.graphic.display.vsync@nope",
            "game.graphic.nope.deeper",
        ] {
            let err =
                set_values(GAME, &[edit("game.language", "0"), edit(missing, "1")]).unwrap_err();
            assert!(err.contains("nothing was saved"), "{err}");
            assert!(err.contains(missing), "{err}");
            assert!(!err.contains("game.language"), "{err}");
        }
    }

    /// #354: a value written as CDATA is read.
    #[test]
    fn reads_cdata_text() {
        let xml = "<careerSavegame><settings><savegameName><![CDATA[Elm & Ash]]></savegameName></settings></careerSavegame>";
        let v = get_values(xml, &["careerSavegame.settings.savegameName".into()]);
        assert_eq!(
            v.get("careerSavegame.settings.savegameName")
                .map(String::as_str),
            Some("Elm & Ash")
        );
    }
}
