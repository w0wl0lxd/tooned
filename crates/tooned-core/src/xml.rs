// SPDX-License-Identifier: AGPL-3.0-only

//! XML detection and conversion to `serde_json::Value` for the TOON pipeline.
//!
//! Detection is intentionally conservative: it returns `DocType::Xml` only when
//! the leading bytes strongly resemble XML and clearly are *not* HTML.
//! Parsing uses `quick-xml`'s streaming event reader with a configurable
//! element-depth guard and no external entity/DTD resolution.

use quick_xml::XmlVersion;
use quick_xml::events::Event;
use quick_xml::reader::Reader;
use serde_json::{Map, Value};

use crate::DocType;
use crate::parse::ParseError;

/// Tunables for the XML parser. Exposed publicly so tests and future callers
/// can adjust them without changing the crate API.
#[derive(Debug, Clone, PartialEq)]
pub struct XmlParseOptions {
    /// Maximum XML element nesting depth. Past this, parsing errors with
    /// [`ParseError::TooDeep`].
    pub max_depth: usize,
    /// Strip namespace prefixes from element and attribute names, keeping only
    /// the local name.
    pub strip_namespaces: bool,
    /// JSON key used for individual text nodes in a mixed-content array.
    pub mixed_content_key: String,
    /// Prefix applied to XML attributes in the JSONified representation.
    pub attribute_prefix: String,
    /// JSON key used for the text content of an element that also carries
    /// attributes.
    pub text_key: String,
}

impl Default for XmlParseOptions {
    fn default() -> Self {
        Self {
            max_depth: 100,
            strip_namespaces: true,
            mixed_content_key: "#text".to_string(),
            attribute_prefix: "@".to_string(),
            text_key: "$text".to_string(),
        }
    }
}

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// Skips a leading UTF-8 BOM and any leading ASCII whitespace.
fn skip_bom_and_whitespace(input: &[u8]) -> &[u8] {
    let after_bom = if let Some(rest) = input.strip_prefix(UTF8_BOM) { rest } else { input };
    let start = match after_bom.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(i) => i,
        None => after_bom.len(),
    };
    match after_bom.get(start..) {
        Some(rest) => rest,
        None => &[],
    }
}

fn is_xml_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_xml_name_char(b: u8) -> bool {
    is_xml_name_start(b) || b.is_ascii_digit() || b == b'-' || b == b'.'
}

/// Returns the first contiguous XML name bytes in `s` (after the leading `<`),
/// or `None` if the next byte is not a valid name-start character.
fn element_tag_name(s: &[u8]) -> Option<&[u8]> {
    if s.first().copied().is_none_or(|b| !is_xml_name_start(b)) {
        return None;
    }
    let len = match s.iter().position(|b| !is_xml_name_char(*b)) {
        Some(i) => i,
        None => s.len(),
    };
    s.get(..len)
}

const HTML_TAGS: &[&[u8]] = &[
    b"html",
    b"head",
    b"body",
    b"div",
    b"span",
    b"p",
    b"a",
    b"script",
    b"style",
    b"table",
    b"tr",
    b"td",
    b"th",
    b"thead",
    b"tbody",
    b"tfoot",
    b"ul",
    b"ol",
    b"li",
    b"h1",
    b"h2",
    b"h3",
    b"h4",
    b"h5",
    b"h6",
    b"form",
    b"input",
    b"button",
    b"select",
    b"option",
    b"textarea",
    b"label",
    b"img",
    b"br",
    b"hr",
    b"meta",
    b"link",
    b"title",
    b"base",
    b"nav",
    b"section",
    b"article",
    b"aside",
    b"header",
    b"footer",
    b"main",
    b"figure",
    b"figcaption",
    b"pre",
    b"code",
    b"strong",
    b"em",
    b"b",
    b"i",
    b"u",
    b"s",
    b"small",
    b"sub",
    b"sup",
    b"mark",
    b"q",
    b"blockquote",
    b"cite",
    b"time",
    b"address",
    b"details",
    b"summary",
    b"dialog",
    b"canvas",
    b"video",
    b"audio",
    b"source",
    b"track",
    b"embed",
    b"object",
    b"param",
    b"iframe",
    b"frame",
    b"frameset",
    b"noframes",
    b"applet",
    b"marquee",
    b"font",
    b"center",
];

