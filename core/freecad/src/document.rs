// SPDX-License-Identifier: MIT
//! `Document.xml`: the document's properties, its objects in file order
//! with their state flags and dependencies, and every object's properties.
//!
//! Layout (schema 4; schema 2 names the sections `Features` and
//! `FeatureData`):
//!
//! ```text
//! <Document SchemaVersion ProgramVersion FileVersion [StringHasher]>
//!   [<StringHasher…/> <StringHasher2 file=…/>]          (1.0)
//!   <Properties>…</Properties>                           the document's
//!   <Objects Count Dependencies>
//!     <ObjectDeps Name Count><Dep Name/>…</ObjectDeps>…
//!     <Object type name id [Touched] [Invalid] [Error] [Freeze]/>…
//!   </Objects>
//!   <ObjectData Count>
//!     <Object name [Extensions]>[<Extensions>…]<Properties>…</Properties></Object>…
//!   </ObjectData>
//! </Document>
//! ```

use std::collections::HashMap;

use serde::Serialize;

use crate::placement::Placement;
use crate::value::{LinkRef, ShapeRef, Value, parse_value};
use crate::xml::{Node, attr, parse};

/// A property: its name, FreeCAD type (`App::PropertyLength`) and value.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Property {
    pub name: String,
    #[serde(rename = "type")]
    pub type_name: String,
    /// FreeCAD's status bits as written.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u64>,
    pub value: Value,
    /// Written without a value (`<_Property>`).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub transient: bool,
    /// A dynamic property's group and description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub doc: Option<String>,
}

/// An object's state when it was saved. A touched or invalid object, or one
/// with an error, may have a stored shape that is out of date or empty.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ObjectState {
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub touched: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub invalid: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Frozen (1.0): not recomputed.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub frozen: bool,
}

