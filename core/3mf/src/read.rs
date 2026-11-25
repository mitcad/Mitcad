// SPDX-License-Identifier: MIT
//! Reading a 3MF package's core model and checking it.
//!
//! The model may keep its objects in more model parts than the root one
//! (the Production extension, mitcad#2156): a component or a build item of
//! the root model with a `p:path` refers to an object of the model part at
//! that path, as slicers' projects keep each object's mesh in a part of its
//! own (`3D/Objects/object_1.model`). Objects without a `name` take the
//! name a slicer's settings file of the package gives them
//! (`Metadata/model_settings.config`, `Metadata/Slic3r_PE_model.config`).

use std::collections::HashMap;

use mitcad_zip::{Archive, DEFAULT_ENTRY_LIMIT, Entry, LARGE_ENTRY_LIMIT};
use roxmltree::{Document, Node};

use crate::{
    BuildObject, CORE_NAMESPACE, Error, ImportRead, MODEL_CONTENT_TYPE, MODEL_RELATIONSHIP, Mesh,
    Model, PRODUCTION_NAMESPACE, Part, Placement,
};

/// How deep components may nest.
const MAX_DEPTH: usize = 32;

/// The most model parts a root model's components may refer to.
const MAX_MODEL_PARTS: usize = 4096;

/// The largest settings file read for the objects' names.
const MAX_SETTINGS_FILE: u64 = 64 << 20;

/// The slicers' settings files that name a project's objects and their
/// parts, when the model does not.
const SETTINGS_FILES: [&str; 2] = [
    "Metadata/model_settings.config",
    "Metadata/Slic3r_PE_model.config",
];

fn error(message: impl Into<String>) -> Error {
    Error(message.into())
}

/// Reads a 3MF package: the root model part's build, every build item
/// followed through its components to mesh objects, as parts with their
/// placements, names and base material colours, in millimetres, and with
/// several build items each item's placements as an object of
/// [`Model::objects`] (mitcad#856). Components and build items of the root
/// model may refer to objects of other model parts of the package (the
/// Production extension's `p:path`, mitcad#2156); an object without a name
/// takes the one a slicer's settings file gives it, else `Object <id>`. It
/// checks what the core specification requires: the content type and the
/// relationship of the model part, the core namespace and a known unit, no
/// required extensions but the Production extension, unique object ids,
/// references to objects defined before, vertex indices in range, and that
/// the meshes of objects of type `model` are closed (each edge shared by two
/// triangles in opposite directions) and face outwards.
pub fn read(data: &[u8]) -> Result<Model, Error> {
    read_with(data, false).map(|read| read.model)
}

/// [`read`] for an import (mitcad#2156): a mesh of an object of type
/// `model` that is not closed, or faces inwards, is read as it is, and
/// triangles that use a vertex twice are left out, each with a warning in
/// words that names the part; a base material that is not there leaves
/// the part without a colour. What cannot be read at all (not a zip
/// archive, no model part, an extension it requires that is not read, a
/// vertex index out of range) is still refused.
pub fn read_for_import(data: &[u8]) -> Result<ImportRead, Error> {
    read_with(data, true)
}

/// An object's key: the index of its model part (the root model is 0) and
/// its id in that part.
type Key = (usize, u32);

/// A model part's base material groups by id: each base's name and colour.
type Materials = HashMap<u32, Vec<(Option<String>, [f64; 3])>>;

/// An object of the resources.
enum Object {
    Mesh {
        name: Option<String>,
        color: Option<[f64; 3]>,
        /// The base material's name of the colour.
        material: Option<String>,
        /// In millimetres.
        mesh: Mesh,
        /// Why the mesh is no closed solid, for [`read_for_import`]'s
        /// warning (the part's name goes before it).
        problem: Option<String>,
    },
    Components {
        name: Option<String>,
        components: Vec<(Key, Placement)>,
    },
}

impl Object {
    fn name(&self) -> Option<&str> {
        match self {
            Object::Mesh { name, .. } | Object::Components { name, .. } => name.as_deref(),
        }
    }
}

