// SPDX-License-Identifier: MIT
//! Property values: the XML a FreeCAD property writes inside its
//! `<Property>` element, read by the element's name (several property types
//! share one form: lengths, angles and distances are all `<Float>`).
//!
//! Lists that FreeCAD writes into files of their own in the archive
//! (`<PlacementList file=…>`, `<ColorList file=…>`, …) are first read as
//! [`Value::File`] and then replaced by their values
//! ([`crate::Document::resolve_files`], [`crate::binary`]).

use serde::Serialize;

use crate::placement::Placement;
use crate::xml::{Element, Node, attr, attr_f64, attr_i64};

/// A reference to an object of this or another document, with names of
/// elements of it (faces, edges, sub-objects).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LinkRef {
    /// The object's name (`Pad`), not its label.
    pub object: String,
    /// Another document, by the path saved in the file (usually relative
    /// to the linking document's folder).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub subs: Vec<SubName>,
}

/// The name of an element of a linked object: an index name (`Face6`, the
/// sixth face of its stored shape), or a path through sub-objects
/// (`Body.Pad.Face3`), with FreeCAD 1.0's mapped (topological) name when
/// it wrote one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubName {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mapped: Option<String>,
}

/// An enumeration's value: an index into the property's list of values,
/// which the file holds only for custom lists; others come from the version
/// tables ([`crate::version::enum_values`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Enumeration {
    pub index: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom: Option<Vec<String>>,
}

/// An expression bound to a property path (`Length`, `Placement.Base.z`,
/// `Constraints.width`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Expression {
    pub path: String,
    pub expression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// A stored shape (`Part::PropertyPartShape`): its file in the archive,
/// OCCT's B-rep (text, or binary for `.bin`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShapeRef {
    /// None or empty: no shape (or one written into the XML, which this
    /// reader does not take).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// FreeCAD 1.0's element map version, when the shape has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_map: Option<String>,
}

/// An RGBA colour, components in [0, 1]. FreeCAD packs colours as
/// `r << 24 | g << 16 | b << 8 | a` in a 32-bit integer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Color(pub [f64; 4]);

impl Color {
    pub fn from_packed(packed: u32) -> Self {
        let byte = |shift: u32| f64::from((packed >> shift) & 0xff) / 255.0;
        Color([byte(24), byte(16), byte(8), byte(0)])
    }

    pub fn rgb(&self) -> [f64; 3] {
        [self.0[0], self.0[1], self.0[2]]
    }

    /// The RGB components as bytes.
    pub fn rgb8(&self) -> [u8; 3] {
        self.rgb()
            .map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
    }
}

/// A material of a view (`App::PropertyMaterial`, and the entries of 1.0's
/// `App::PropertyMaterialList`, the shape appearance).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Material {
    pub ambient: Color,
    pub diffuse: Color,
    pub specular: Color,
    pub emissive: Color,
    pub shininess: f64,
    pub transparency: f64,
    /// The material library entry it came from, when any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uuid: Option<String>,
}

/// A property's value.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum Value {
    Integer(i64),
    Enumeration(Enumeration),
    /// Floats and quantities: lengths in millimetres, `PropertyAngle` in
    /// degrees.
    Float(f64),
    Bool(bool),
    String(String),
    Uuid(String),
    IntegerList(Vec<i64>),
    FloatList(Vec<f64>),
    StringList(Vec<String>),
    BoolList(Vec<bool>),
    Vector([f64; 3]),
    VectorList(Vec<[f64; 3]>),
    Placement(Placement),
    PlacementList(Vec<Placement>),
    /// One reference (`Link`, `LinkSub`, `XLink`, `XLinkSub`); None when
    /// empty.
    Link(Option<LinkRef>),
    /// Several (`LinkList`, `LinkSubList`, `XLinkList`, `XLinkSubList`).
    LinkList(Vec<LinkRef>),
    Color(Color),
    ColorList(Vec<Color>),
    Material(Material),
    MaterialList(Vec<Material>),
    Map(Vec<(String, String)>),
    /// A Part fillet's or chamfer's edges: the edge's number (`Edge<n>`)
    /// and the first and second radius or size.
    FilletEdges(Vec<(i64, f64, f64)>),
    Expressions(Vec<Expression>),
    Shape(ShapeRef),
    /// Data in a file of the archive not read yet (or of a kind this reader
    /// does not decode): the element's name and the file.
    File {
        element: String,
        file: String,
    },
    /// Any other content, kept as it is (sketch geometry and constraints,
    /// spreadsheet cells, Python objects…), for later stages.
    Xml(Element),
    /// No value: a transient property (`<_Property>`), or an empty element.
    None,
}

