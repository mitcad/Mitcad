// SPDX-License-Identifier: MIT
//! `GuiDocument.xml`: the view data FreeCAD's user interface saves per
//! object (a view provider of the same name): visibility, colours and
//! transparency. Optional: FreeCAD opens a document without it, and the
//! command-line FreeCAD does not write it.
//!
//! Colours: up to 0.21 `ShapeColor` (a packed colour) and `DiffuseColor`
//! (per face, a file); from 1.0 `ShapeAppearance` (a list of materials, one
//! for the shape or one per face, in a file).

use std::collections::HashMap;

use serde::Serialize;

use crate::document::{Property, parse_properties};
use crate::value::{Color, Value};
use crate::xml::{attr, parse};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ViewProvider {
    /// The object's name.
    pub name: String,
    pub properties: Vec<Property>,
}

impl ViewProvider {
    pub fn value(&self, name: &str) -> Option<&Value> {
        self.properties
            .iter()
            .find(|p| p.name == name)
            .map(|p| &p.value)
    }

    pub fn visible(&self) -> Option<bool> {
        self.value("Visibility").and_then(Value::as_bool)
    }

    /// The shape's colour: the first appearance material's diffuse colour
    /// (1.0), else `ShapeColor`.
    pub fn shape_color(&self) -> Option<Color> {
        if let Some(Value::MaterialList(materials)) = self.value("ShapeAppearance")
            && let Some(first) = materials.first()
        {
            return Some(first.diffuse);
        }
        match self.value("ShapeColor")? {
            Value::Color(c) => Some(*c),
            _ => None,
        }
    }

    /// Colours per face, when the view has them (one entry: the whole
    /// shape).
    pub fn face_colors(&self) -> Vec<Color> {
        match self.value("ShapeAppearance") {
            Some(Value::MaterialList(materials)) => materials.iter().map(|m| m.diffuse).collect(),
            _ => match self.value("DiffuseColor") {
                Some(Value::ColorList(colors)) => colors.clone(),
                _ => Vec::new(),
            },
        }
    }

    /// Transparency in percent.
    pub fn transparency(&self) -> Option<f64> {
        self.value("Transparency").and_then(Value::as_f64)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct GuiDocument {
    pub providers: Vec<ViewProvider>,
    #[serde(skip)]
    index: HashMap<String, usize>,
}

impl GuiDocument {
    pub fn parse(text: &str) -> Result<GuiDocument, String> {
        let xml = parse(text)?;
        let root = xml.root_element();
        let mut providers = Vec::new();
        for data in root
            .children()
            .filter(|n| n.tag_name().name() == "ViewProviderData")
        {
            for node in data
                .children()
                .filter(|n| n.tag_name().name() == "ViewProvider")
            {
                let name = attr(&node, "name").unwrap_or_default().to_owned();
                let mut properties = Vec::new();
                for child in node
                    .children()
                    .filter(|n| n.tag_name().name() == "Properties")
                {
                    // A view value that does not read is of no use to the
                    // import; it stays as XML.
                    properties = parse_properties(&child, &mut Vec::new());
                }
                providers.push(ViewProvider { name, properties });
            }
        }
        let index = providers
            .iter()
            .enumerate()
            .map(|(i, p)| (p.name.clone(), i))
            .collect();
        Ok(GuiDocument { providers, index })
    }

    pub fn get(&self, object: &str) -> Option<&ViewProvider> {
        self.index.get(object).map(|&i| &self.providers[i])
    }

    /// See [`crate::Document::resolve_files`].
    pub fn resolve_files(&mut self, read: &mut dyn FnMut(&str) -> Option<Vec<u8>>) -> Vec<String> {
        let mut problems = Vec::new();
        for provider in &mut self.providers {
            for problem in crate::document::resolve(&mut provider.properties, read) {
                problems.push(format!("view of {}: {problem}", provider.name));
            }
        }
        problems
    }
}