/// The content types of the package's parts.
#[derive(Default)]
struct ContentTypes {
    /// (extension in lower case, content type)
    defaults: Vec<(String, String)>,
    /// (part name with its leading `/`, content type)
    overrides: Vec<(String, String)>,
}

impl ContentTypes {
    fn parse(text: &str) -> Result<Self, Error> {
        let document =
            Document::parse(text).map_err(|e| error(format!("[Content_Types].xml: {e}")))?;
        let mut types = ContentTypes::default();
        for node in document.root_element().children() {
            let Some(content) = node.attribute("ContentType") else {
                continue;
            };
            if node.has_tag_name("Default")
                && let Some(extension) = node.attribute("Extension")
            {
                types
                    .defaults
                    .push((extension.to_ascii_lowercase(), content.to_owned()));
            } else if node.has_tag_name("Override")
                && let Some(part) = node.attribute("PartName")
            {
                types.overrides.push((part.to_owned(), content.to_owned()));
            }
        }
        Ok(types)
    }

    /// The content type of a part: an override for its name, else the
    /// default for its extension (ignoring case).
    fn of(&self, part: &str) -> Option<&str> {
        let name = format!("/{part}");
        let extension = part.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        self.overrides
            .iter()
            .find(|(p, _)| p.eq_ignore_ascii_case(&name))
            .or_else(|| {
                self.defaults
                    .iter()
                    .find(|(e, _)| Some(e.as_str()) == extension.as_deref())
            })
            .map(|(_, t)| t.as_str())
    }
}

/// A part of the package by its name: as written, else ignoring case (OPC
/// part names are compared so).
fn find_entry<'a>(archive: &'a Archive, name: &str) -> Option<&'a Entry> {
    archive.find(name).or_else(|| {
        archive
            .entries()
            .iter()
            .find(|e| !e.is_dir() && e.name.eq_ignore_ascii_case(name))
    })
}

/// The names a slicer's settings files give the objects of the root model
/// and their parts (by the object id in the root model, then the object
/// id in the object's own model part).
#[derive(Default)]
struct SettingsNames {
    objects: HashMap<u32, (Option<String>, HashMap<u32, String>)>,
}

impl SettingsNames {
    fn read(archive: &Archive) -> Self {
        let mut names = SettingsNames::default();
        for file in SETTINGS_FILES {
            let Some(entry) = find_entry(archive, file) else {
                continue;
            };
            // A settings file that cannot be read names nothing.
            let Some(text) = archive
                .read_with_limit(entry, MAX_SETTINGS_FILE)
                .ok()
                .and_then(|bytes| String::from_utf8(bytes).ok())
            else {
                continue;
            };
            let Ok(document) = Document::parse(&text) else {
                continue;
            };
            let id =
                |node: &Node<'_, '_>| -> Option<u32> { node.attribute("id")?.trim().parse().ok() };
            for object in document
                .root_element()
                .children()
                .filter(|n| n.has_tag_name("object"))
            {
                let Some(object_id) = id(&object) else {
                    continue;
                };
                let known = names.objects.entry(object_id).or_default();
                if known.0.is_none() {
                    known.0 = name_metadata(&object, "object");
                }
                for part in object.children().filter(|n| n.has_tag_name("part")) {
                    if let (Some(part_id), Some(name)) = (id(&part), name_metadata(&part, "volume"))
                    {
                        known.1.entry(part_id).or_insert(name);
                    }
                }
            }
        }
        names
    }

    /// The name of the object `key` placed through the root model's object
    /// `owner`: its part's name in the owner's settings when it is in
    /// another model part, else the owner's name.
    fn name(&self, key: Key, owner: Option<u32>) -> Option<String> {
        let (object, parts) = self.objects.get(&owner?)?;
        if key.0 != 0
            && let Some(name) = parts.get(&key.1)
        {
            return Some(name.clone());
        }
        object.clone()
    }
}

/// A settings element's name: its `<metadata key="name" value="…"/>`
/// (with no `type`, or the given one).
fn name_metadata(node: &Node<'_, '_>, kind: &str) -> Option<String> {
    node.children()
        .filter(|n| n.has_tag_name("metadata"))
        .find(|m| {
            m.attribute("key") == Some("name") && m.attribute("type").is_none_or(|t| t == kind)
        })
        .and_then(|m| m.attribute("value"))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}

