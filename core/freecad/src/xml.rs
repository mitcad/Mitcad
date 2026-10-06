// SPDX-License-Identifier: MIT
//! XML access: parsing without DTDs (so no entity expansion: FreeCAD writes
//! none, and a file that has one is refused), attribute helpers, and an
//! owned copy of elements this reader keeps without interpreting.

use serde::Serialize;

pub(crate) type Node<'a, 'input> = roxmltree::Node<'a, 'input>;

/// The most nodes one XML file may have (a guard against hostile files;
/// the largest documents seen have a few million).
const NODES_LIMIT: u32 = 50_000_000;

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
    pub(crate) fn from_node(node: &Node<'_, '_>) -> Self {
        let mut text = String::new();
        let mut children = Vec::new();
        for child in node.children() {
            if child.is_element() {
                children.push(Element::from_node(&child));
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
