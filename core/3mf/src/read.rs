// SPDX-License-Identifier: MIT
//! Reading a 3MF package's core model and checking it.

use std::collections::HashMap;

use mitcad_zip::Archive;
use roxmltree::{Document, Node};

use crate::{
    CORE_NAMESPACE, Error, MODEL_CONTENT_TYPE, MODEL_RELATIONSHIP, Mesh, Model, Part, Placement,
};

/// How deep components may nest.
const MAX_DEPTH: usize = 32;

fn error(message: impl Into<String>) -> Error {
    Error(message.into())
}

/// Reads a 3MF package: the root model part's build, every build item
/// followed through its components to mesh objects, as parts with their
/// placements, names and base material colours, in millimetres. It checks
/// what the core specification requires: the content type and the
/// relationship of the model part, the core namespace and a known unit, no
/// required extensions, unique object ids, references to objects defined
/// before, vertex indices in range, and that the meshes of objects of type
/// `model` are closed (each edge shared by two triangles in opposite
/// directions) and face outwards.
pub fn read(data: &[u8]) -> Result<Model, Error> {
    let archive =
        Archive::new(data.to_vec()).map_err(|e| error(format!("not a zip package: {e}")))?;
    let text = |name: &str| -> Result<String, Error> {
        let bytes = archive
            .read_by_name(name)
            .map_err(|e| error(format!("{name}: {e}")))?;
        String::from_utf8(bytes).map_err(|_| error(format!("{name} is not UTF-8")))
    };
    let types = text("[Content_Types].xml")?;
    let types = Document::parse(&types).map_err(|e| error(format!("[Content_Types].xml: {e}")))?;
    let relationships = text("_rels/.rels")?;
    let relationships =
        Document::parse(&relationships).map_err(|e| error(format!("_rels/.rels: {e}")))?;
    let target = relationships
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("Relationship"))
        .find(|n| n.attribute("Type") == Some(MODEL_RELATIONSHIP))
        .and_then(|n| n.attribute("Target"))
        .ok_or_else(|| error("_rels/.rels has no 3D model relationship"))?;
    let part = target.trim_start_matches('/');
    let content_type = content_type(&types, part);
    if content_type.as_deref() != Some(MODEL_CONTENT_TYPE) {
        return Err(error(format!(
            "{part} has the content type {}, not {MODEL_CONTENT_TYPE}",
            content_type.as_deref().unwrap_or("(none)")
        )));
    }
    let xml = text(part)?;
    let document = Document::parse(&xml).map_err(|e| error(format!("{part}: {e}")))?;
    read_model(document.root_element())
}

/// The content type of a part: an override for its name, else the default
/// for its extension (ignoring case).
fn content_type(types: &Document<'_>, part: &str) -> Option<String> {
    let name = format!("/{part}");
    let extension = part.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
    let root = types.root_element();
    let overridden = root
        .children()
        .filter(|n| n.has_tag_name("Override"))
        .find(|n| {
            n.attribute("PartName")
                .is_some_and(|p| p.eq_ignore_ascii_case(&name))
        });
    let default = || {
        root.children()
            .filter(|n| n.has_tag_name("Default"))
            .find(|n| n.attribute("Extension").map(str::to_ascii_lowercase) == extension)
    };
    overridden
        .or_else(default)
        .and_then(|n| n.attribute("ContentType"))
        .map(str::to_owned)
}

/// An object of the resources.
enum Object {
    Mesh {
        name: String,
        color: Option<[f64; 3]>,
        mesh: Mesh,
    },
    Components {
        name: String,
        components: Vec<(u32, Placement)>,
    },
}