fn is_html_tag(name: &[u8]) -> bool {
    let lower = name.to_ascii_lowercase();
    HTML_TAGS.iter().any(|tag| **tag == lower)
}

/// True when `input` begins with `<!DOCTYPE` (case-insensitive) followed by an
/// ASCII-whitespace-separated token that starts with `html` (also
/// case-insensitive).
fn is_html_doctype(input: &[u8]) -> bool {
    let s = skip_bom_and_whitespace(input);
    let Some(after_bang) = s.strip_prefix(b"<!") else {
        return false;
    };
    let keyword_end = match after_bang.iter().position(|b| !b.is_ascii_alphabetic()) {
        Some(i) => i,
        None => after_bang.len(),
    };
    let Some(keyword) = after_bang.get(..keyword_end) else {
        return false;
    };
    if !keyword.eq_ignore_ascii_case(b"doctype") {
        return false;
    }
    let Some(tail) = after_bang.get(keyword_end..) else {
        return false;
    };
    let after_ws = match tail.iter().position(|b| !b.is_ascii_whitespace()) {
        Some(i) => match tail.get(i..) {
            Some(r) => r,
            None => &[],
        },
        None => &[],
    };
    if after_ws.len() < 4 {
        return false;
    }
    let Some(html_prefix) = after_ws.get(0..4) else {
        return false;
    };
    html_prefix.eq_ignore_ascii_case(b"html")
        && after_ws.get(4).is_none_or(|b| !is_xml_name_char(*b))
}

/// Conservative XML sniffer. Returns `Some(DocType::Xml)` only when the input
/// starts with a clear XML declaration, DOCTYPE, or element-start and is not an
/// HTML document.
pub fn sniff(input: &[u8]) -> Option<DocType> {
    let s = skip_bom_and_whitespace(input);
    if s.is_empty() {
        return None;
    }

    if is_html_doctype(input) {
        return None;
    }

    if s.starts_with(b"<?xml") || s.starts_with(b"<?XML") {
        return Some(DocType::Xml);
    }

    if s.starts_with(b"<!DOCTYPE") || s.starts_with(b"<!doctype") {
        return Some(DocType::Xml);
    }

    if s.first().copied() == Some(b'<') {
        let after_lt = match s.get(1..) {
            Some(rest) => rest,
            None => &[],
        };
        // End tags, comments, PIs and generic declarations have their own
        // recognizers above or are too ambiguous to treat as XML on their own.
        let second = after_lt.first().copied();
        if second == Some(b'/') || second == Some(b'?') || second == Some(b'!') {
            return None;
        }
        if let Some(name) = element_tag_name(after_lt) {
            if is_html_tag(name) {
                return None;
            }
            return Some(DocType::Xml);
        }
    }

    None
}

/// Iterative depth guard that counts `<`/`>` nesting while ignoring contents of
/// CDATA sections, comments, and double/single-quoted strings. This mirrors the
/// bracket guard used for JSON/YAML/TOML but for XML element syntax.
fn exceeds_max_depth(input: &[u8], max_depth: usize) -> bool {
    #[derive(Clone, Copy)]
    enum State {
        Outside,
        InTag,
        InQuote(u8),
    }

    let mut depth: usize = 0;
    let mut state = State::Outside;
    let mut i = 0;

    while let Some(b) = input.get(i).copied() {
        match state {
            State::Outside => {
                if b == b'<' {
                    // Skip comments: <!-- ... -->
                    if input.get(i + 1..i + 4) == Some(b"!--") {
                        let Some(tail) = input.get(i + 4..) else {
                            break;
                        };
                        if let Some(end) = tail.windows(3).position(|w| w == b"-->") {
                            i += 4 + end + 3;
                            continue;
                        }
                        break;
                    }
                    // Skip CDATA sections entirely so brackets inside do not count.
                    if input.get(i + 1..i + 9) == Some(b"![CDATA[") {
                        let Some(tail) = input.get(i + 9..) else {
                            break;
                        };
                        if let Some(end) = tail.windows(3).position(|w| w == b"]]>") {
                            i += 9 + end + 3;
                            continue;
                        }
                        break;
                    }
                    depth += 1;
                    if depth > max_depth {
                        return true;
                    }
                    state = State::InTag;
                }
                i += 1;
            }
            State::InTag => {
                if b == b'"' || b == b'\'' {
                    state = State::InQuote(b);
                } else if b == b'>' {
                    depth = depth.saturating_sub(1);
                    state = State::Outside;
                }
                i += 1;
            }
            State::InQuote(q) => {
                if b == q {
                    state = State::InTag;
                }
                i += 1;
            }
        }
    }
    false
}