impl Value {
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Float(v) => Some(*v),
            Value::Integer(v) => Some(*v as f64),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::Integer(v) => Some(*v),
            Value::Enumeration(e) => Some(e.index),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::String(v) | Value::Uuid(v) => Some(v),
            _ => None,
        }
    }

    /// The references a value holds (one, several or none).
    pub fn links(&self) -> &[LinkRef] {
        match self {
            Value::Link(Some(link)) => std::slice::from_ref(link),
            Value::LinkList(links) => links,
            _ => &[],
        }
    }
}

/// The value inside a `<Property>` element: its first child element, with
/// the siblings some forms have (a custom enumeration's list).
pub(crate) fn parse_value(property: &Node<'_, '_>) -> Result<Value, String> {
    let mut elements = property.children().filter(|n| n.is_element());
    let Some(first) = elements.next() else {
        return Ok(Value::None);
    };
    let name = first.tag_name().name();
    let f = |key: &str| attr_f64(&first, key);
    Ok(match name {
        "Integer" => {
            let index = attr_i64(&first, "value")?;
            if attr(&first, "CustomEnum") == Some("true") {
                let list = elements
                    .find(|n| n.tag_name().name() == "CustomEnumList")
                    .map(|l| {
                        l.children()
                            .filter(|n| n.tag_name().name() == "Enum")
                            .map(|n| attr(&n, "value").unwrap_or_default().to_owned())
                            .collect()
                    })
                    .unwrap_or_default();
                Value::Enumeration(Enumeration {
                    index,
                    custom: Some(list),
                })
            } else {
                Value::Integer(index)
            }
        }
        "Float" => Value::Float(f("value")?),
        "Bool" => Value::Bool(matches!(attr(&first, "value"), Some("true" | "1"))),
        "String" => Value::String(attr(&first, "value").unwrap_or_default().to_owned()),
        "Uuid" => Value::Uuid(attr(&first, "value").unwrap_or_default().to_owned()),
        "PropertyVector" => Value::Vector([f("valueX")?, f("valueY")?, f("valueZ")?]),
        "PropertyPlacement" => Value::Placement(Placement::from_quaternion(
            [f("Px")?, f("Py")?, f("Pz")?],
            [f("Q0")?, f("Q1")?, f("Q2")?, f("Q3")?],
        )),
        "PropertyColor" => Value::Color(Color::from_packed(packed(&first, "value")?)),
        // A view's material; 1.0's material of a shape (a library entry)
        // has the same element with other attributes and is kept as XML.
        "PropertyMaterial" if attr(&first, "ambientColor").is_some() => Value::Material(Material {
            ambient: Color::from_packed(packed(&first, "ambientColor")?),
            diffuse: Color::from_packed(packed(&first, "diffuseColor")?),
            specular: Color::from_packed(packed(&first, "specularColor")?),
            emissive: Color::from_packed(packed(&first, "emissiveColor")?),
            shininess: f("shininess")?,
            transparency: f("transparency")?,
            uuid: attr(&first, "uuid")
                .filter(|u| !u.is_empty())
                .map(str::to_owned),
        }),
        "IntegerList" => Value::IntegerList(
            items(&first, "I")
                .map(|n| attr_i64(&n, "v"))
                .collect::<Result<_, _>>()?,
        ),
        "FloatList" | "StringList" | "PlacementList" | "VectorList" | "ColorList"
        | "MaterialList" | "FilletEdges"
            if attr(&first, "file").is_some_and(|f| !f.is_empty()) =>
        {
            Value::File {
                element: name.to_owned(),
                file: attr(&first, "file").unwrap_or_default().to_owned(),
            }
        }
        "FloatList" => Value::FloatList(
            items(&first, "F")
                .map(|n| attr_f64(&n, "v"))
                .collect::<Result<_, _>>()?,
        ),
        "StringList" => Value::StringList(
            items(&first, "String")
                .map(|n| attr(&n, "value").unwrap_or_default().to_owned())
                .collect(),
        ),
        "PlacementList" => Value::PlacementList(Vec::new()),
        "VectorList" => Value::VectorList(Vec::new()),
        "ColorList" => Value::ColorList(Vec::new()),
        "MaterialList" => Value::MaterialList(Vec::new()),
        // A bit string with the first element last (`1011`: elements 0, 1
        // and 3 set).
        "BoolList" => Value::BoolList(
            attr(&first, "value")
                .unwrap_or_default()
                .chars()
                .rev()
                .map(|c| c == '1')
                .collect(),
        ),
        "Map" => Value::Map(
            items(&first, "Item")
                .map(|n| {
                    (
                        attr(&n, "key").unwrap_or_default().to_owned(),
                        attr(&n, "value").unwrap_or_default().to_owned(),
                    )
                })
                .collect(),
        ),
        "Link" => Value::Link(object_ref(attr(&first, "value"), None)),
        "LinkList" => Value::LinkList(
            items(&first, "Link")
                .filter_map(|n| object_ref(attr(&n, "value"), None))
                .collect(),
        ),
        "LinkSub" => Value::Link(object_ref(attr(&first, "value"), None).map(|mut link| {
            link.subs = subs(&first);
            link
        })),
        "LinkSubList" => Value::LinkList(link_sub_list(&first)),
        "XLink" | "XLinkSub" => Value::Link(xlink(&first)),
        "XLinkList" | "XLinkSubList" => Value::LinkList(
            first
                .children()
                .filter(|n| n.is_element())
                .filter_map(|n| match n.tag_name().name() {
                    "XLink" | "XLinkSub" => xlink(&n),
                    _ => None,
                })
                .collect(),
        ),
        "ExpressionEngine" => Value::Expressions(
            items(&first, "Expression")
                .map(|n| Expression {
                    path: attr(&n, "path").unwrap_or_default().to_owned(),
                    expression: attr(&n, "expression").unwrap_or_default().to_owned(),
                    comment: attr(&n, "comment")
                        .filter(|c| !c.is_empty())
                        .map(str::to_owned),
                })
                .collect(),
        ),
        "Part" => Value::Shape(ShapeRef {
            file: attr(&first, "file")
                .filter(|f| !f.is_empty())
                .map(str::to_owned),
            element_map: attr(&first, "ElementMap").map(str::to_owned),
        }),
        _ => Value::Xml(Element::from_node(&first)),
    })
}

