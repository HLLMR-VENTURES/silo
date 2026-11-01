//! Shared helpers for pulling text out of quick-xml events.
//!
//! quick-xml 0.41 split the old `BytesText::unescape()` — which decoded the bytes AND
//! resolved XML entities in one call — into a byte `decode()` plus a separate
//! `escape::unescape()` step. It also stopped putting entity references inside `Text`
//! events: `Elm &amp; Ash` arrives as `Text("Elm ")`, `GeneralRef("amp")`, `Text(" Ash")`.
//! A parser that keeps only the first `Text` run cuts the value at the first entity
//! (#266), so element text is collected with [`TextRun`] and read when the element's
//! content ends.
//!
//! It also holds the attribute rewrite shared by the two config editors
//! ([`rebuild_attributes`]): untouched attributes keep their raw bytes, so editing one value
//! never rewrites the others (#354).

use quick_xml::events::attributes::Attribute;
use quick_xml::events::{BytesRef, BytesStart, BytesText};
use std::borrow::Cow;

/// Escape a value for writing as a double-quoted attribute: the five predefined entities,
/// plus tab, newline and carriage return as character references. Written as literal
/// characters, those three would be read back as spaces (attribute-value normalization).
pub fn escape_attr(v: &str) -> String {
    let mut out = String::with_capacity(v.len());
    for ch in v.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' => out.push_str("&#9;"),
            '\n' => out.push_str("&#10;"),
            '\r' => out.push_str("&#13;"),
            c => out.push(c),
        }
    }
    out
}

/// An attribute's raw value (still escaped, exactly as the file has it), ready to write
/// between double quotes. A value that was single-quoted may hold a literal `"`: that
/// character becomes `&quot;`, which reads back the same.
fn raw_for_double_quotes(raw: &[u8]) -> Cow<'_, [u8]> {
    if !raw.contains(&b'"') {
        return Cow::Borrowed(raw);
    }
    let mut out = Vec::with_capacity(raw.len() + 8);
    for &b in raw {
        if b == b'"' {
            out.extend_from_slice(b"&quot;");
        } else {
            out.push(b);
        }
    }
    Cow::Owned(out)
}

/// The value a reader sees for this attribute (entities resolved, whitespace normalized),
/// or `None` when it can't be resolved (an undeclared entity).
pub fn attr_value(a: &Attribute) -> Option<String> {
    a.normalized_value(quick_xml::XmlVersion::Implicit1_0)
        .ok()
        .map(Cow::into_owned)
}

/// XML 1.0's `Char` production: tab, LF, CR, and everything from U+0020 up, except
/// surrogates (which a Rust `char` can't hold anyway) and U+FFFE/U+FFFF.
fn is_xml_char(c: u32) -> bool {
    matches!(c, 0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF)
}

/// Refuse text holding a character XML can't contain at all, not even as a reference (the
/// C0 controls other than tab/LF/CR, U+FFFE, U+FFFF).
pub fn check_chars(s: &str) -> Result<(), String> {
    match s.chars().find(|&c| !is_xml_char(c as u32)) {
        Some(c) => Err(format!(
            "it contains the character U+{:04X}, which XML can't hold",
            c as u32
        )),
        None => Ok(()),
    }
}

/// XML 1.0's `NameStartChar` (and, with `rest`, `NameChar`) productions.
fn is_name_char(c: char, rest: bool) -> bool {
    let start = matches!(c,
        ':' | 'A'..='Z' | '_' | 'a'..='z' | '\u{C0}'..='\u{D6}' | '\u{D8}'..='\u{F6}'
        | '\u{F8}'..='\u{2FF}' | '\u{370}'..='\u{37D}' | '\u{37F}'..='\u{1FFF}'
        | '\u{200C}'..='\u{200D}' | '\u{2070}'..='\u{218F}' | '\u{2C00}'..='\u{2FEF}'
        | '\u{3001}'..='\u{D7FF}' | '\u{F900}'..='\u{FDCF}' | '\u{FDF0}'..='\u{FFFD}'
        | '\u{10000}'..='\u{EFFFF}');
    start
        || (rest
            && matches!(c,
                '-' | '.' | '0'..='9' | '\u{B7}' | '\u{300}'..='\u{36F}' | '\u{203F}'..='\u{2040}'))
}

/// Refuse an element or attribute name that isn't an XML `Name` (`1value`, `a b`, ``).
pub fn check_name(name: &[u8]) -> Result<(), String> {
    let name = std::str::from_utf8(name).map_err(|_| "a name isn't valid text".to_string())?;
    let mut chars = name.chars();
    let ok = chars.next().is_some_and(|c| is_name_char(c, false))
        && chars.all(|c| is_name_char(c, true));
    if ok {
        Ok(())
    } else {
        Err(format!("\"{name}\" isn't a valid XML name"))
    }
}