fn read_model(model: Node<'_, '_>) -> Result<Model, Error> {
    if model.tag_name().name() != "model" || model.tag_name().namespace() != Some(CORE_NAMESPACE) {
        return Err(error("the model part's root is not a 3MF core <model>"));
    }
    let scale = match model.attribute("unit").unwrap_or("millimeter") {
        "micron" => 0.001,
        "millimeter" => 1.0,
        "centimeter" => 10.0,
        "inch" => 25.4,
        "foot" => 304.8,
        "meter" => 1000.0,
        other => return Err(error(format!("unknown unit '{other}'"))),
    };
    if let Some(required) = model
        .attribute("requiredextensions")
        .filter(|r| !r.trim().is_empty())
    {
        return Err(error(format!(
            "the model requires extensions this reader lacks: {required}"
        )));
    }
    let mut title = None;
    let mut materials: HashMap<u32, Vec<[f64; 3]>> = HashMap::new();
    let mut objects: HashMap<u32, Object> = HashMap::new();
    let mut items = Vec::new();
    for child in model.children().filter(Node::is_element) {
        if core(&child, "metadata") && child.attribute("name") == Some("Title") {
            title = child.text().map(str::to_owned);
        } else if core(&child, "resources") {
            for resource in child.children().filter(Node::is_element) {
                if core(&resource, "basematerials") {
                    let id = id_of(&resource, "id")?;
                    let colors = resource
                        .children()
                        .filter(|n| core(n, "base"))
                        .map(|base| color(base.attribute("displaycolor").unwrap_or("")))
                        .collect::<Result<Vec<_>, _>>()?;
                    if materials.insert(id, colors).is_some() || objects.contains_key(&id) {
                        return Err(error(format!("resource id {id} is used twice")));
                    }
                } else if core(&resource, "object") {
                    let id = id_of(&resource, "id")?;
                    if objects.contains_key(&id) || materials.contains_key(&id) {
                        return Err(error(format!("resource id {id} is used twice")));
                    }
                    let object = read_object(&resource, id, &materials, &objects)?;
                    objects.insert(id, object);
                }
            }
        } else if core(&child, "build") {
            for item in child.children().filter(|n| core(n, "item")) {
                let id = id_of(&item, "objectid")?;
                if !objects.contains_key(&id) {
                    return Err(error(format!(
                        "a build item refers to object {id}, which is not defined"
                    )));
                }
                items.push((id, transform(&item)?));
            }
        }
    }
    if items.is_empty() {
        return Err(error("the build has no items"));
    }
    // Every placement of every mesh object, through the components.
    let mut parts: Vec<Part> = Vec::new();
    let mut part_of: HashMap<u32, usize> = HashMap::new();
    let mut stack: Vec<(u32, Placement, usize)> =
        items.iter().rev().map(|&(id, p)| (id, p, 0)).collect();
    while let Some((id, placement, depth)) = stack.pop() {
        match &objects[&id] {
            Object::Mesh { name, color, mesh } => {
                let index = *part_of.entry(id).or_insert_with(|| {
                    parts.push(Part {
                        name: name.clone(),
                        color: *color,
                        mesh: Mesh {
                            vertices: mesh.vertices.iter().map(|v| v.map(|c| c * scale)).collect(),
                            triangles: mesh.triangles.clone(),
                        },
                        placements: Vec::new(),
                    });
                    parts.len() - 1
                });
                let mut placed = placement;
                placed.translation = placed.translation.map(|c| c * scale);
                parts[index].placements.push(placed);
            }
            Object::Components { components, .. } => {
                if depth >= MAX_DEPTH {
                    return Err(error("components nest too deep"));
                }
                for &(child, inner) in components.iter().rev() {
                    stack.push((child, placement.after(&inner), depth + 1));
                }
            }
        }
    }
    let first = match &objects[&items[0].0] {
        Object::Mesh { name, .. } | Object::Components { name, .. } => name.clone(),
    };
    Ok(Model {
        name: title.unwrap_or(first),
        parts,
        application: String::new(),
    })
}