/// The words for a 3MF extension by its namespace.
fn extension_words(uri: &str) -> String {
    let known = [
        ("/material/", "Materials and Properties"),
        ("/slice/", "Slice"),
        ("/beamlattice/", "Beam Lattice"),
        ("/securecontent/", "Secure Content (encrypted parts)"),
        ("/volumetric/", "Volumetric"),
        ("/displacement/", "Displacement"),
        ("/booleanoperations/", "Boolean Operations"),
        ("/implicit/", "Implicit"),
    ];
    known
        .iter()
        .find(|(part, _)| uri.contains(part))
        .map_or_else(|| uri.to_owned(), |(_, words)| (*words).to_owned())
}

/// Refuses a model that requires an extension the reader lacks: only the
/// Production extension is read.
fn check_required(model: &Node<'_, '_>) -> Result<(), Error> {
    let Some(required) = model.attribute("requiredextensions") else {
        return Ok(());
    };
    let mut lacking = Vec::new();
    for prefix in required.split_whitespace() {
        match model.lookup_namespace_uri(Some(prefix)) {
            Some(PRODUCTION_NAMESPACE) => {}
            Some(uri) => lacking.push(format!("the 3MF {} extension", extension_words(uri))),
            None => lacking.push(format!(
                "an extension by the prefix “{prefix}”, which the file does not declare"
            )),
        }
    }
    if lacking.is_empty() {
        return Ok(());
    }
    Err(error(format!(
        "the file requires {}, which MitCad does not read",
        lacking.join(" and ")
    )))
}

/// The millimetres of a model's unit, after checking that it is a core
/// model whose extensions are read.
fn check_model(model: &Node<'_, '_>, part: &str) -> Result<f64, Error> {
    if model.tag_name().name() != "model" || model.tag_name().namespace() != Some(CORE_NAMESPACE) {
        return Err(error(format!(
            "the model part's root is not a 3MF core <model> ({part})"
        )));
    }
    let scale = match model.attribute("unit").unwrap_or("millimeter") {
        "micron" => 0.001,
        "millimeter" => 1.0,
        "centimeter" => 10.0,
        "inch" => 25.4,
        "foot" => 304.8,
        "meter" => 1000.0,
        other => return Err(error(format!("unknown unit “{other}”"))),
    };
    check_required(model)?;
    Ok(scale)
}

/// The reader's state: the package, the model parts read and their
/// objects.
struct Reader {
    archive: Archive,
    types: ContentTypes,
    lenient: bool,
    /// The model parts read by their paths, the root model first.
    parts: Vec<String>,
    objects: HashMap<Key, Object>,
}

fn read_with(data: &[u8], lenient: bool) -> Result<ImportRead, Error> {
    let archive = Archive::new(data.to_vec())
        .map_err(|e| error(format!("not a zip package, as a 3MF file is: {e}")))?;
    let text = |name: &str| -> Result<String, Error> {
        let entry = find_entry(&archive, name)
            .ok_or_else(|| error(format!("the package has no {name}, as a 3MF file has")))?;
        let bytes = archive
            .read_with_limit(entry, DEFAULT_ENTRY_LIMIT)
            .map_err(|e| error(format!("{name}: {e}")))?;
        String::from_utf8(bytes).map_err(|_| error(format!("{name} is not UTF-8")))
    };
    let types = ContentTypes::parse(&text("[Content_Types].xml")?)?;
    let relationships = text("_rels/.rels")?;
    let relationships =
        Document::parse(&relationships).map_err(|e| error(format!("_rels/.rels: {e}")))?;
    let root = relationships
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("Relationship"))
        .find(|n| n.attribute("Type") == Some(MODEL_RELATIONSHIP))
        .and_then(|n| n.attribute("Target"))
        .ok_or_else(|| error("the package has no 3D model: _rels/.rels names none"))?
        .trim_start_matches('/')
        .to_owned();
    let reader = Reader {
        archive,
        types,
        lenient,
        parts: Vec::new(),
        objects: HashMap::new(),
    };
    let xml = reader.model_text(&root)?;
    let document = Document::parse(&xml).map_err(|e| error(format!("{root}: {e}")))?;
    reader.read_root(document.root_element(), root)
}

