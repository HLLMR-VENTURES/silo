//! Shared helpers for pulling text out of quick-xml events.
//!
//! quick-xml 0.41 split the old `BytesText::unescape()` — which decoded the bytes AND
//! resolved XML entities in one call — into a byte `decode()` plus a separate
//! `escape::unescape()` step. It also stopped putting entity references inside `Text`
//! events: `Elm &amp; Ash` arrives as `Text("Elm ")`, `GeneralRef("amp")`, `Text(" Ash")`.
//! A parser that keeps only the first `Text` run cuts the value at the first entity
//! (#266), so element text is collected with [`TextRun`] and read when the element's
//! content ends.

use quick_xml::events::{BytesRef, BytesText};

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