/// Check an `<?xml ...?>` declaration's content (`xml version="1.0" ...`, as a `Decl`
/// event holds it): `version` first, then optionally `encoding`, then `standalone`, each
/// quoted once, with the values XML allows.
pub fn check_decl(content: &[u8]) -> Result<(), String> {
    let start = BytesStart::from_content(String::from_utf8_lossy(content), 3);
    let mut attrs = start.attributes();
    attrs.with_checks(true);
    let mut seen: Vec<String> = Vec::new();
    for a in attrs {
        let a = a.map_err(|err| format!("in the <?xml ...?> declaration: {err}"))?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let v = String::from_utf8_lossy(&a.value);
        let ok = match (seen.len(), key.as_str()) {
            (0, "version") => v
                .strip_prefix("1.")
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit())),
            (1, "encoding") => {
                v.starts_with(|c: char| c.is_ascii_alphabetic())
                    && v.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
            }
            // A second `standalone` is a duplicate, which `with_checks` already refuses.
            (1 | 2, "standalone") => v == "yes" || v == "no",
            _ => false,
        };
        if !ok {
            return Err(format!(
                "the <?xml ...?> declaration's {key}=\"{v}\" isn't allowed there"
            ));
        }
        seen.push(key);
    }
    if seen.is_empty() {
        return Err("the <?xml ...?> declaration has no version".into());
    }
    Ok(())
}

/// Check one reference's name (what sits between `&` and `;`): it must be one of the five
/// predefined entities or a character reference to a character XML allows. Settings files
/// have no DTD, so any other entity is undeclared and the game's parser rejects the file.
pub fn check_reference(name: &str) -> Result<(), String> {
    if quick_xml::escape::resolve_predefined_entity(name).is_some() {
        return Ok(());
    }
    let Some(num) = name.strip_prefix('#') else {
        return Err(format!("&{name}; isn't a known entity"));
    };
    let code = match num.strip_prefix('x') {
        Some(hex) if !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()) => {
            u32::from_str_radix(hex, 16).ok()
        }
        Some(_) => None,
        None if !num.is_empty() && num.bytes().all(|b| b.is_ascii_digit()) => num.parse().ok(),
        None => None,
    };
    match code {
        Some(c) if is_xml_char(c) => Ok(()),
        _ => Err(format!("&{name}; isn't a valid character reference")),
    }
}

/// Check every attribute of a tag the way the game's parser does: each one a valid name,
/// quoted and named once (`with_checks`), no `<` or character XML can't hold in a value,
/// and every `&` starting a reference that [`check_reference`] accepts.
pub fn check_attributes(e: &BytesStart) -> Result<(), String> {
    let mut attrs = e.attributes();
    attrs.with_checks(true);
    for a in attrs {
        let a = a.map_err(|err| err.to_string())?;
        check_name(a.key.as_ref())?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let value = std::str::from_utf8(&a.value)
            .map_err(|_| format!("the value of {key} isn't valid text"))?;
        check_chars(value).map_err(|why| format!("the value of {key}: {why}"))?;
        if value.contains('<') {
            return Err(format!("the value of {key} contains '<'"));
        }
        let mut rest = value;
        while let Some(at) = rest.find('&') {
            let after = &rest[at + 1..];
            let Some(end) = after.find(';') else {
                return Err(format!("the value of {key} has an '&' that isn't escaped"));
            };
            check_reference(&after[..end]).map_err(|why| format!("in {key}: {why}"))?;
            rest = &after[end + 1..];
        }
    }
    Ok(())
}

/// The error for a settings file that can't be rewritten safely (cut off, or not XML).
pub fn damaged(detail: impl std::fmt::Display) -> String {
    format!("This file is cut off or damaged ({detail}), so Silo didn't change it.")
}

/// Refuse a value typed into a settings form that XML can't hold.
pub fn check_value(v: &str) -> Result<(), String> {
    check_chars(v).map_err(|why| format!("That value can't be saved: {why}. Nothing was saved."))
}