fn packed(node: &Node<'_, '_>, key: &str) -> Result<u32, String> {
    let value = attr_i64(node, key)?;
    u32::try_from(value).map_err(|_| format!("{key}=\"{value}\" is not a packed colour"))
}

/// The child elements of `node` named `name`.
fn items<'a, 'input>(
    node: &Node<'a, 'input>,
    name: &'static str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children()
        .filter(move |n| n.is_element() && n.tag_name().name() == name)
}

fn object_ref(name: Option<&str>, file: Option<&str>) -> Option<LinkRef> {
    let name = name.filter(|n| !n.is_empty())?;
    Some(LinkRef {
        object: name.to_owned(),
        file: file.filter(|f| !f.is_empty()).map(str::to_owned),
        subs: Vec::new(),
    })
}

/// A mapped name: 1.0's `shadowed` attribute, else `shadow`.
fn mapped(node: &Node<'_, '_>) -> Option<String> {
    attr(node, "shadowed")
        .or_else(|| attr(node, "shadow"))
        .filter(|m| !m.is_empty())
        .map(str::to_owned)
}

/// `<Sub value=… [shadowed=…]/>` children.
fn subs(node: &Node<'_, '_>) -> Vec<SubName> {
    items(node, "Sub")
        .map(|n| SubName {
            name: attr(&n, "value").unwrap_or_default().to_owned(),
            mapped: mapped(&n),
        })
        .collect()
}

/// `<LinkSubList><Link obj=… sub=…/>…`: consecutive entries of one object
/// make one reference.
fn link_sub_list(node: &Node<'_, '_>) -> Vec<LinkRef> {
    let mut out: Vec<LinkRef> = Vec::new();
    for n in items(node, "Link") {
        let Some(object) = attr(&n, "obj").filter(|o| !o.is_empty()) else {
            continue;
        };
        let sub = attr(&n, "sub").unwrap_or_default();
        let entry = SubName {
            name: sub.to_owned(),
            mapped: mapped(&n),
        };
        match out.last_mut() {
            Some(last) if last.object == object => {
                if !sub.is_empty() {
                    last.subs.push(entry);
                }
            }
            _ => out.push(LinkRef {
                object: object.to_owned(),
                file: None,
                subs: if sub.is_empty() {
                    Vec::new()
                } else {
                    vec![entry]
                },
            }),
        }
    }
    out
}

/// `<XLink file=… stamp=… name=…/>` (also with `<Sub>` children).
fn xlink(node: &Node<'_, '_>) -> Option<LinkRef> {
    let mut link = object_ref(attr(node, "name"), attr(node, "file"))?;
    link.subs = subs(node);
    if let Some(sub) = attr(node, "sub").filter(|s| !s.is_empty()) {
        link.subs.push(SubName {
            name: sub.to_owned(),
            mapped: mapped(node),
        });
    }
    Some(link)
}