impl Reader {
    /// A model part's text, after checking its content type.
    fn model_text(&self, part: &str) -> Result<String, Error> {
        let entry = find_entry(&self.archive, part)
            .ok_or_else(|| error(format!("the package has no model part {part}")))?;
        let content_type = self.types.of(&entry.name);
        if content_type != Some(MODEL_CONTENT_TYPE) {
            return Err(error(format!(
                "{part} has the content type {}, not {MODEL_CONTENT_TYPE}",
                content_type.unwrap_or("(none)")
            )));
        }
        // The model part may be a large mesh: the explicit opt-in to the
        // zip reader's large entry limit (mitcad#923).
        let bytes = self
            .archive
            .read_with_limit(entry, LARGE_ENTRY_LIMIT)
            .map_err(|e| error(format!("{part}: {e}")))?;
        String::from_utf8(bytes).map_err(|_| error(format!("{part} is not UTF-8")))
    }

    /// Reads another model part's objects (once), for a reference of the
    /// root model by `p:path`; its index in `parts`.
    fn load_part(&mut self, path: &str) -> Result<usize, Error> {
        if let Some(known) = self.parts.iter().position(|p| p.eq_ignore_ascii_case(path)) {
            return Ok(known);
        }
        if self.parts.len() >= MAX_MODEL_PARTS {
            return Err(error(format!(
                "the model refers to more than {MAX_MODEL_PARTS} model parts"
            )));
        }
        let xml = self.model_text(path)?;
        let document = Document::parse(&xml).map_err(|e| error(format!("{path}: {e}")))?;
        let model = document.root_element();
        let scale = check_model(&model, path)?;
        let index = self.parts.len();
        self.parts.push(path.to_owned());
        for resources in model.children().filter(|n| core(n, "resources")) {
            self.read_resources(&resources, index, scale)?;
        }
        Ok(index)
    }

    /// The key of the object a component or build item of model part
    /// `part` refers to (`objectid`, and `p:path` in the root model), with
    /// `what` naming the referrer in the error when it is not defined (a
    /// component's object must be defined before it in its own part).
    fn reference(
        &mut self,
        node: &Node<'_, '_>,
        part: usize,
        what: &str,
        component: bool,
    ) -> Result<Key, Error> {
        let id = id_of(node, "objectid")?;
        let path = node
            .attribute((PRODUCTION_NAMESPACE, "path"))
            .map(|p| p.trim().trim_start_matches('/'))
            .filter(|p| !p.eq_ignore_ascii_case(&self.parts[part]));
        let target = match path {
            None => part,
            Some(path) if part == 0 => self.load_part(path)?,
            Some(path) => {
                return Err(error(format!(
                    "{}: {what} refers to object {id} of {path}; only the root model refers to \
                     other model parts",
                    self.parts[part]
                )));
            }
        };
        if !self.objects.contains_key(&(target, id)) {
            let place = if target == part {
                String::new()
            } else {
                format!(" of {}", self.parts[target])
            };
            let before = if component && target == part {
                " before it"
            } else {
                ""
            };
            return Err(error(format!(
                "{what} refers to object {id}{place}, which is not defined{before}"
            )));
        }
        Ok((target, id))
    }