/// A single child of an element frame, preserving the order in which it was
/// encountered.
enum ChildNode {
    Text(String),
    Element(String, Value),
}

/// Partially-built element used while walking the XML event stream.
struct ElementFrame {
    name: String,
    attrs: Map<String, Value>,
    children: Vec<ChildNode>,
}

/// Parses `input` as XML using the default options.
pub fn parse(input: &[u8]) -> Result<Value, ParseError> {
    parse_with_options(input, &XmlParseOptions::default())
}

fn parse_with_options(input: &[u8], opts: &XmlParseOptions) -> Result<Value, ParseError> {
    if exceeds_max_depth(input, opts.max_depth) {
        return Err(ParseError::TooDeep);
    }

    let mut reader = Reader::from_reader(input);

    let mut stack: Vec<ElementFrame> = Vec::with_capacity(opts.max_depth);
    let mut root: Option<Value> = None;
    let version = XmlVersion::Implicit1_0;

    loop {
        let event = reader.read_event().map_err(|e| ParseError::Xml(e.to_string()))?;
        match event {
            Event::Decl(_) | Event::Comment(_) | Event::PI(_) | Event::DocType(_) => {}
            Event::GeneralRef(e) => {
                if let Some(ch) =
                    e.resolve_char_ref().map_err(|err| ParseError::Xml(err.to_string()))?
                {
                    push_text(&mut stack, ch.to_string())?;
                } else {
                    let name = std::str::from_utf8(e.as_ref()).map_err(|_| ParseError::Utf8)?;
                    let text = if let Some(s) = quick_xml::escape::resolve_predefined_entity(name) {
                        s.to_string()
                    } else {
                        format!("&{name};")
                    };
                    push_text(&mut stack, text)?;
                }
            }
            Event::Start(e) => {
                let name = local_name(&e, opts)?;
                let attrs = collect_attrs(&e, opts, version)?;
                stack.push(ElementFrame { name, attrs, children: Vec::new() });
                if stack.len() > opts.max_depth {
                    return Err(ParseError::TooDeep);
                }
            }
            Event::Empty(e) => {
                let name = local_name(&e, opts)?;
                let attrs = collect_attrs(&e, opts, version)?;
                let frame = ElementFrame { name: name.clone(), attrs, children: Vec::new() };
                let value = finalize_element(frame, opts);
                insert_child_or_root(&mut stack, &mut root, name, value)?;
            }
            Event::End(e) => {
                let expected = end_name(&e, opts)?;
                let frame = stack.pop().ok_or_else(|| {
                    ParseError::Xml(format!("unexpected closing tag </{expected}>"))
                })?;
                if frame.name != expected {
                    return Err(ParseError::Xml(format!(
                        "mismatched tags: opened <{}>, closed </{expected}>",
                        frame.name
                    )));
                }
                let name = frame.name.clone();
                let value = finalize_element(frame, opts);
                insert_child_or_root(&mut stack, &mut root, name, value)?;
            }
            Event::Text(e) => {
                let text = text_value(&e)?;
                push_text(&mut stack, text)?;
            }
            Event::CData(e) => {
                let text = cdata_value(&e)?;
                push_text(&mut stack, text)?;
            }
            Event::Eof => break,
        }
    }

    root.ok_or_else(|| ParseError::Xml("no root element found".to_string()))
}

fn collect_attrs(
    e: &quick_xml::events::BytesStart<'_>,
    opts: &XmlParseOptions,
    version: XmlVersion,
) -> Result<Map<String, Value>, ParseError> {
    let mut attrs = Map::new();
    for attr in e.attributes() {
        let attr = attr.map_err(|e| ParseError::Xml(e.to_string()))?;
        if is_namespace_attr(&attr) {
            continue;
        }
        let key = attr_key(&attr, opts)?;
        let value = attr_value(&attr, version)?;
        attrs.insert(key, Value::String(value));
    }
    Ok(attrs)
}