/// Rebuild `e` with some attribute values replaced. `replacement` is asked once per
/// attribute, in order, and returns the new value for an attribute being edited. Every
/// other attribute keeps its raw bytes, so its character references (`&#10;`) and entities
/// survive; so does an edited one whose new value is what a reader already sees. A
/// malformed or duplicated attribute, or a new value XML can't hold, is an error: never
/// dropped, never written back empty.
pub fn rebuild_attributes(
    e: &BytesStart,
    mut replacement: impl FnMut(&Attribute) -> Option<String>,
) -> Result<BytesStart<'static>, String> {
    let name = String::from_utf8_lossy(e.name().as_ref()).into_owned();
    let mut out = BytesStart::new(name.clone());
    for a in e.attributes() {
        let a = a.map_err(|err| format!("an attribute of <{name}> is malformed ({err})"))?;
        let edited = replacement(&a).filter(|v| attr_value(&a).as_ref() != Some(v));
        let value = match &edited {
            Some(v) => {
                check_value(v)?;
                Cow::Owned(escape_attr(v).into_bytes())
            }
            None => raw_for_double_quotes(&a.value),
        };
        out.push_attribute(Attribute::from((a.key.as_ref(), value.as_ref())));
    }
    Ok(out)
}

/// Decode a text event to an owned `String` with XML entities resolved (`&amp;` → `&`),
/// or `None` if the bytes aren't valid text or an entity can't be resolved. Does not
/// trim — callers trim as needed.
pub fn text(t: &BytesText) -> Option<String> {
    let decoded = t.decode().ok()?;
    let unescaped = quick_xml::escape::unescape(&decoded).ok()?;
    Some(unescaped.into_owned())
}

/// Resolve a `GeneralRef` event: a character reference (`&#38;`, `&#x26;`) or one of the
/// five predefined entities (`&amp;` …). An entity we can't resolve (a DTD-declared one,
/// which FS files never use) is kept as written, `&name;`, rather than dropped.
pub fn reference(r: &BytesRef) -> Option<String> {
    if r.is_char_ref() {
        return r.resolve_char_ref().ok().flatten().map(String::from);
    }
    let name = r.decode().ok()?;
    Some(match quick_xml::escape::resolve_predefined_entity(&name) {
        Some(s) => s.to_string(),
        None => format!("&{name};"),
    })
}

/// The text content of the current element, gathered across the `Text`, `GeneralRef`
/// and `CData` events quick-xml splits it into. Push every piece, then [`take`] it when
/// the next tag (`Start`, `Empty` or `End`) arrives.
///
/// [`take`]: TextRun::take
#[derive(Debug, Default)]
pub struct TextRun(String);

impl TextRun {
    /// Append a `Text` event (entities resolved).
    pub fn push_text(&mut self, t: &BytesText) {
        if let Some(s) = text(t) {
            self.0.push_str(&s);
        }
    }

    /// Append a `GeneralRef` event (`&amp;` → `&`).
    pub fn push_ref(&mut self, r: &BytesRef) {
        if let Some(s) = reference(r) {
            self.0.push_str(&s);
        }
    }

    /// Append raw text, e.g. the bytes of a `CData` section.
    pub fn push_str(&mut self, s: &str) {
        self.0.push_str(s);
    }