    fn read_root(mut self, model: Node<'_, '_>, root: String) -> Result<ImportRead, Error> {
        let scale = check_model(&model, &root)?;
        self.parts.push(root);
        let mut title = None;
        let mut items = Vec::new();
        for child in model.children().filter(Node::is_element) {
            if core(&child, "metadata") && child.attribute("name") == Some("Title") {
                title = child
                    .text()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                    .map(str::to_owned);
            } else if core(&child, "resources") {
                self.read_resources(&child, 0, scale)?;
            } else if core(&child, "build") {
                for item in child.children().filter(|n| core(n, "item")) {
                    let key = self.reference(&item, 0, "a build item", false)?;
                    items.push((key, transform(&item, scale)?));
                }
            }
        }
        if items.is_empty() {
            return Err(error("the build has no items"));
        }
        let settings = SettingsNames::read(&self.archive);
        let mut objects = self.objects;
        let named = |key: Key, settings: &SettingsNames, objects: &HashMap<Key, Object>| {
            objects[&key]
                .name()
                .map(str::to_owned)
                .or_else(|| settings.name(key, (key.0 == 0).then_some(key.1)))
                .unwrap_or_else(|| format!("Object {}", key.1))
        };
        // Every placement of every mesh object, through the components; with
        // several build items, each item's (mitcad#856).
        let mut parts: Vec<Part> = Vec::new();
        let mut materials: Vec<Option<String>> = Vec::new();
        let mut warnings = Vec::new();
        let mut part_of: HashMap<Key, usize> = HashMap::new();
        let mut built: Vec<BuildObject> = items
            .iter()
            .map(|(key, _)| BuildObject {
                name: named(*key, &settings, &objects),
                components: Vec::new(),
            })
            .collect();
        // (object, placement, depth, build item, the root model's object
        // it is placed through)
        let mut stack: Vec<(Key, Placement, usize, usize, Option<u32>)> = items
            .iter()
            .enumerate()
            .rev()
            .map(|(item, &(key, p))| (key, p, 0, item, None))
            .collect();
        while let Some((key, placement, depth, item, owner)) = stack.pop() {
            let owner = if key.0 == 0 { Some(key.1) } else { owner };
            let index = match part_of.get(&key) {
                Some(&index) => Some(index),
                None if matches!(objects[&key], Object::Mesh { .. }) => {
                    // The part's name: its own, else the settings', else
                    // the name of the root model's object that places it.
                    let own = objects[&key].name().map(str::to_owned);
                    let name = own
                        .or_else(|| settings.name(key, owner))
                        .or_else(|| {
                            owner
                                .filter(|o| (0, *o) != key)
                                .and_then(|o| objects.get(&(0, o)))
                                .and_then(Object::name)
                                .map(str::to_owned)
                        })
                        .unwrap_or_else(|| format!("Object {}", key.1));
                    let Some(Object::Mesh {
                        color,
                        material,
                        mesh,
                        problem,
                        ..
                    }) = objects.get_mut(&key)
                    else {
                        unreachable!("a mesh object");
                    };
                    if let Some(problem) = problem.take() {
                        warnings.push(format!("{name}: {problem}"));
                    }
                    parts.push(Part {
                        name,
                        color: *color,
                        mesh: std::mem::take(mesh),
                        placements: Vec::new(),
                        metadata: Vec::new(),
                    });
                    materials.push(material.take());
                    part_of.insert(key, parts.len() - 1);
                    Some(parts.len() - 1)
                }
                None => None,
            };
            if let Some(index) = index {
                parts[index].placements.push(placement);
                built[item]
                    .components
                    .push((index, parts[index].placements.len() - 1));
                continue;
            }
            let Object::Components { components, .. } = &objects[&key] else {
                continue;
            };
            if depth >= MAX_DEPTH {
                return Err(error("components nest too deep"));
            }
            for &(child, inner) in components.iter().rev() {
                stack.push((child, placement.after(&inner), depth + 1, item, owner));
            }
        }
        let first = built[0].name.clone();
        // One item is the model's one object.
        if built.len() == 1 {
            built.clear();
        }
        Ok(ImportRead {
            model: Model {
                name: title.unwrap_or(first),
                parts,
                objects: built,
                ..Model::default()
            },
            materials,
            warnings,
        })
    }