fn read_object(
    object: &Node<'_, '_>,
    id: u32,
    materials: &HashMap<u32, Vec<[f64; 3]>>,
    objects: &HashMap<u32, Object>,
) -> Result<Object, Error> {
    let name = object
        .attribute("name")
        .map_or_else(|| format!("Object {id}"), str::to_owned);
    let kind = object.attribute("type").unwrap_or("model");
    if let Some(mesh) = object.children().find(|n| core(n, "mesh")) {
        let mut out = Mesh::default();
        for list in mesh.children().filter(Node::is_element) {
            if core(&list, "vertices") {
                for vertex in list.children().filter(|n| core(n, "vertex")) {
                    out.vertices.push([
                        number(&vertex, "x")?,
                        number(&vertex, "y")?,
                        number(&vertex, "z")?,
                    ]);
                }
            } else if core(&list, "triangles") {
                for triangle in list.children().filter(|n| core(n, "triangle")) {
                    out.triangles.push([
                        id_of(&triangle, "v1")?,
                        id_of(&triangle, "v2")?,
                        id_of(&triangle, "v3")?,
                    ]);
                }
            }
        }
        if kind == "model" {
            out.check_closed()
                .map_err(|e| error(format!("object {id} ({name}): {e}")))?;
            if out.volume() <= 0.0 {
                return Err(error(format!(
                    "object {id} ({name}): its triangles face inwards"
                )));
            }
        } else if let Some(t) = out
            .triangles
            .iter()
            .find(|t| t.iter().any(|&v| v as usize >= out.vertices.len()))
        {
            return Err(error(format!(
                "object {id} ({name}): triangle {t:?} refers to a missing vertex"
            )));
        }
        let color = match object.attribute("pid") {
            None => None,
            Some(_) => {
                let group = id_of(object, "pid")?;
                let index = object
                    .attribute("pindex")
                    .map_or(Ok(0), |_| id_of(object, "pindex"))?;
                let colors = materials.get(&group).ok_or_else(|| {
                    error(format!(
                        "object {id} refers to material group {group}, which is not defined"
                    ))
                })?;
                Some(*colors.get(index as usize).ok_or_else(|| {
                    error(format!(
                        "object {id} refers to material {index} of group {group}, which has {}",
                        colors.len()
                    ))
                })?)
            }
        };
        return Ok(Object::Mesh {
            name,
            color,
            mesh: out,
        });
    }
    let Some(list) = object.children().find(|n| core(n, "components")) else {
        return Err(error(format!(
            "object {id} ({name}) has neither a mesh nor components"
        )));
    };
    let mut components = Vec::new();
    for component in list.children().filter(|n| core(n, "component")) {
        let child = id_of(&component, "objectid")?;
        if !objects.contains_key(&child) {
            return Err(error(format!(
                "object {id} ({name}) refers to object {child}, which is not defined before it"
            )));
        }
        components.push((child, transform(&component)?));
    }
    Ok(Object::Components { name, components })
}

/// An element of the core namespace.
fn core(node: &Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.has_tag_name((CORE_NAMESPACE, name))
}

/// A non-negative integer attribute.
fn id_of(node: &Node<'_, '_>, attribute: &str) -> Result<u32, Error> {
    let text = node
        .attribute(attribute)
        .ok_or_else(|| error(format!("<{}> has no {attribute}", node.tag_name().name())))?;
    text.trim().parse().map_err(|_| {
        error(format!(
            "<{}> {attribute}=\"{text}\" is not an index",
            node.tag_name().name()
        ))
    })
}

fn number(node: &Node<'_, '_>, attribute: &str) -> Result<f64, Error> {
    let text = node
        .attribute(attribute)
        .ok_or_else(|| error(format!("<{}> has no {attribute}", node.tag_name().name())))?;
    text.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| {
            error(format!(
                "<{}> {attribute}=\"{text}\" is not a number",
                node.tag_name().name()
            ))
        })
}

/// A component's or build item's transform; the identity when it has none.
fn transform(node: &Node<'_, '_>) -> Result<Placement, Error> {
    let Some(text) = node.attribute("transform") else {
        return Ok(Placement::IDENTITY);
    };
    let values: Vec<f64> = text
        .split_whitespace()
        .map(|v| v.parse::<f64>().ok().filter(|v| v.is_finite()))
        .collect::<Option<_>>()
        .ok_or_else(|| {
            error(format!(
                "transform \"{text}\" has a value that is not a number"
            ))
        })?;
    let values: [f64; 12] = values
        .try_into()
        .map_err(|_| error(format!("transform \"{text}\" does not have 12 values")))?;
    Ok(Placement::from_3mf(values))
}

/// `#RRGGBB` or `#RRGGBBAA` as sRGB components in [0, 1].
fn color(text: &str) -> Result<[f64; 3], Error> {
    let hex = text
        .strip_prefix('#')
        .filter(|h| matches!(h.len(), 6 | 8) && h.is_ascii());
    let byte = |i: usize| hex.and_then(|h| u8::from_str_radix(&h[i..i + 2], 16).ok());
    match (byte(0), byte(2), byte(4)) {
        (Some(r), Some(g), Some(b)) => Ok([r, g, b].map(|c| f64::from(c) / 255.0)),
        _ => Err(error(format!(
            "displaycolor \"{text}\" is not #RRGGBB or #RRGGBBAA"
        ))),
    }
}