/// Returns true for namespace-declaration attributes (`xmlns` or `xmlns:*`).
fn is_namespace_attr(attr: &quick_xml::events::attributes::Attribute<'_>) -> bool {
    let raw = attr.key.as_ref();
    raw == b"xmlns" || raw.starts_with(b"xmlns:")
}

fn insert_child_or_root(
    stack: &mut [ElementFrame],
    root: &mut Option<Value>,
    name: String,
    value: Value,
) -> Result<(), ParseError> {
    if let Some(parent) = stack.last_mut() {
        parent.children.push(ChildNode::Element(name, value));
        Ok(())
    } else if root.is_some() {
        Err(ParseError::Xml("multiple root elements".to_string()))
    } else {
        let mut m = Map::new();
        m.insert(name, value);
        *root = Some(Value::Object(m));
        Ok(())
    }
}

fn push_text(stack: &mut [ElementFrame], text: String) -> Result<(), ParseError> {
    if text.trim().is_empty() {
        return Ok(());
    }
    if let Some(parent) = stack.last_mut() {
        parent.children.push(ChildNode::Text(text));
        Ok(())
    } else {
        Err(ParseError::Xml("text outside root element".to_string()))
    }
}

fn local_name(
    e: &quick_xml::events::BytesStart<'_>,
    opts: &XmlParseOptions,
) -> Result<String, ParseError> {
    if opts.strip_namespaces {
        std::str::from_utf8(e.local_name().into_inner())
            .map_err(|_| ParseError::Utf8)
            .map(String::from)
    } else {
        let name = e.name();
        std::str::from_utf8(name.as_ref()).map_err(|_| ParseError::Utf8).map(String::from)
    }
}

fn end_name(
    e: &quick_xml::events::BytesEnd<'_>,
    opts: &XmlParseOptions,
) -> Result<String, ParseError> {
    if opts.strip_namespaces {
        std::str::from_utf8(e.local_name().into_inner())
            .map_err(|_| ParseError::Utf8)
            .map(String::from)
    } else {
        let name = e.name();
        std::str::from_utf8(name.as_ref()).map_err(|_| ParseError::Utf8).map(String::from)
    }
}

fn attr_key(
    attr: &quick_xml::events::attributes::Attribute<'_>,
    opts: &XmlParseOptions,
) -> Result<String, ParseError> {
    let bytes =
        if opts.strip_namespaces { attr.key.local_name().into_inner() } else { attr.key.as_ref() };
    let name = std::str::from_utf8(bytes).map_err(|_| ParseError::Utf8)?;
    let mut result = String::with_capacity(opts.attribute_prefix.len() + name.len());
    result.push_str(&opts.attribute_prefix);
    result.push_str(name);
    Ok(result)
}

fn attr_value(
    attr: &quick_xml::events::attributes::Attribute<'_>,
    version: XmlVersion,
) -> Result<String, ParseError> {
    let value = attr.normalized_value(version).map_err(|e| ParseError::Xml(e.to_string()))?;
    Ok(value.into_owned())
}

fn text_value(e: &quick_xml::events::BytesText<'_>) -> Result<String, ParseError> {
    let decoded = e.xml10_content().map_err(|e| ParseError::Xml(e.to_string()))?;
    let unescaped =
        quick_xml::escape::unescape(&decoded).map_err(|e| ParseError::Xml(e.to_string()))?;
    Ok(unescaped.into_owned())
}

fn cdata_value(e: &quick_xml::events::BytesCData<'_>) -> Result<String, ParseError> {
    let value = e.xml10_content().map_err(|e| ParseError::Xml(e.to_string()))?;
    Ok(value.into_owned())
}

