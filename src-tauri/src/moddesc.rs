//! Streaming parser for `modDesc.xml`, grounded in the authoritative
//! `shared/xml/schema/modDesc.xsd` (descVersion 105-era). Uses quick-xml — never
//! regex (regex XML parsing is a top bug source in this class of tool).
//!
//! We extract exactly what the library + conflict engines need and nothing more:
//! identity/metadata, map detection, dependencies (tech-name strings), the six
//! namespace-registration surfaces, `<uniqueType>`, script sources, and a couple
//! of content counts.

use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde::Serialize;
use std::collections::BTreeMap;

/// One name registered into a game namespace — the raw material for conflict
/// detection. `kind` is one of the six surfaces plus a few extras.
#[derive(Debug, Clone, Serialize)]
pub struct Registration {
    pub kind: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModDesc {
    pub desc_version: Option<i64>,
    pub author: Option<String>,
    pub version: Option<String>,
    /// Best-effort display title (prefers English, falls back to any language).
    pub title: Option<String>,
    pub icon_filename: Option<String>,

    pub is_map: bool,
    pub map_id: Option<String>,
    pub map_title: Option<String>,

    /// Dependency tech-names (mod filename without `.zip`). Authoritative schema:
    /// each `<dependency>` is a plain string.
    pub dependencies: Vec<String>,
    /// `<extraSourceFiles><sourceFile filename>` — global Lua injected into the game.
    pub scripts: Vec<String>,
    /// Namespace registrations across all six surfaces (+ actions/brands/etc.).
    pub registrations: Vec<Registration>,
    /// GIANTS' explicit conflict primitive: only one mod of a `uniqueType` loads.
    pub unique_type: Option<String>,

    pub store_item_count: u32,
    /// `xmlFilename`s of `<storeItem>`s — used to read the authoritative FS store
    /// `<category>` for accurate vehicle/tool sub-categorization.
    pub store_item_files: Vec<String>,
    pub mp_supported: bool,
    pub mp_only: bool,
    /// `<fillTypes filename>` — the additional fillTypes XML this mod registers. Used to
    /// detect cross-mod fillType name collisions (silent load-order overrides).
    pub fill_types_file: Option<String>,
    /// `<densityMapHeightTypes filename>`: the ground-pile (height) types this mod adds. They
    /// share the map's fixed number of slots (#339).
    pub height_types_file: Option<String>,
    /// Each `<map configFilename>` (`map.xml`), for maps that bring their own height and fill
    /// types. A mod can ship several maps.
    pub map_config_files: Vec<String>,
}

/// Pick the best display title from a set of localized strings.
fn pick_localized(langs: &BTreeMap<String, String>) -> Option<String> {
    langs
        .get("en")
        .or_else(|| langs.values().next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn attr(e: &quick_xml::events::BytesStart, key: &[u8]) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| {
            a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
                .ok()
                .map(|c| c.into_owned())
        })
}

/// Parse a `modDesc.xml` document into the fields Silo needs.
/// Never panics on malformed input — returns whatever it could extract.
pub fn parse(xml: &str) -> ModDesc {
    let mut md = ModDesc::default();
    let mut mod_title: BTreeMap<String, String> = BTreeMap::new();
    let mut map_title: BTreeMap<String, String> = BTreeMap::new();

    let mut reader = Reader::from_str(xml);
    // We trim text ourselves before `absorb_text`, so no reader-level trim config is
    // needed (keeps us off quick-xml's version-specific Config API).

    // Stack of local element names, so text/attrs can be attributed to context.
    let mut stack: Vec<String> = Vec::new();
    let mut buf = Vec::new();
    // Text of the current element, which quick-xml splits at every `&amp;` (#266).
    // Routed to its field when the next tag arrives, while `stack` still names the
    // element it belongs to.
    let mut run = crate::xmltext::TextRun::default();
    let mut flush = |md: &mut ModDesc, stack: &[String], run: &mut crate::xmltext::TextRun| {
        if !run.is_empty() {
            let txt = run.take();
            absorb_text(md, &mut mod_title, &mut map_title, stack, txt.trim());
        }
    };

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Eof) | Err(_) => break,
            Ok(Event::Start(e)) => {
                flush(&mut md, &stack, &mut run);
                let name = local_name(&e);
                handle_open(&mut md, &e, &name, &stack);
                stack.push(name);
            }
            Ok(Event::Empty(e)) => {
                // Empty (self-closing) elements have NO End event — pushing them
                // would corrupt parent detection for every following sibling.
                flush(&mut md, &stack, &mut run);
                let name = local_name(&e);
                handle_open(&mut md, &e, &name, &stack);
            }
            Ok(Event::End(_)) => {
                flush(&mut md, &stack, &mut run);
                stack.pop();
            }
            Ok(Event::Text(t)) => run.push_text(&t),
            Ok(Event::GeneralRef(r)) => run.push_ref(&r),
            Ok(Event::CData(c)) => {
                if let Ok(txt) = std::str::from_utf8(&c) {
                    run.push_str(txt);
                }
            }
            _ => {}
        }
        buf.clear();
    }

    md.title = pick_localized(&mod_title);
    md.map_title = pick_localized(&map_title);
    md
}