    /// True when nothing has been pushed since the last [`TextRun::take`].
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Return the gathered text and start over. Does not trim.
    pub fn take(&mut self) -> String {
        std::mem::take(&mut self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{reference, text, TextRun};
    use quick_xml::events::{BytesRef, BytesText, Event};
    use quick_xml::reader::Reader;

    /// Attribute values read back exactly as written, including tab/newline/CR (#354).
    #[test]
    fn escape_attr_round_trips_whitespace_and_markup() {
        use quick_xml::events::Event;
        let value = "a\tb\nc\r\nd & <e> \"f\" 'g'";
        let xml = format!("<x v=\"{}\"/>", super::escape_attr(value));
        let mut reader = Reader::from_str(&xml);
        let Event::Empty(e) = reader.read_event().unwrap() else {
            panic!("expected <x/>");
        };
        let a = e.attributes().next().unwrap().unwrap();
        assert_eq!(super::attr_value(&a).as_deref(), Some(value));
    }

    /// Untouched attributes keep their raw bytes; a single-quoted value holding `"` is
    /// requoted safely; an unresolvable entity is kept, not blanked.
    #[test]
    fn rebuild_keeps_untouched_attributes_raw() {
        use quick_xml::events::Event;
        let xml = r#"<s a="1&#10;2" b='say "hi"' c="&custom;" d="old"/>"#;
        let mut reader = Reader::from_str(xml);
        let Event::Empty(e) = reader.read_event().unwrap() else {
            panic!("expected <s/>");
        };
        let out = super::rebuild_attributes(&e, |a| {
            (a.key.as_ref() == b"d").then(|| "new\tvalue".to_string())
        })
        .unwrap();
        let written = String::from_utf8(out.to_vec()).unwrap();
        assert_eq!(
            written,
            r#"s a="1&#10;2" b="say &quot;hi&quot;" c="&custom;" d="new&#9;value""#
        );
    }

    #[test]
    fn only_predefined_entities_and_legal_character_references_pass() {
        for ok in [
            "amp", "lt", "gt", "quot", "apos", "#10", "#x9", "#233", "#x1F600",
        ] {
            assert!(super::check_reference(ok).is_ok(), "{ok}");
        }
        for bad in [
            "nbsp", "missing", "#", "#x", "#0", "#1", "#xD800", "#xFFFE", "#+5", "#X41", "#-1",
            "#x110000",
        ] {
            assert!(super::check_reference(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_well_formed_declarations_pass() {
        for ok in [
            r#"xml version="1.0""#,
            r#"xml version='1.0' encoding="utf-8""#,
            r#"xml version="1.0" encoding="utf-8" standalone="no""#,
            r#"xml version="1.1" standalone="yes""#,
        ] {
            assert!(super::check_decl(ok.as_bytes()).is_ok(), "{ok}");
        }
        for bad in [
            "xml",
            "xml version=1.0",
            r#"xml version="1.0" version="1.0""#,
            r#"xml encoding="utf-8" version="1.0""#,
            r#"xml version="1.0" standalone="no" encoding="utf-8""#,
            r#"xml version="2.0""#,
            r#"xml version="1.0" standalone="maybe""#,
            r#"xml version="1.0" encoding="8bit""#,
            r#"xml version="1.0" foo="bar""#,
        ] {
            assert!(super::check_decl(bad.as_bytes()).is_err(), "{bad}");
        }
    }

    #[test]
    fn only_xml_names_pass() {
        for ok in [
            "settings", "a", "_x", "xsi:nil", "intValue", "a-b.c1", "été",
        ] {
            assert!(super::check_name(ok.as_bytes()).is_ok(), "{ok}");
        }
        for bad in ["", "1value", "-a", ".a", "a b", "a\"", "a=b", "a/"] {
            assert!(super::check_name(bad.as_bytes()).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn characters_xml_cannot_hold_are_refused() {
        assert!(super::check_chars("tab\tnewline\n\r é 😀").is_ok());
        for bad in ["a\u{1}b", "\u{0}", "\u{1F}", "\u{FFFE}", "\u{FFFF}"] {
            assert!(super::check_chars(bad).is_err(), "{bad:?}");
        }
    }

    /// A duplicated attribute is refused rather than silently dropped.
    #[test]
    fn rebuild_refuses_a_duplicate_attribute() {
        use quick_xml::events::Event;
        let mut reader = Reader::from_str(r#"<s a="1" a="2"/>"#);
        let Event::Empty(e) = reader.read_event().unwrap() else {
            panic!("expected <s/>");
        };
        assert!(super::rebuild_attributes(&e, |_| None).is_err());
    }

    #[test]
    fn resolves_entities_like_the_old_unescape() {
        // The whole reason this helper exists: a mod titled "Farms & Ranches" is stored as
        // `Farms &amp; Ranches` and must come back with the `&` — quick-xml 0.41 no longer
        // does that in `decode()`.
        let t = BytesText::from_escaped("Farms &amp; Ranches &lt;v2&gt;");
        assert_eq!(text(&t).as_deref(), Some("Farms & Ranches <v2>"));

        let plain = BytesText::from_escaped("no entities here");
        assert_eq!(text(&plain).as_deref(), Some("no entities here"));
    }

    #[test]
    fn resolves_predefined_and_character_references() {
        assert_eq!(reference(&BytesRef::new("amp")).as_deref(), Some("&"));
        assert_eq!(reference(&BytesRef::new("apos")).as_deref(), Some("'"));
        assert_eq!(reference(&BytesRef::new("#38")).as_deref(), Some("&"));
        assert_eq!(reference(&BytesRef::new("#xE9")).as_deref(), Some("é"));
        // Unknown entities stay visible instead of vanishing.
        assert_eq!(reference(&BytesRef::new("nbsp")).as_deref(), Some("&nbsp;"));
    }

    /// The reader really does split element text at each reference (#266); a `TextRun`
    /// puts it back together.
    #[test]
    fn text_run_rejoins_text_split_at_references() {
        let mut reader = Reader::from_str("<a>Elm &amp; Ash &#38; Co</a>");
        let mut run = TextRun::default();
        let mut pieces = 0;
        let mut out = None;
        loop {
            match reader.read_event().unwrap() {
                Event::Text(t) => {
                    pieces += 1;
                    run.push_text(&t);
                }
                Event::GeneralRef(r) => {
                    pieces += 1;
                    run.push_ref(&r);
                }
                Event::End(_) => out = Some(run.take()),
                Event::Eof => break,
                _ => {}
            }
        }
        assert!(pieces > 1, "quick-xml no longer splits text at references");
        assert_eq!(out.as_deref(), Some("Elm & Ash & Co"));
        assert!(run.is_empty());
    }
}