    /// The resources of model part `part` (in `scale` millimetres per
    /// unit): its base materials and objects.
    fn read_resources(
        &mut self,
        resources: &Node<'_, '_>,
        part: usize,
        scale: f64,
    ) -> Result<(), Error> {
        let mut materials: Materials = HashMap::new();
        for resource in resources.children().filter(Node::is_element) {
            if core(&resource, "basematerials") {
                let id = id_of(&resource, "id")?;
                let colors = resource
                    .children()
                    .filter(|n| core(n, "base"))
                    .map(|base| {
                        let name = base.attribute("name").map(str::to_owned);
                        color(base.attribute("displaycolor").unwrap_or("")).map(|c| (name, c))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if materials.insert(id, colors).is_some() || self.objects.contains_key(&(part, id))
                {
                    return Err(error(format!("resource id {id} is used twice")));
                }
            } else if core(&resource, "object") {
                let id = id_of(&resource, "id")?;
                if self.objects.contains_key(&(part, id)) || materials.contains_key(&id) {
                    return Err(error(format!("resource id {id} is used twice")));
                }
                let object = self.read_object(&resource, part, id, scale, &materials)?;
                self.objects.insert((part, id), object);
            }
        }
        Ok(())
    }

    fn read_object(
        &mut self,
        object: &Node<'_, '_>,
        part: usize,
        id: u32,
        scale: f64,
        materials: &Materials,
    ) -> Result<Object, Error> {
        let name = object.attribute("name").map(str::to_owned);
        let label = name.clone().unwrap_or_else(|| format!("Object {id}"));
        let kind = object.attribute("type").unwrap_or("model");
        if let Some(mesh) = object.children().find(|n| core(n, "mesh")) {
            let mut out = Mesh::default();
            for list in mesh.children().filter(Node::is_element) {
                if core(&list, "vertices") {
                    for vertex in list.children().filter(|n| core(n, "vertex")) {
                        out.vertices.push([
                            number(&vertex, "x")? * scale,
                            number(&vertex, "y")? * scale,
                            number(&vertex, "z")? * scale,
                        ]);
                    }
                } else if core(&list, "triangles") {
                    for triangle in list.children().filter(|n| core(n, "triangle")) {
                        out.triangles.push([
                            id_of(&triangle, "v1")?,
                            id_of(&triangle, "v2")?,
                            id_of(&triangle, "v3")?,
                        ]);
                        let color = self.triangle_color(&triangle, object, id, materials)?;
                        out.triangle_colors.push(color);
                    }
                }
            }
            // No triangle of a colour of its own: none listed.
            if out.triangle_colors.iter().all(Option::is_none) {
                out.triangle_colors.clear();
            }
            let problem = if self.lenient {
                lenient_mesh(&mut out, &label, id, kind)?
            } else {
                strict_mesh(&out, &label, id, kind)?;
                None
            };
            let (color, material) = match object.attribute("pid") {
                None => (None, None),
                Some(_) => match self.base_material(object, id, materials) {
                    Ok((material, color)) => (Some(color), material),
                    // The import takes the part without its colour.
                    Err(_) if self.lenient => (None, None),
                    Err(e) => return Err(e),
                },
            };
            return Ok(Object::Mesh {
                name,
                color,
                material,
                mesh: out,
                problem,
            });
        }
        let Some(list) = object.children().find(|n| core(n, "components")) else {
            return Err(error(format!(
                "object {id} ({label}) has neither a mesh nor components"
            )));
        };
        let mut components = Vec::new();
        for component in list.children().filter(|n| core(n, "component")) {
            let what = format!("object {id} ({label})");
            let key = self.reference(&component, part, &what, true)?;
            components.push((key, transform(&component, scale)?));
        }
        Ok(Object::Components { name, components })
    }

    /// A triangle's colour of its own (mitcad#2385): the base material its
    /// `p1` names in the group of its `pid`, else of its object's `pid`
    /// (the colour of its first corner, which [`crate::write`] gives all
    /// three). None without `p1`, and for [`read_for_import`] where the
    /// group is no base material group (a colour group of the Materials
    /// extension) or has no such material.
    fn triangle_color(
        &self,
        triangle: &Node<'_, '_>,
        object: &Node<'_, '_>,
        id: u32,
        materials: &Materials,
    ) -> Result<Option<[f64; 3]>, Error> {
        if triangle.attribute("p1").is_none() {
            return Ok(None);
        }
        let found = (|| {
            let group = match triangle.attribute("pid") {
                Some(_) => id_of(triangle, "pid")?,
                None => id_of(object, "pid")?,
            };
            let index = id_of(triangle, "p1")?;
            let colors = materials.get(&group).ok_or_else(|| {
                error(format!(
                    "a triangle of object {id} refers to material group {group}, which is not \
                     defined"
                ))
            })?;
            colors.get(index as usize).map(|c| c.1).ok_or_else(|| {
                error(format!(
                    "a triangle of object {id} refers to material {index} of group {group}, \
                     which has {}",
                    colors.len()
                ))
            })
        })();
        match found {
            Ok(color) => Ok(Some(color)),
            // The import takes the triangle in its part's colour.
            Err(_) if self.lenient => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// The base material an object's `pid` and `pindex` name: its name and
    /// colour.
    fn base_material(
        &self,
        object: &Node<'_, '_>,
        id: u32,
        materials: &Materials,
    ) -> Result<(Option<String>, [f64; 3]), Error> {
        let group = id_of(object, "pid")?;
        let index = object
            .attribute("pindex")
            .map_or(Ok(0), |_| id_of(object, "pindex"))?;
        let colors = materials.get(&group).ok_or_else(|| {
            error(format!(
                "object {id} refers to material group {group}, which is not defined"
            ))
        })?;
        colors.get(index as usize).cloned().ok_or_else(|| {
            error(format!(
                "object {id} refers to material {index} of group {group}, which has {}",
                colors.len()
            ))
        })
    }
}

/// [`read`]'s checks of a mesh: closed and facing outwards for an object
/// of type `model`, its vertex indices in range for the others.
fn strict_mesh(mesh: &Mesh, label: &str, id: u32, kind: &str) -> Result<(), Error> {
    if kind == "model" {
        mesh.check_closed()
            .map_err(|e| error(format!("object {id} ({label}): {e}")))?;
        if mesh.volume() <= 0.0 {
            return Err(error(format!(
                "object {id} ({label}): its triangles face inwards"
            )));
        }
    } else if let Some(t) = mesh
        .triangles
        .iter()
        .find(|t| t.iter().any(|&v| v as usize >= mesh.vertices.len()))
    {
        return Err(error(format!(
            "object {id} ({label}): triangle {t:?} refers to a missing vertex"
        )));
    }
    Ok(())
}

/// [`read_for_import`]'s checks of a mesh: an index out of range is
/// refused; triangles that use a vertex twice are left out; a mesh of an
/// object of type `model` that is not closed or faces inwards is kept. What
/// it has to say, in words.
fn lenient_mesh(
    mesh: &mut Mesh,
    label: &str,
    id: u32,
    kind: &str,
) -> Result<Option<String>, Error> {
    let count = mesh.vertices.len();
    if let Some(i) = mesh
        .triangles
        .iter()
        .position(|t| t.iter().any(|&v| v as usize >= count))
    {
        return Err(error(format!(
            "object {id} ({label}): triangle {i} refers to a vertex beyond the {count} vertices"
        )));
    }
    let before = mesh.triangles.len();
    let kept = |t: &[u32; 3]| t[0] != t[1] && t[1] != t[2] && t[2] != t[0];
    // The triangles' own colours go with them (mitcad#2385).
    if !mesh.triangle_colors.is_empty() {
        let mut colors = std::mem::take(&mut mesh.triangle_colors).into_iter();
        mesh.triangle_colors = mesh
            .triangles
            .iter()
            .filter_map(|t| {
                let color = colors.next().flatten();
                kept(t).then_some(color)
            })
            .collect();
    }
    mesh.triangles.retain(kept);
    let mut said = Vec::new();
    let dropped = before - mesh.triangles.len();
    if dropped > 0 {
        said.push(format!(
            "triangles that use a vertex twice are left out: {dropped}"
        ));
    }
    if kind == "model" && !mesh.triangles.is_empty() {
        let convert = "it came in as a mesh body, which Convert Mesh cannot make a solid";
        match mesh.check_closed() {
            Err(why) => said.push(format!("its mesh is not closed ({why}): {convert}")),
            Ok(()) if mesh.volume() <= 0.0 => {
                said.push(format!("its triangles face inwards: {convert}"));
            }
            Ok(()) => {}
        }
    }
    Ok((!said.is_empty()).then(|| said.join("; ")))
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

/// A component's or build item's transform, its move in millimetres
/// (`scale` per unit of its model part); the identity when it has none.
fn transform(node: &Node<'_, '_>, scale: f64) -> Result<Placement, Error> {
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
    let mut placement = Placement::from_3mf(values);
    placement.translation = placement.translation.map(|c| c * scale);
    Ok(placement)
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
