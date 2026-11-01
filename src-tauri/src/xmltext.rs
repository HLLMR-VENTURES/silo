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
    // XML 1.0's Char production: tab, LF, CR, and everything else above the C0 controls
    // except surrogates and U+FFFE/U+FFFF.
    match code {
        Some(0x9 | 0xA | 0xD | 0x20..=0xD7FF | 0xE000..=0xFFFD | 0x10000..=0x10FFFF) => Ok(()),
        _ => Err(format!("&{name}; isn't a valid character reference")),
    }
}

/// Check every attribute of a tag the way the game's parser does: each one quoted and named
/// once (`with_checks`), no `<` in a value, and every `&` starting a reference that
/// [`check_reference`] accepts.
pub fn check_attributes(e: &BytesStart) -> Result<(), String> {
    let mut attrs = e.attributes();
    attrs.with_checks(true);
    for a in attrs {
        let a = a.map_err(|err| err.to_string())?;
        let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
        let value = String::from_utf8_lossy(&a.value);
        if value.contains('<') {
            return Err(format!("the value of {key} contains '<'"));
        }
        let mut rest = value.as_ref();
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

/// Rebuild `e` with some attribute values replaced. `replacement` is asked once per
/// attribute, in order, and returns the new value for an attribute being edited. Every
/// other attribute keeps its raw bytes, so its character references (`&#10;`) and entities
/// survive; so does an edited one whose new value is what a reader already sees. A
/// malformed or duplicated attribute is an error: never dropped, never written back empty.
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
            Some(v) => Cow::Owned(escape_attr(v).into_bytes()),
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
