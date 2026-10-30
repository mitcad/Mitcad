// SPDX-License-Identifier: MIT
//! XML access: parsing without DTDs (so no entity expansion: FreeCAD writes
//! none, and a file that has one is refused), attribute helpers, and an
//! owned copy of elements this reader keeps without interpreting.
//!
//! Nesting is bounded (guards against hostile files, whose nesting would
//! otherwise overflow the stack of the recursive XML parser and of the
//! recursive copies): elements deeper than [`XML_DEPTH`] are left out of
//! the text before it is parsed ([`shallow`]), and an [`Element`] copy
//! keeps [`ELEMENT_DEPTH`] levels.

use std::borrow::Cow;

use serde::Serialize;

pub(crate) type Node<'a, 'input> = roxmltree::Node<'a, 'input>;

/// The most nodes one XML file may have (a guard against hostile files;
/// the largest documents seen have a few million).
const NODES_LIMIT: u32 = 50_000_000;

/// How deep the elements of an XML file may nest; deeper ones are left out
/// when the text is read. FreeCAD's files nest about ten levels (the
/// deepest in the test corpora nine), so 256 leaves out nothing FreeCAD
/// writes, while the XML parser's recursion (a few frames per level) stays
/// far within a thread's stack.
pub const XML_DEPTH: usize = 256;

/// How many levels an [`Element`] copy keeps: deeper child elements are
/// left out (the element at the last level keeps its attributes and text).
/// The values kept as XML nest a few levels (the deepest in the test
/// corpora four); 64 keeps the copy's recursion (made, compared, cloned,
/// serialized, dropped) small.
pub const ELEMENT_DEPTH: usize = 64;

/// The XML text without the elements nested deeper than [`XML_DEPTH`]
/// (each left out with everything inside it), borrowed when there are
/// none; and whether any were left out. A scan without recursion: tags,
/// comments, CDATA sections, processing instructions and declarations,
/// quoted attribute values (which may hold `>`). Text that is not
/// well-formed stays so (the parser reports it).
pub(crate) fn shallow(text: &str) -> (Cow<'_, str>, bool) {
    let cuts = deep_ranges(text.as_bytes(), XML_DEPTH);
    if cuts.is_empty() {
        return (Cow::Borrowed(text), false);
    }
    let mut out = String::with_capacity(text.len());
    let mut from = 0;
    for (start, end) in cuts {
        out.push_str(&text[from..start]);
        from = end;
    }
    out.push_str(&text[from..]);
    (Cow::Owned(out), true)
}

/// The byte ranges of the elements that start deeper than `limit` (each
/// from its start tag to the end of its end tag), in order. Every range
/// starts at a `<` and ends after a `>` (or at the end), so cutting them
/// keeps the text valid UTF-8.
fn deep_ranges(b: &[u8], limit: usize) -> Vec<(usize, usize)> {
    let find = |from: usize, what: &[u8]| -> usize {
        b.get(from..)
            .and_then(|rest| rest.windows(what.len()).position(|w| w == what))
            .map_or(b.len(), |k| from + k + what.len())
    };
    let mut cuts = Vec::new();
    let mut depth = 0usize;
    let mut cut_from: Option<usize> = None;
    let mut i = 0;
    while let Some(k) = b[i..].iter().position(|&c| c == b'<') {
        let start = i + k;
        let rest = &b[start..];
        if rest.starts_with(b"<!--") {
            i = find(start + 4, b"-->");
        } else if rest.starts_with(b"<![CDATA[") {
            i = find(start + 9, b"]]>");
        } else if rest.starts_with(b"<?") {
            i = find(start + 2, b"?>");
        } else if rest.starts_with(b"<!") {
            // A document type declaration (refused when parsed).
            i = find(start + 2, b">");
        } else {
            // A tag: to its `>`, past quoted attribute values.
            let mut end = start + 1;
            let mut quote = None;
            while end < b.len() {
                let c = b[end];
                end += 1;
                match quote {
                    Some(q) if c == q => quote = None,
                    Some(_) => {}
                    None if c == b'"' || c == b'\'' => quote = Some(c),
                    None if c == b'>' => break,
                    None => {}
                }
            }
            i = end;
            if rest.starts_with(b"</") {
                if depth == limit + 1
                    && let Some(from) = cut_from.take()
                {
                    cuts.push((from, end));
                }
                depth = depth.saturating_sub(1);
            } else if end >= 2 && b[end - 1] == b'>' && b[end - 2] == b'/' {
                // Empty: `<a/>`.
                if depth == limit && cut_from.is_none() {
                    cuts.push((start, end));
                }
            } else {
                depth += 1;
                if depth == limit + 1 && cut_from.is_none() {
                    cut_from = Some(start);
                }
            }
        }
        if i >= b.len() {
            break;
        }
    }
    if let Some(from) = cut_from {
        cuts.push((from, b.len()));
    }
    cuts
}