/// Handle an opening tag (Start or Empty): pull attributes / set flags based on
/// the element name and its parent (`stack.last()`, before the element is pushed).
fn handle_open(md: &mut ModDesc, e: &quick_xml::events::BytesStart, name: &str, stack: &[String]) {
    let parent = stack.last().map(String::as_str).unwrap_or("");

    match name {
        "modDesc" => {
            if let Some(v) = attr(e, b"descVersion") {
                md.desc_version = v.trim().parse().ok();
            }
        }
        "maps" => md.is_map = true,
        "fillTypes" if parent == "modDesc" => {
            if let Some(f) = attr(e, b"filename") {
                md.fill_types_file = Some(f.replace('\\', "/"));
            }
        }
        "densityMapHeightTypes" if parent == "modDesc" => {
            if let Some(f) = attr(e, b"filename") {
                md.height_types_file = Some(f.replace('\\', "/"));
            }
        }
        "map" if parent == "maps" => {
            if md.map_id.is_none() {
                md.map_id = attr(e, b"id");
            }
            if let Some(f) = attr(e, b"configFilename") {
                md.map_config_files.push(f.replace('\\', "/"));
            }
        }
        "sourceFile" if parent == "extraSourceFiles" => {
            if let Some(f) = attr(e, b"filename") {
                md.scripts.push(f.replace('\\', "/"));
            }
        }
        "storeItem" if parent == "storeItems" => {
            md.store_item_count += 1;
            if let Some(f) = attr(e, b"xmlFilename") {
                md.store_item_files.push(f.replace('\\', "/"));
            }
        }
        "multiplayer" => {
            md.mp_supported = attr(e, b"supported").as_deref() == Some("true");
            md.mp_only = attr(e, b"only").as_deref() == Some("true");
        }
        // A <specialization> whose *direct* parent is one of the three
        // specialization containers is a registration (NOT one inside a <type>,
        // which is a reference).
        "specialization" => {
            let kind = match parent {
                "specializations" => Some("specialization"),
                "placeableSpecializations" => Some("placeableSpecialization"),
                "handToolSpecializations" => Some("handToolSpecialization"),
                _ => None,
            };
            if let (Some(kind), Some(nm)) = (kind, attr(e, b"name")) {
                md.registrations.push(Registration {
                    kind: kind.into(),
                    name: nm,
                });
            }
        }
        // <type> under one of the three type containers registers a type name.
        "type" => {
            let kind = match parent {
                "vehicleTypes" => Some("vehicleType"),
                "placeableTypes" => Some("placeableType"),
                "handToolTypes" => Some("handToolType"),
                _ => None,
            };
            if let (Some(kind), Some(nm)) = (kind, attr(e, b"name")) {
                md.registrations.push(Registration {
                    kind: kind.into(),
                    name: nm,
                });
            }
        }
        "action" if parent == "actions" => {
            if let Some(nm) = attr(e, b"name") {
                md.registrations.push(Registration {
                    kind: "action".into(),
                    name: nm,
                });
            }
        }
        "brand" if parent == "brands" => {
            if let Some(nm) = attr(e, b"name") {
                md.registrations.push(Registration {
                    kind: "brand".into(),
                    name: nm,
                });
            }
        }
        "storeCategory" if parent == "storeCategories" => {
            if let Some(nm) = attr(e, b"name") {
                md.registrations.push(Registration {
                    kind: "storeCategory".into(),
                    name: nm,
                });
            }
        }
        _ => {}
    }
}

/// Route an element's whole (trimmed) text to the right field based on the element stack.
fn absorb_text(
    md: &mut ModDesc,
    mod_title: &mut BTreeMap<String, String>,
    map_title: &mut BTreeMap<String, String>,
    stack: &[String],
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    let last = stack.last().map(String::as_str).unwrap_or("");
    let parent = stack.iter().rev().nth(1).map(String::as_str).unwrap_or("");
    let gp = stack.iter().rev().nth(2).map(String::as_str).unwrap_or("");

    match last {
        "author" if parent == "modDesc" => set_once(&mut md.author, text),
        "version" if parent == "modDesc" => set_once(&mut md.version, text),
        "iconFilename" if parent == "modDesc" => set_once(&mut md.icon_filename, text),
        "uniqueType" if parent == "modDesc" => set_once(&mut md.unique_type, text),
        "dependency" if parent == "dependencies" => {
            // Take the leading tech-name token; ignore any trailing URL/notes that
            // some mods non-canonically append. Never assume a fixed shape.
            if let Some(tok) = text.split_whitespace().next() {
                let tok = tok.trim_matches(|c: char| c == '"' || c == '\'');
                if !tok.is_empty() {
                    md.dependencies.push(tok.to_string());
                }
            }
        }
        // Localized <title><en>… directly under modDesc.
        lang if parent == "title" && gp == "modDesc" => {
            mod_title
                .entry(lang.to_string())
                .or_insert_with(|| text.to_string());
        }
        // Localized map title: <maps><map><title><en>…
        lang if parent == "title" && gp == "map" => {
            map_title
                .entry(lang.to_string())
                .or_insert_with(|| text.to_string());
        }
        _ => {}
    }
}