fn finalize_element(frame: ElementFrame, opts: &XmlParseOptions) -> Value {
    let has_text = frame.children.iter().any(|c| matches!(c, ChildNode::Text(_)));
    let has_elements = frame.children.iter().any(|c| matches!(c, ChildNode::Element(_, _)));

    if has_elements && has_text {
        // Mixed content: preserve order as an array of {"#text": ...} and
        // {"tag": value} nodes. If attributes are present, wrap them around a
        // "$mixed" array so the JSON shape stays unambiguous.
        let mixed: Vec<Value> = frame
            .children
            .into_iter()
            .map(|c| match c {
                ChildNode::Text(t) => {
                    let mut m = Map::new();
                    m.insert(opts.mixed_content_key.clone(), Value::String(t));
                    Value::Object(m)
                }
                ChildNode::Element(tag, value) => {
                    let mut m = Map::new();
                    m.insert(tag, value);
                    Value::Object(m)
                }
            })
            .collect();
        if frame.attrs.is_empty() {
            Value::Array(mixed)
        } else {
            let mut obj = frame.attrs;
            obj.insert("$mixed".to_string(), Value::Array(mixed));
            Value::Object(obj)
        }
    } else if has_elements {
        let mut grouped: Map<String, Value> = Map::new();
        for child in frame.children {
            if let ChildNode::Element(tag, value) = child {
                append_or_insert(&mut grouped, tag, value);
            }
        }
        let mut obj = frame.attrs;
        for (k, v) in grouped {
            obj.insert(k, v);
        }
        Value::Object(obj)
    } else if has_text {
        let text: String = frame
            .children
            .into_iter()
            .filter_map(|c| match c {
                ChildNode::Text(t) => Some(t),
                ChildNode::Element(_, _) => None,
            })
            .collect();
        if frame.attrs.is_empty() {
            Value::String(text)
        } else {
            let mut obj = frame.attrs;
            obj.insert(opts.text_key.clone(), Value::String(text));
            Value::Object(obj)
        }
    } else if frame.attrs.is_empty() {
        Value::Object(Map::new())
    } else {
        Value::Object(frame.attrs)
    }
}