/// Parses XML text (after [`shallow`]).
pub(crate) fn parse(text: &str) -> Result<roxmltree::Document<'_>, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: false,
        nodes_limit: NODES_LIMIT,
    };
    roxmltree::Document::parse_with_options(text, options).map_err(|e| e.to_string())
}

pub(crate) fn attr<'a>(node: &Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attribute(name)
}

pub(crate) fn attr_f64(node: &Node<'_, '_>, name: &str) -> Result<f64, String> {
    let text = attr(node, name).ok_or_else(|| missing(node, name))?;
    text.trim()
        .parse()
        .map_err(|_| format!("<{}> {name}=\"{text}\" is not a number", tag(node)))
}

pub(crate) fn attr_i64(node: &Node<'_, '_>, name: &str) -> Result<i64, String> {
    let text = attr(node, name).ok_or_else(|| missing(node, name))?;
    let text = text.trim();
    text.parse::<i64>()
        // Some integers are written as unsigned 64-bit (packed values).
        .or_else(|_| text.parse::<u64>().map(|v| v as i64))
        .map_err(|_| format!("<{}> {name}=\"{text}\" is not an integer", tag(node)))
}

fn missing(node: &Node<'_, '_>, name: &str) -> String {
    format!("<{}> has no {name}", tag(node))
}

fn tag<'a>(node: &Node<'a, '_>) -> &'a str {
    node.tag_name().name()
}

/// An XML element kept as it is: name, attributes in order, child
/// elements and text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Element {
    pub name: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<(String, String)>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Element>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub text: String,
}

impl Element {
    /// A copy of `node` with [`ELEMENT_DEPTH`] levels of elements (`node`
    /// the first).
    pub(crate) fn from_node(node: &Node<'_, '_>) -> Self {
        Element::copy(node, 1)
    }

    fn copy(node: &Node<'_, '_>, level: usize) -> Self {
        let mut text = String::new();
        let mut children = Vec::new();
        for child in node.children() {
            if child.is_element() {
                if level < ELEMENT_DEPTH {
                    children.push(Element::copy(&child, level + 1));
                }
            } else if let Some(t) = child.text() {
                let t = t.trim();
                if !t.is_empty() {
                    text.push_str(t);
                }
            }
        }
        Element {
            name: node.tag_name().name().to_owned(),
            attributes: node
                .attributes()
                .map(|a| (a.name().to_owned(), a.value().to_owned()))
                .collect(),
            children,
            text,
        }
    }

    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    /// The child elements named `name`.
    pub fn elements<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Element> + 'a {
        self.children.iter().filter(move |c| c.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text without the ranges `deep_ranges` cuts at `limit`.
    fn cut(text: &str, limit: usize) -> String {
        let mut out = String::new();
        let mut from = 0;
        for (start, end) in deep_ranges(text.as_bytes(), limit) {
            out.push_str(&text[from..start]);
            from = end;
        }
        out.push_str(&text[from..]);
        out
    }

    #[test]
    fn elements_past_the_limit_are_cut_with_their_contents() {
        let text = "<?xml version=\"1.0\"?><a><b x=\"1>2\" y='/>'><c>t<d/></c><e/></b>\
                    <!-- <b><c><d> --><f><![CDATA[<g><h>]]></f></a>";
        // Nothing past four levels (`<d/>` is the fourth): the text as it
        // is.
        assert_eq!(cut(text, 4), text);
        assert!(matches!(shallow(text), (Cow::Borrowed(_), false)));
        // Two levels: `<c>…</c>` and `<e/>` go; quoted `>` and `/>`,
        // comments and CDATA do not open elements.
        assert_eq!(
            cut(text, 2),
            "<?xml version=\"1.0\"?><a><b x=\"1>2\" y='/>'></b>\
             <!-- <b><c><d> --><f><![CDATA[<g><h>]]></f></a>"
        );
        assert_eq!(
            cut(text, 1),
            "<?xml version=\"1.0\"?><a><!-- <b><c><d> --></a>"
        );
        // An element left open is cut to the end (the parser then reports
        // the open elements).
        assert_eq!(cut("<a><b><c>", 1), "<a>");
    }

    #[test]
    fn hostile_nesting_parses_shallow() {
        let depth = 100_000;
        let text = format!("<a>{}x{}</a>", "<b>".repeat(depth), "</b>".repeat(depth));
        let (shallow_text, was_cut) = shallow(&text);
        assert!(was_cut);
        let xml = parse(&shallow_text).unwrap();
        let root = xml.root_element();
        let element = Element::from_node(&root);
        // The copy keeps ELEMENT_DEPTH levels; the last has no children.
        let mut levels = 1;
        let mut at = &element;
        while let Some(child) = at.children.first() {
            levels += 1;
            at = child;
        }
        assert_eq!(levels, ELEMENT_DEPTH);
        assert!(root.descendants().count() <= 2 * XML_DEPTH);
    }
}