fn set_once(slot: &mut Option<String>, text: &str) {
    if slot.is_none() {
        *slot = Some(text.to_string());
    }
}

fn local_name(e: &quick_xml::events::BytesStart) -> String {
    String::from_utf8_lossy(e.local_name().as_ref()).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<modDesc descVersion="105">
    <author>Pepperonie, EWW_Bobo</author>
    <version>1.0.0.0</version>
    <title><en>Adjustable Engine Power</en><de>Einstellbare Motorleistung</de></title>
    <iconFilename>icon_adjustEnginePower.dds</iconFilename>
    <multiplayer supported="true"/>
    <extraSourceFiles>
        <sourceFile filename="AdjustEnginePower.lua"/>
        <sourceFile filename="register.lua"/>
    </extraSourceFiles>
</modDesc>"#;

    #[test]
    fn parses_core_fields() {
        let md = parse(SAMPLE);
        assert_eq!(md.desc_version, Some(105));
        assert_eq!(md.author.as_deref(), Some("Pepperonie, EWW_Bobo"));
        assert_eq!(md.version.as_deref(), Some("1.0.0.0"));
        assert_eq!(md.title.as_deref(), Some("Adjustable Engine Power"));
        assert_eq!(
            md.icon_filename.as_deref(),
            Some("icon_adjustEnginePower.dds")
        );
        assert!(md.mp_supported);
        assert_eq!(md.scripts.len(), 2);
        assert!(!md.is_map);
    }

    /// #266: quick-xml splits text at each entity reference; the whole value must survive.
    #[test]
    fn keeps_text_after_an_escaped_ampersand() {
        let xml = r#"<modDesc descVersion="105">
            <author>Elm &amp; Ash</author>
            <version>1.0.0.0</version>
            <title><en>Rust &amp; Wear Skins</en><de>Rost &#38; Verschlei&#223;</de></title>
            <maps><map id="m"><title><en>Hills &amp; Dales</en></title></map></maps>
            <uniqueType><![CDATA[a&b]]> &lt;x&gt;</uniqueType>
        </modDesc>"#;
        let md = parse(xml);
        assert_eq!(md.author.as_deref(), Some("Elm & Ash"));
        assert_eq!(md.title.as_deref(), Some("Rust & Wear Skins"));
        assert_eq!(md.map_title.as_deref(), Some("Hills & Dales"));
        assert_eq!(md.unique_type.as_deref(), Some("a&b <x>"));

        let de_only =
            parse(r#"<modDesc><title><de>Rost &#38; Verschlei&#223;</de></title></modDesc>"#);
        assert_eq!(de_only.title.as_deref(), Some("Rost & Verschleiß"));
    }

    #[test]
    fn detects_map_and_registrations() {
        let xml = r#"<modDesc descVersion="105">
            <author>A</author><version>1.0.0.0</version><iconFilename>i.dds</iconFilename>
            <maps><map id="mapFoo" configFilename="map\map.xml"><title><en>Foo Valley</en></title></map></maps>
            <densityMapHeightTypes filename="xml/heightTypes.xml"/>
            <specializations>
                <specialization name="fooSpec" className="C" filename="f.lua"/>
            </specializations>
            <vehicleTypes>
                <type name="fooTruck" filename="t.lua">
                    <specialization name="fooSpec"/>
                </type>
            </vehicleTypes>
            <uniqueType>fooUnique</uniqueType>
        </modDesc>"#;
        let md = parse(xml);
        assert!(md.is_map);
        assert_eq!(md.map_id.as_deref(), Some("mapFoo"));
        assert_eq!(md.map_config_files, ["map/map.xml"]);
        assert_eq!(md.height_types_file.as_deref(), Some("xml/heightTypes.xml"));
        assert_eq!(md.map_title.as_deref(), Some("Foo Valley"));
        assert_eq!(md.unique_type.as_deref(), Some("fooUnique"));
        // One specialization registration + one vehicleType registration.
        // The <specialization> inside <type> is a reference, not a registration.
        assert_eq!(md.registrations.len(), 2);
        assert!(md
            .registrations
            .iter()
            .any(|r| r.kind == "specialization" && r.name == "fooSpec"));
        assert!(md
            .registrations
            .iter()
            .any(|r| r.kind == "vehicleType" && r.name == "fooTruck"));
    }
}