fn append_or_insert(map: &mut Map<String, Value>, key: String, value: Value) {
    if let Some(existing) = map.get_mut(&key) {
        if let Value::Array(arr) = existing {
            arr.push(value);
        } else {
            let old = std::mem::take(existing);
            *existing = Value::Array(vec![old, value]);
        }
    } else {
        map.insert(key, value);
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn detects_xml_declaration() {
        assert_eq!(sniff(b"<?xml version=\"1.0\"?><r/>"), Some(DocType::Xml));
    }

    #[test]
    fn detects_doctype() {
        assert_eq!(sniff(b"<!DOCTYPE root><root/>"), Some(DocType::Xml));
    }

    #[test]
    fn detects_simple_root_element() {
        assert_eq!(sniff(b"<root></root>"), Some(DocType::Xml));
        assert_eq!(sniff(b"<root/>"), Some(DocType::Xml));
    }

    #[test]
    fn detects_repeated_element_as_xml() {
        assert_eq!(sniff(b"<items><item/><item/></items>"), Some(DocType::Xml));
    }

    #[test]
    fn rejects_html_doctype() {
        assert_eq!(sniff(b"<!DOCTYPE html><html/>"), None);
        assert_eq!(sniff(b"<!DOCTYPE HTML><html/>"), None);
    }

    #[test]
    fn rejects_common_html_tags() {
        assert_eq!(sniff(b"<html></html>"), None);
        assert_eq!(sniff(b"<head></head>"), None);
        assert_eq!(sniff(b"<body></body>"), None);
        assert_eq!(sniff(b"<div></div>"), None);
        assert_eq!(sniff(b"<script></script>"), None);
        assert_eq!(sniff(b"<table></table>"), None);
        assert_eq!(sniff(b"<p>hello</p>"), None);
        assert_eq!(sniff(b"<a href='x'>link</a>"), None);
    }

    #[test]
    fn rejects_markdown_and_plain_text_false_positives() {
        assert_eq!(sniff(b"this is <not xml"), None);
        assert_eq!(sniff(b"value < 10 and value > 5"), None);
        assert_eq!(sniff(b"< - arrow"), None);
        assert_eq!(sniff(b"look: < xml-ish"), None);
        assert_eq!(sniff(b""), None);
    }

    #[test]
    fn tolerates_bom_and_whitespace() {
        assert_eq!(sniff(b"\xEF\xBB\xBF<?xml version=\"1.0\"?><r/>"), Some(DocType::Xml));
        assert_eq!(sniff(b"\n\t  \r<root></root>"), Some(DocType::Xml));
    }

    #[test]
    fn parses_declaration_doctype_and_root() {
        let xml = b"<?xml version=\"1.0\"?><!DOCTYPE root><root/>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": {}}));
    }

    #[test]
    fn parses_attributes_prefixed_with_at() {
        let xml = b"<root id=\"1\" enabled=\"true\"/>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": {"@id": "1", "@enabled": "true"}}));
    }

    #[test]
    fn parses_repeated_children_as_arrays() {
        let xml = b"<root><item>1</item><item>2</item><item>3</item></root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": {"item": ["1", "2", "3"]}}));
    }

    #[test]
    fn parses_text_only_element_as_string() {
        let xml = b"<root>hello world</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "hello world"}));
    }

    #[test]
    fn parses_element_with_attributes_and_text() {
        let xml = b"<root id=\"1\">hello</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": {"@id": "1", "$text": "hello"}}));
    }

    #[test]
    fn parses_mixed_content_as_ordered_array() {
        let xml = b"<p>Hello <b>world</b>!</p>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(
            value,
            json!({
                "p": [
                    {"#text": "Hello "},
                    {"b": "world"},
                    {"#text": "!"}
                ]
            })
        );
    }

    #[test]
    fn parses_cdata_as_text() {
        let xml = b"<root><![CDATA[some <unescaped> content]]></root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "some <unescaped> content"}));
    }

    #[test]
    fn ignores_comments_and_pis() {
        let xml = b"<?xml version=\"1.0\"?><!-- a comment --><?pi value?><root>ok</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "ok"}));
    }

    #[test]
    fn strips_namespace_prefixes() {
        let xml = b"<ns:root xmlns:ns='http://x'><ns:child ns:attr='v'/></ns:root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(
            value,
            json!({
                "root": {
                    "child": {"@attr": "v"}
                }
            })
        );
    }

    #[test]
    fn depth_guard_rejects_adversarially_nested_xml() {
        let mut xml = Vec::new();
        for _ in 0..110 {
            xml.extend_from_slice(b"<a>");
        }
        for _ in 0..110 {
            xml.extend_from_slice(b"</a>");
        }
        let result = parse(&xml);
        assert!(matches!(result, Err(ParseError::TooDeep)), "expected TooDeep, got {result:?}");
    }

    #[test]
    fn depth_guard_ignores_brackets_in_cdata() {
        let xml = b"<a><![CDATA[ <<<<<<< >>>>>>> ]]></a>";
        assert!(!exceeds_max_depth(xml, 2));
    }

    #[test]
    fn depth_guard_ignores_brackets_in_quoted_strings() {
        let xml = br#"<a b="<<<<<<< >>>>>>>"></a>"#;
        assert!(!exceeds_max_depth(xml, 2));
    }

    #[test]
    fn invalid_utf8_is_an_error_not_panic() {
        let xml = [0xFF, 0xFE, 0x7B];
        assert!(parse(&xml).is_err());
    }

    #[test]
    fn truncated_xml_is_an_error() {
        let xml = b"<?xml version=\"1.0\"?><root>";
        assert!(parse(xml).is_err());
    }

    #[test]
    fn preserves_custom_entity_references_as_literal_text() {
        let xml = b"<?xml version=\"1.0\"?><!DOCTYPE root [<!ENTITY foo \"hello\">]><root>before&foo;after</root>";
        let value = parse(xml).expect("valid XML with entity refs must parse");
        assert_eq!(value, json!({"root": "before&foo;after"}));
    }

    #[test]
    fn preserves_xml_prefixed_attributes() {
        let xml = b"<root xml:lang=\"en\">hello</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": {"@lang": "en", "$text": "hello"}}));
    }

    #[test]
    fn rejects_html_doctype_without_trailing_tag_characters() {
        assert_eq!(sniff(b"<!DOCTYPE html>"), None);
    }

    #[test]
    fn resolves_predefined_entities_in_text() {
        let xml = b"<root>a&lt;b</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "a<b"}));
    }

    #[test]
    fn resolves_numeric_character_references_in_text() {
        let xml = b"<root>&#65; and &#x42;</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "A and B"}));
    }

    #[test]
    fn resolves_amp_and_quote_entities_in_text() {
        let xml = b"<root>a&amp;b&quot;c&apos;</root>";
        let value = parse(xml).expect("valid XML must parse");
        assert_eq!(value, json!({"root": "a&b\"c'"}));
    }
}