impl ObjectState {
    /// The stored shape may not be the object's result.
    pub fn is_stale(&self) -> bool {
        self.touched || self.invalid || self.error.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Object {
    /// The unique name (`Pad001`).
    pub name: String,
    /// The C++ type (`PartDesign::Pad`).
    #[serde(rename = "type")]
    pub type_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<i64>,
    pub state: ObjectState,
    /// The extensions' types (`App::OriginGroupExtension`, …).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extensions: Vec<String>,
    /// The objects it depends on, as the file lists them.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<String>,
    pub properties: Vec<Property>,
}

impl Object {
    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }

    pub fn value(&self, name: &str) -> Option<&Value> {
        self.property(name).map(|p| &p.value)
    }

    /// The label (the name shown to the user), else the name.
    pub fn label(&self) -> &str {
        self.value("Label")
            .and_then(Value::as_str)
            .filter(|l| !l.is_empty())
            .unwrap_or(&self.name)
    }

    pub fn placement(&self) -> Option<Placement> {
        match self.value("Placement")? {
            Value::Placement(p) => Some(*p),
            _ => None,
        }
    }

    pub fn bool(&self, name: &str) -> Option<bool> {
        self.value(name).and_then(Value::as_bool)
    }

    pub fn f64(&self, name: &str) -> Option<f64> {
        self.value(name).and_then(Value::as_f64)
    }

    pub fn i64(&self, name: &str) -> Option<i64> {
        self.value(name).and_then(Value::as_i64)
    }

    pub fn str(&self, name: &str) -> Option<&str> {
        self.value(name).and_then(Value::as_str)
    }

    /// The first reference of a link property.
    pub fn link(&self, name: &str) -> Option<&LinkRef> {
        self.value(name).and_then(|v| v.links().first())
    }

    /// The references of a link property (none when it is not one).
    pub fn links(&self, name: &str) -> &[LinkRef] {
        self.value(name).map_or(&[], Value::links)
    }

    /// Every reference the object holds, with its property's name.
    pub fn references(&self) -> impl Iterator<Item = (&str, &LinkRef)> {
        self.properties
            .iter()
            .flat_map(|p| p.value.links().iter().map(move |l| (p.name.as_str(), l)))
    }

    /// The stored shape (the `Shape` property).
    pub fn shape(&self) -> Option<&ShapeRef> {
        match self.value("Shape")? {
            Value::Shape(s) => Some(s),
            _ => None,
        }
    }

    /// The archive entry of the stored shape, when there is one.
    pub fn shape_file(&self) -> Option<&str> {
        self.shape()?.file.as_deref()
    }

    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|e| e == name)
    }

    /// The App-level visibility (the `Visibility` property 0.19 and later
    /// save); None when the file has none.
    pub fn visibility(&self) -> Option<bool> {
        self.bool("Visibility")
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Document {
    pub schema_version: u32,
    /// The FreeCAD that saved it, as written (`1.0R39319 (Git)`).
    pub program_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_version: Option<u32>,
    /// 1.0's string table for topological names is used.
    pub string_hasher: bool,
    pub properties: Vec<Property>,
    pub objects: Vec<Object>,
    /// Values that could not be read as their forms should (kept as XML).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl Document {
    /// Parses `Document.xml`.
    pub fn parse(text: &str) -> Result<Document, String> {
        let xml = parse(text)?;
        let root = xml.root_element();
        if root.tag_name().name() != "Document" {
            return Err(format!(
                "the root element is <{}>, not <Document>",
                root.tag_name().name()
            ));
        }
        let number = |name: &str| attr(&root, name).and_then(|v| v.trim().parse::<u32>().ok());
        let mut document = Document {
            schema_version: number("SchemaVersion").unwrap_or(0),
            program_version: attr(&root, "ProgramVersion").unwrap_or_default().to_owned(),
            file_version: number("FileVersion"),
            string_hasher: attr(&root, "StringHasher") == Some("1"),
            properties: Vec::new(),
            objects: Vec::new(),
            warnings: Vec::new(),
            index: HashMap::new(),
        };
        let mut data: HashMap<String, Node<'_, '_>> = HashMap::new();
        let mut dependencies: HashMap<String, Vec<String>> = HashMap::new();
        for section in root.children().filter(|n| n.is_element()) {
            match section.tag_name().name() {
                "Properties" => {
                    document.properties = parse_properties(&section, &mut document.warnings);
                }
                "Objects" | "Features" => {
                    for node in section.children().filter(|n| n.is_element()) {
                        match node.tag_name().name() {
                            "ObjectDeps" => {
                                let name = attr(&node, "Name").unwrap_or_default().to_owned();
                                let deps = node
                                    .children()
                                    .filter(|d| d.tag_name().name() == "Dep")
                                    .filter_map(|d| attr(&d, "Name").map(str::to_owned))
                                    .collect();
                                dependencies.insert(name, deps);
                            }
                            "Object" | "Feature" => document.objects.push(object_entry(&node)?),
                            _ => {}
                        }
                    }
                }
                "ObjectData" | "FeatureData" => {
                    for node in section
                        .children()
                        .filter(|n| matches!(n.tag_name().name(), "Object" | "Feature"))
                    {
                        if let Some(name) = attr(&node, "name") {
                            data.insert(name.to_owned(), node);
                        }
                    }
                }
                _ => {}
            }
        }
        for object in &mut document.objects {
            if let Some(node) = data.get(&object.name) {
                for child in node.children().filter(|n| n.is_element()) {
                    match child.tag_name().name() {
                        "Extensions" => {
                            object.extensions = child
                                .children()
                                .filter(|e| e.tag_name().name() == "Extension")
                                .filter_map(|e| attr(&e, "type").map(str::to_owned))
                                .collect();
                        }
                        "Properties" => {
                            let mut problems = Vec::new();
                            object.properties = parse_properties(&child, &mut problems);
                            document.warnings.extend(
                                problems
                                    .into_iter()
                                    .map(|p| format!("object {}: {p}", object.name)),
                            );
                        }
                        _ => {}
                    }
                }
            }
            object.dependencies = dependencies.remove(&object.name).unwrap_or_default();
        }
        document.reindex();
        Ok(document)
    }

    fn reindex(&mut self) {
        self.index = self
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| (o.name.clone(), i))
            .collect();
    }

    pub fn object(&self, name: &str) -> Option<&Object> {
        self.index.get(name).map(|&i| &self.objects[i])
    }

    /// The position of an object in the file's order.
    pub fn position(&self, name: &str) -> Option<usize> {
        self.index.get(name).copied()
    }

    pub fn property(&self, name: &str) -> Option<&Property> {
        self.properties.iter().find(|p| p.name == name)
    }

    /// The document's label.
    pub fn label(&self) -> Option<&str> {
        self.property("Label").and_then(|p| p.value.as_str())
    }

    pub fn version(&self) -> crate::version::Version {
        crate::version::Version::parse(&self.program_version)
    }

    /// An enumeration's value as text: from the file's own list (a custom
    /// enumeration), else from the version's tables; None when unknown.
    pub fn enum_text(&self, object: &Object, property: &str) -> Option<String> {
        let Value::Enumeration(e) = object.value(property)? else {
            return None;
        };
        let index = usize::try_from(e.index).ok()?;
        match &e.custom {
            Some(list) => list.get(index).cloned(),
            None => crate::version::enum_values(&self.version(), &object.type_name, property)?
                .get(index)
                .cloned(),
        }
    }

    /// Replaces lists kept in files of the archive by their values; `read`
    /// gives an entry's data. Entries that are missing or do not decode
    /// stay as [`Value::File`], with a message each.
    pub fn resolve_files(&mut self, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<String> {
        let mut problems = resolve(&mut self.properties, read);
        for object in &mut self.objects {
            for problem in resolve(&mut object.properties, read) {
                problems.push(format!("{}: {problem}", object.name));
            }
        }
        problems
    }
}

pub(crate) fn resolve(
    properties: &mut [Property],
    read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<String> {
    let mut problems = Vec::new();
    for property in properties {
        let Value::File { element, file } = &property.value else {
            continue;
        };
        let Some(data) = read(file) else {
            problems.push(format!("{}: no entry {file}", property.name));
            continue;
        };
        match crate::binary::decode(element, &data) {
            Some(Ok(value)) => property.value = value,
            Some(Err(e)) => problems.push(format!("{} ({file}): {e}", property.name)),
            None => {}
        }
    }
    problems
}

/// An entry of the object list: type, name, id and state flags.
fn object_entry(node: &Node<'_, '_>) -> Result<Object, String> {
    let name = attr(node, "name")
        .ok_or("an object without a name")?
        .to_owned();
    let flag = |key: &str| matches!(attr(node, key), Some("1" | "true"));
    Ok(Object {
        type_name: attr(node, "type").unwrap_or_default().to_owned(),
        id: attr(node, "id").and_then(|v| v.trim().parse().ok()),
        state: ObjectState {
            touched: flag("Touched"),
            invalid: flag("Invalid"),
            error: attr(node, "Error")
                .filter(|e| !e.is_empty())
                .map(str::to_owned),
            frozen: flag("Freeze"),
        },
        name,
        extensions: Vec::new(),
        dependencies: Vec::new(),
        properties: Vec::new(),
    })
}

/// The `<Property>` and `<_Property>` children of a `<Properties>` element.
/// A value that does not read as its form should is kept as XML, with a
/// message in `problems`.
pub(crate) fn parse_properties(node: &Node<'_, '_>, problems: &mut Vec<String>) -> Vec<Property> {
    let mut out = Vec::new();
    for child in node.children().filter(|n| n.is_element()) {
        let transient = match child.tag_name().name() {
            "Property" => false,
            "_Property" => true,
            _ => continue,
        };
        let name = attr(&child, "name").unwrap_or_default().to_owned();
        let type_name = attr(&child, "type").unwrap_or_default().to_owned();
        let value = if transient {
            Value::None
        } else {
            match parse_value(&child) {
                // Enumerations are written as integers (indices).
                Ok(Value::Integer(index)) if type_name.ends_with("PropertyEnumeration") => {
                    Value::Enumeration(crate::value::Enumeration {
                        index,
                        custom: None,
                    })
                }
                Ok(value) => value,
                Err(e) => {
                    problems.push(format!("property {name}: {e}"));
                    match child.children().find(|n| n.is_element()) {
                        Some(first) => Value::Xml(crate::xml::Element::from_node(&first)),
                        None => Value::None,
                    }
                }
            }
        };
        out.push(Property {
            type_name,
            status: attr(&child, "status").and_then(|s| s.trim().parse().ok()),
            group: attr(&child, "group").map(str::to_owned),
            doc: attr(&child, "doc").map(str::to_owned),
            name,
            value,
            transient,
        });
    }
    out
}
