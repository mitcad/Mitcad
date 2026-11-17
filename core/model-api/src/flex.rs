// SPDX-License-Identifier: MIT
//! The commands and queries of Flex items (mitcad#1713; `commands.md`,
//! "Flex"; the feature is `mitcad_model::features::flex`). An item is made
//! with `add_feature` and a `flex` definition; the `flex.*` commands here
//! change one thing of it each, as one undo step
//! (`Document::edit_flex`): a node added at an end or between two, moved,
//! attached to something else, deleted or twisted, the item or its options
//! set. The `flex` query reads what the item's last evaluation found (or
//! the previewed definition's): the nodes where they are, the route as a
//! polyline, its length, the faces it runs on and its problems with their
//! places; `flex_items` lists the items with their lengths (a cut list's
//! rows); `end_ports` lists the ends of pipes and Flex items where an item
//! begins or ends.

use std::path::Path;

use serde::{Deserialize, Deserializer};
use serde_json::{Value, json};

use super::ApiError;
use super::dispatch::named_enum;
use mitcad_model::features::flex::{FlexDef, FlexNode, FlexOutput, NodePlace, NodeSupport};
use mitcad_model::features::{OccurrencePath, ProfileRef, ValueInput};
use mitcad_model::flex::{FlexSection, FlexType};
use mitcad_model::flex_library;
use mitcad_model::ids::{BodyUid, ComponentUid, FeatureUid};
use mitcad_model::kernel::Kernel;
use mitcad_model::{Document, FeatureStatus, FlexTypeWrite, GeomRef, OccurrenceLink, PortsSeen};

/// Where a new node goes: `"end"` (the default), `"start"` or
/// `{"after": "n2"}`.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PlaceInput {
    Start,
    #[default]
    End,
    After(String),
}

/// A field that tells `null` (take the value away) from left out (leave it
/// as it is): `Some(None)` and `None`.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

named_enum! {
    #[derive(Debug, Deserialize)]
    #[serde(tag = "cmd", rename_all = "snake_case", deny_unknown_fields)]
    pub(super) enum FlexCommand {
        "flex.add_node" => AddNode {
            /// The Flex item (the feature's uid).
            flex: FeatureUid,
            at: [f64; 3],
            #[serde(default)]
            /// A reference, with `occurrence` and `context` for geometry of
            /// another component.
            on: Option<Value>,
            #[serde(default)]
            port: Option<Value>,
            #[serde(default)]
            twist: f64,
            #[serde(default)]
            place: PlaceInput,
        },
        "flex.move_node" => MoveNode {
            flex: FeatureUid,
            node: String,
            at: [f64; 3],
        },
        "flex.attach_node" => AttachNode {
            flex: FeatureUid,
            node: String,
            /// One of `on`, `port` and `free`.
            #[serde(default)]
            /// A reference, with `occurrence` and `context` for geometry of
            /// another component.
            on: Option<Value>,
            #[serde(default)]
            port: Option<Value>,
            #[serde(default)]
            free: bool,
            /// Where on the new face or edge; where the node is now when
            /// left out.
            #[serde(default)]
            at: Option<[f64; 3]>,
        },
        "flex.delete_node" => DeleteNode {
            flex: FeatureUid,
            node: String,
        },
        "flex.set_twist" => SetTwist {
            flex: FeatureUid,
            node: String,
            twist: f64,
        },
        "flex.set_item" => SetItem {
            flex: FeatureUid,
            /// The whole item, as in the definition.
            #[serde(default)]
            item: Option<FlexType>,
            /// The id of a Flex type, copied into the item: a built-in one,
            /// or one of `library`.
            #[serde(default, rename = "type")]
            type_id: Option<String>,
            /// The library of `type`: `built_in` (when left out), `user`
            /// or `project`.
            #[serde(default)]
            library: Option<String>,
            /// The project's folder, for a type of the project's library.
            #[serde(default)]
            project: Option<String>,
            #[serde(default, deserialize_with = "present")]
            profile: Option<Option<ProfileRef>>,
            #[serde(default, deserialize_with = "present")]
            bend_radius: Option<Option<ValueInput>>,
            #[serde(default, deserialize_with = "present")]
            bend_radius_edge: Option<Option<ValueInput>>,
            #[serde(default, deserialize_with = "present")]
            fixed_length: Option<Option<ValueInput>>,
        },
        "flex.save_type" => SaveType(TypeCommand),
        "flex.edit_type" => EditType(TypeCommand),
        "flex.delete_type" => DeleteType {
            /// `user` or `project`.
            library: String,
            id: String,
            /// The project's folder, for the project's library.
            #[serde(default)]
            project: Option<String>,
        },
        // A Flex item let hang (mitcad#1713, `flex_hang.rs`).
        "flex.hang" => Hang(super::flex_hang::HangCommand),
        "flex.hang_step" => HangStep(super::flex_hang::HangStepCommand),
        "flex.hang_set" => HangSet(super::flex_hang::HangSetCommand),
        "flex.hang_lock" => HangLock {},
        "flex.hang_cancel" => HangCancel {},
    }
}

/// The `flex` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FlexQuery {
    /// The Flex item; left out with `preview` for a previewed `add_feature`.
    #[serde(default)]
    uid: Option<FeatureUid>,
    /// The previewed definition's evaluation instead of the item's.
    #[serde(default)]
    preview: bool,
    /// Also the route's samples with their directions.
    #[serde(default)]
    frames: bool,
}

/// The `end_ports` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EndPortsQuery {
    /// Only this pipe's or Flex item's.
    #[serde(default)]
    feature: Option<FeatureUid>,
    /// Only the ports of this body: of the body under the pointer.
    #[serde(default)]
    body: Option<BodyUid>,
    /// The occurrence path the ports' component was picked at: the ports
    /// are then given in the coordinates of `component`.
    #[serde(default)]
    occurrence: Option<OccurrencePath>,
    /// The path `component` is seen at; its first when left out.
    #[serde(default)]
    context: Option<OccurrencePath>,
    /// The component of the item that will use the ports (uid or name);
    /// the active one when left out.
    #[serde(default)]
    component: Option<String>,
}

/// Runs a `flex.*` command: the item as it is now (`flex`, `node`,
/// `nodes`, `length`, `warnings`).
pub(super) fn run_flex_command<K: Kernel>(
    doc: &mut Document<K>,
    command: FlexCommand,
) -> Result<Value, ApiError> {
    let (uid, node) = match command {
        FlexCommand::AddNode {
            flex,
            at,
            on,
            port,
            twist,
            place,
        } => {
            let place = match place {
                PlaceInput::Start => NodePlace::Start,
                PlaceInput::End => NodePlace::End,
                PlaceInput::After(id) => NodePlace::After(id),
            };
            let on = picked(doc, flex, on, "on")?;
            let port = picked(doc, flex, port, "port")?;
            let node = FlexNode {
                id: String::new(),
                at,
                on: on.as_ref().map(|picked| picked.reference.clone()),
                port: port.as_ref().map(|picked| picked.reference.clone()),
                twist,
            };
            let id = doc.edit_flex(flex, |def| {
                let id = def.add_node(node, &place)?;
                for picked in [&on, &port].into_iter().flatten() {
                    picked.link_in(def);
                }
                Ok(id)
            })?;
            (flex, Some(id))
        }
        FlexCommand::MoveNode { flex, node, at } => {
            doc.edit_flex(flex, |def| def.move_node(&node, at))?;
            (flex, Some(node))
        }
        FlexCommand::AttachNode {
            flex,
            node,
            on,
            port,
            free,
            at,
        } => {
            let on = picked(doc, flex, on, "on")?;
            let port = picked(doc, flex, port, "port")?;
            let (support, placed) = match (on, port, free) {
                (Some(on), None, false) => (NodeSupport::On(on.reference.clone()), Some(on)),
                (None, Some(port), false) => {
                    (NodeSupport::Port(port.reference.clone()), Some(port))
                }
                (None, None, true) => (NodeSupport::Free, None),
                _ => {
                    return Err(ApiError(
                        "flex.attach_node takes one of on, port and free".to_owned(),
                    ));
                }
            };
            // Left out: where the node is now.
            let at = at.or_else(|| {
                let output = doc.flex_output(flex)?;
                output.nodes.iter().find(|n| n.id == node).map(|n| n.point)
            });
            doc.edit_flex(flex, |def| {
                def.attach_node(&node, support, at)?;
                if let Some(picked) = &placed {
                    picked.link_in(def);
                }
                Ok(())
            })?;
            (flex, Some(node))
        }
        FlexCommand::DeleteNode { flex, node } => {
            doc.edit_flex(flex, |def| def.delete_node(&node))?;
            (flex, None)
        }
        FlexCommand::SetTwist { flex, node, twist } => {
            doc.edit_flex(flex, |def| def.set_twist(&node, twist))?;
            (flex, Some(node))
        }
        FlexCommand::SetItem {
            flex,
            item,
            type_id,
            library,
            project,
            profile,
            bend_radius,
            bend_radius_edge,
            fixed_length,
        } => {
            // A type by its id is copied into the item, with its origin:
            // a built-in one unless a library is named.
            let item = match type_id {
                Some(id) => {
                    if item.is_some() {
                        return Err(ApiError(
                            "flex.set_item takes the item or a type's id, not both".to_owned(),
                        ));
                    }
                    let library = library.as_deref().unwrap_or(flex_library::BUILT_IN);
                    let project = project.as_deref().map(Path::new);
                    let found =
                        flex_library::find(Some(library), &id, project).map_err(ApiError)?;
                    Some(found.copy())
                }
                None => item,
            };
            doc.edit_flex(flex, |def| {
                if let Some(item) = item {
                    item.check()?;
                    def.item = item;
                }
                if let Some(profile) = profile {
                    def.profile = profile;
                }
                if let Some(value) = bend_radius {
                    def.bend_radius = value;
                }
                if let Some(value) = bend_radius_edge {
                    def.bend_radius_edge = value;
                }
                if let Some(value) = fixed_length {
                    def.fixed_length = value;
                }
                Ok(())
            })?;
            (flex, None)
        }
        FlexCommand::SaveType(command) => return save_type(doc, &command),
        FlexCommand::EditType(command) => return edit_type(doc, &command),
        FlexCommand::DeleteType {
            library,
            id,
            project,
        } => {
            let project = project.as_deref().map(Path::new);
            let deleted = doc.delete_flex_type(&library, project, &id)?;
            return Ok(type_written(&deleted));
        }
        // A Flex item let hang (mitcad#1713, `flex_hang.rs`).
        FlexCommand::Hang(command) => return super::flex_hang::hang(doc, command),
        FlexCommand::HangStep(command) => return super::flex_hang::step(doc, command),
        FlexCommand::HangSet(command) => return super::flex_hang::set(doc, command),
        FlexCommand::HangLock {} => {
            let locked = doc.flex_hang_lock()?;
            let mut value = summary(doc, locked.flex, None);
            value["deviation"] = json!(locked.deviation);
            return Ok(value);
        }
        FlexCommand::HangCancel {} => return Ok(super::flex_hang::cancel(doc)),
    };
    Ok(summary(doc, uid, node))
}

/// `flex.save_type` and `flex.edit_type`: where the type goes or is, and
/// its fields. For a field that may be left out of a type, `null` takes
/// it away.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct TypeCommand {
    /// `user` (the default of `flex.save_type`) or `project`.
    #[serde(default)]
    library: Option<String>,
    /// The project's folder, for the project's library.
    #[serde(default)]
    project: Option<String>,
    /// The type's id; a saved one's is the next free one made from its
    /// name when left out.
    #[serde(default)]
    id: Option<String>,
    /// `flex.save_type`: a Flex item of the design whose item is saved.
    #[serde(default)]
    flex: Option<FeatureUid>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    section: Option<FlexSection>,
    #[serde(default, deserialize_with = "present")]
    wall: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    bend_radius: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    bend_radius_edge: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    max_twist: Option<Option<f64>>,
    #[serde(default, deserialize_with = "present")]
    appearance: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    material: Option<Option<String>>,
    #[serde(default, deserialize_with = "present")]
    mass_per_length: Option<Option<f64>>,
}

impl TypeCommand {
    /// Whether the command gives a field of the type.
    fn has_fields(&self) -> bool {
        self.name.is_some()
            || self.section.is_some()
            || self.wall.is_some()
            || self.bend_radius.is_some()
            || self.bend_radius_edge.is_some()
            || self.max_twist.is_some()
            || self.appearance.is_some()
            || self.material.is_some()
            || self.mass_per_length.is_some()
    }

    /// The command's fields into `item`.
    fn apply(&self, item: &mut FlexType) {
        if let Some(name) = &self.name {
            item.name = name.trim().to_owned();
        }
        if let Some(section) = &self.section {
            item.section = section.clone();
        }
        if let Some(wall) = self.wall {
            item.wall = wall;
        }
        if let Some(radius) = self.bend_radius {
            item.bend_radius = radius;
        }
        if let Some(radius) = self.bend_radius_edge {
            item.bend_radius_edge = radius;
        }
        if let Some(twist) = self.max_twist {
            item.max_twist = twist;
        }
        if let Some(appearance) = &self.appearance {
            item.appearance.clone_from(appearance);
        }
        if let Some(material) = &self.material {
            item.material.clone_from(material);
        }
        if let Some(mass) = self.mass_per_length {
            item.mass_per_length = mass;
        }
    }
}

/// What a command that wrote a type answers with.
fn type_written(written: &FlexTypeWrite) -> Value {
    json!({
        "id": written.id,
        "library": written.library,
        "version": written.version,
        "path": written.path.to_string_lossy(),
    })
}

/// `flex.save_type`: a new type of the user's or the project's library,
/// from the fields given or from a Flex item of the design (the fields
/// given then change the copy saved).
fn save_type<K: Kernel>(doc: &mut Document<K>, command: &TypeCommand) -> Result<Value, ApiError> {
    let mut item = match command.flex {
        Some(uid) => doc.flex_item_as_type(uid)?,
        None => {
            let (Some(name), Some(section)) = (&command.name, &command.section) else {
                return Err(ApiError(
                    "flex.save_type needs the type's name and section, or a flex item to save"
                        .to_owned(),
                ));
            };
            FlexType {
                name: name.trim().to_owned(),
                section: section.clone(),
                wall: None,
                bend_radius: None,
                bend_radius_edge: None,
                max_twist: None,
                appearance: None,
                material: None,
                mass_per_length: None,
                origin: None,
            }
        }
    };
    command.apply(&mut item);
    let library = command.library.as_deref().unwrap_or(flex_library::USER);
    let project = command.project.as_deref().map(Path::new);
    let written = doc.save_flex_type(library, project, command.id.as_deref(), &item)?;
    Ok(type_written(&written))
}

/// `flex.edit_type`: the fields given changed in a type of the user's or
/// the project's library.
fn edit_type<K: Kernel>(doc: &mut Document<K>, command: &TypeCommand) -> Result<Value, ApiError> {
    let (Some(library), Some(id)) = (&command.library, &command.id) else {
        return Err(ApiError(
            "flex.edit_type needs the type's library and id".to_owned(),
        ));
    };
    if command.flex.is_some() {
        return Err(ApiError(
            "flex.edit_type takes the fields that change, not a flex item".to_owned(),
        ));
    }
    if !command.has_fields() {
        return Err(ApiError(
            "flex.edit_type needs a field of the type to change".to_owned(),
        ));
    }
    let project = command.project.as_deref().map(Path::new);
    let written = doc.edit_flex_type(library, project, id, |item| {
        command.apply(item);
        Ok(())
    })?;
    Ok(type_written(&written))
}

/// The `flex_types` query.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct FlexTypesQuery {
    /// Only the types whose name or id holds every word.
    #[serde(default)]
    text: Option<String>,
    /// The project's folder, when the design is in one.
    #[serde(default)]
    project: Option<String>,
}

/// The `flex_types` query: the built-in types, then the user's and the
/// project's, each the item's fields with its `id`, `library`, `built_in`
/// and `version`; and what could not be read (`problems`).
pub(super) fn flex_types_json(query: &FlexTypesQuery) -> Value {
    let (types, problems) = flex_library::list(query.project.as_deref().map(Path::new));
    let text = query.text.as_deref().unwrap_or("");
    let rows: Vec<Value> = types
        .iter()
        .filter(|found| found.matches(text))
        .map(|found| {
            let mut row = json!(found.item);
            row["id"] = json!(found.id);
            row["library"] = json!(found.library);
            row["built_in"] = json!(found.is_built_in());
            row["version"] = json!(found.version);
            row
        })
        .collect();
    json!({"types": rows, "problems": problems})
}

/// A reference of a node command: as the definition writes it, or with
/// `occurrence` (and `context`), as `sketch.project` and the 3D sketch's
/// sources take them, for geometry of another component picked where an
/// occurrence shows it. `link` is then where that geometry is, seen from
/// the item's component (None for the item's own component).
struct Picked {
    reference: GeomRef,
    link: Option<OccurrenceLink>,
    /// Whether the command said where the geometry was picked: the link
    /// of that geometry is then set, or taken away.
    placed: bool,
}

fn picked_in<K: Kernel>(
    doc: &Document<K>,
    component: ComponentUid,
    value: Option<Value>,
    what: &str,
) -> Result<Option<Picked>, ApiError> {
    let Some(mut value) = value.filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    let mut link = None;
    let mut placed = false;
    if let Value::Object(map) = &mut value
        && (map.contains_key("occurrence") || map.contains_key("context"))
    {
        let mut path = |field: &str| -> Result<Option<OccurrencePath>, ApiError> {
            map.remove(field)
                .filter(|v| !v.is_null())
                .map(serde_json::from_value)
                .transpose()
                .map_err(|e| ApiError(format!("a node's {what}: \"{field}\": {e}")))
        };
        let occurrence = path("occurrence")?;
        let context = path("context")?;
        let Some(occurrence) = occurrence else {
            return Err(ApiError(format!(
                "a node's {what}: \"context\" needs its \"occurrence\""
            )));
        };
        link = mitcad_model::internal::links::link_for(
            doc.assembly(),
            component,
            &occurrence.0,
            context.as_ref().map(|c| &c.0[..]),
        )
        .map_err(|e| ApiError(format!("a node's {what}: {e}")))?;
        placed = true;
    }
    let reference: GeomRef =
        serde_json::from_value(value).map_err(|e| ApiError(format!("a node's {what}: {e}")))?;
    Ok(Some(Picked {
        reference,
        link,
        placed,
    }))
}

impl Picked {
    /// Records the picked geometry's link in the definition.
    fn link_in(&self, def: &mut FlexDef<ValueInput>) {
        if self.placed {
            def.set_link(&self.reference, self.link.clone());
        }
    }
}

/// [`picked_in`] for a node of the Flex item `flex`.
fn picked<K: Kernel>(
    doc: &Document<K>,
    flex: FeatureUid,
    value: Option<Value>,
    what: &str,
) -> Result<Option<Picked>, ApiError> {
    let component = doc
        .feature(flex)
        .map(|entry| entry.component)
        .ok_or_else(|| ApiError(format!("feature {flex} does not exist")))?;
    picked_in(doc, component, value, what)
}

/// A Flex definition of an `add_feature` or `edit_feature` command (or of
/// its preview) whose nodes say where geometry of another component was
/// picked (`on` or `port` with `occurrence` and `context`, as the node
/// commands take them): those fields become the definition's `links` and
/// `datum_links`, so that the definition reads as it is stored. Any other
/// command is left as it is.
pub(super) fn place_definition<K: Kernel>(
    doc: &Document<K>,
    command: &mut Value,
    preview: bool,
) -> Result<(), ApiError> {
    use super::components::ComponentsCrateOps;
    let cmd = command.get("cmd").and_then(Value::as_str);
    let flex = command.pointer("/def/type").and_then(Value::as_str) == Some("flex");
    if !flex || !matches!(cmd, Some("add_feature" | "edit_feature")) {
        return Ok(());
    }
    // The route alone is for previews: a stored item has its body.
    if !preview && command.pointer("/def/route_only") == Some(&Value::Bool(true)) {
        return Err(ApiError(
            "route_only is for a previewed flex item: preview the command, or leave it out"
                .to_owned(),
        ));
    }
    let component = if cmd == Some("edit_feature") {
        command
            .get("uid")
            .and_then(Value::as_str)
            .and_then(|uid| uid.parse::<FeatureUid>().ok())
            .and_then(|uid| doc.feature(uid))
            .map(|entry| entry.component)
    } else {
        match command.get("component").and_then(Value::as_str) {
            Some(text) => doc.component_ref(text).ok(),
            None => Some(doc.active_component()),
        }
    };
    // Without its component the command is refused in its own words.
    let Some(component) = component else {
        return Ok(());
    };
    let Some(nodes) = command
        .pointer_mut("/def/nodes")
        .and_then(Value::as_array_mut)
    else {
        return Ok(());
    };
    let mut links: Vec<(String, Value)> = Vec::new();
    let mut datum_links: Vec<(String, Value)> = Vec::new();
    for node in nodes {
        for what in ["on", "port"] {
            let placed = node.get(what).is_some_and(|reference| {
                reference.get("occurrence").is_some() || reference.get("context").is_some()
            });
            if !placed {
                continue;
            }
            let Some(found) = picked_in(doc, component, node.get(what).cloned(), what)? else {
                continue;
            };
            // The reference as the definition stores it.
            if let Some(Value::Object(map)) = node.get_mut(what) {
                map.remove("occurrence");
                map.remove("context");
            }
            let Some(link) = &found.link else {
                continue;
            };
            let link = serde_json::to_value(link).expect("a link serializes");
            match &found.reference {
                GeomRef::Face { body, .. }
                | GeomRef::Edge { body, .. }
                | GeomRef::Vertex { body, .. }
                | GeomRef::Body(body) => links.push((body.to_string(), link)),
                GeomRef::Datum(feature)
                | GeomRef::SketchPoint {
                    sketch: feature, ..
                } => datum_links.push((feature.to_string(), link)),
                _ => {}
            }
        }
    }
    let Some(Value::Object(def)) = command.get_mut("def") else {
        return Ok(());
    };
    for (field, found) in [("links", links), ("datum_links", datum_links)] {
        if found.is_empty() {
            continue;
        }
        let map = def.entry(field).or_insert_with(|| json!({}));
        if let Value::Object(map) = map {
            map.extend(found);
        }
    }
    Ok(())
}

/// What every `flex.*` command answers with.
fn summary<K: Kernel>(doc: &Document<K>, uid: FeatureUid, node: Option<String>) -> Value {
    let mut value = json!({"flex": uid, "warnings": doc.warnings(uid)});
    if let Some(node) = node {
        value["node"] = json!(node);
    }
    if let Ok(def) = doc.flex_def(uid) {
        let ids: Vec<&str> = def.nodes.iter().map(|n| n.id.as_str()).collect();
        value["nodes"] = json!(ids);
    }
    if let Some(output) = doc.flex_output(uid) {
        value["length"] = json!(output.length);
    }
    value
}

/// What an evaluation found, into the `flex` query's answer: the nodes
/// (with each one's stored reference and twist where the definition is
/// given), the route, its length, the faces it runs on and its problems.
fn add_route(value: &mut Value, output: &FlexOutput, def: Option<&FlexDef>, frames: bool) {
    let nodes: Vec<Value> = output
        .nodes
        .iter()
        .map(|node| {
            let mut item = json!({
                "id": node.id,
                "point": node.point,
                "normal": node.normal,
                "along": node.along,
                "support": node.support,
                "lost": node.lost,
            });
            if let Some(stored) = def.and_then(|d| d.nodes.iter().find(|n| n.id == node.id)) {
                item["ref"] = json!(stored.port.as_ref().or(stored.on.as_ref()));
                item["twist"] = json!(stored.twist);
            }
            item
        })
        .collect();
    value["nodes"] = json!(nodes);
    value["length"] = json!(output.length);
    value["fixed_length"] = json!(output.fixed_length);
    value["slack"] = json!(output.slack());
    value["bend_radius"] = json!(output.bend_radius);
    value["bend_radius_edge"] = json!(output.bend_radius_edge);
    // How tightly the route bends at its tightest, against the radii
    // above: of a section that is not round over its flat side and over
    // its edge apart.
    let bend = |tightest: Option<(f64, f64, [f64; 3])>| match tightest {
        Some((radius, along, point)) => {
            json!({"radius": radius, "along": along, "point": point})
        }
        None => Value::Null,
    };
    value["smallest_bend"] = bend(output.smallest_bend());
    value["smallest_bend_edge"] = bend(output.smallest_bend_edge());
    value["polyline"] = json!(output.polyline());
    if frames {
        let samples: Vec<Value> = output
            .samples
            .iter()
            .map(|sample| {
                json!({
                    "along": sample.along,
                    "point": sample.point,
                    "tangent": sample.tangent,
                    "normal": sample.normal,
                })
            })
            .collect();
        value["frames"] = json!(samples);
    }
    let runs: Vec<Value> = output
        .runs
        .iter()
        .map(|run| {
            json!({
                "from": run.from,
                "to": run.to,
                "body": run.body.to_string(),
                "face": run.face.to_string(),
            })
        })
        .collect();
    value["runs"] = json!(runs);
    let problems: Vec<Value> = output
        .problems
        .iter()
        .map(|problem| {
            let mut item = json!({
                "kind": problem.kind,
                "message": problem.kind.message(),
                "from": problem.from,
                "to": problem.to,
                "places": problem.places,
            });
            if let Some(node) = &problem.node {
                item["node"] = json!(node);
                // What the lost node was attached to, as stored.
                if let Some(stored) = def.and_then(|d| d.nodes.iter().find(|n| n.id == *node)) {
                    item["was"] = json!(stored.port.as_ref().or(stored.on.as_ref()));
                }
            }
            if let Some(body) = problem.body {
                item["body"] = json!(body.to_string());
            }
            item
        })
        .collect();
    value["problems"] = json!(problems);
}

/// The `flex` query.
pub(super) fn flex_json<K: Kernel>(
    doc: &Document<K>,
    query: &FlexQuery,
) -> Result<Value, ApiError> {
    if query.preview {
        let output = doc
            .preview_flex()
            .ok_or_else(|| ApiError("no flex item is being previewed".to_owned()))?;
        let mut value = json!({"status": "preview"});
        add_route(&mut value, &output, None, query.frames);
        return Ok(value);
    }
    let uid = query
        .uid
        .ok_or_else(|| ApiError("the flex query needs the item's uid".to_owned()))?;
    let def = doc.flex_def(uid)?;
    let name = doc.feature(uid).map(|f| f.name.clone()).unwrap_or_default();
    let status = doc.status(uid);
    let mut value = json!({
        "uid": uid,
        "name": name,
        "status": status.map_or("rolled_back", FeatureStatus::as_str),
        "item": def.item,
    });
    if let Some(FeatureStatus::Failed(error)) = status {
        value["error"] = json!(error.text());
    }
    // The parameter every evaluation sets to the route's length.
    if let Some(id) = def.length {
        value["length_parameter"] = json!(doc.parameters().name(id));
    }
    if let Some(output) = doc.flex_output(uid) {
        add_route(&mut value, &output, Some(def), query.frames);
    }
    Ok(value)
}

/// The `flex_items` query: every evaluated Flex item with its length.
pub(super) fn flex_items_json<K: Kernel>(doc: &Document<K>) -> Value {
    let rows: Vec<Value> = doc
        .flex_items()
        .into_iter()
        .filter_map(|uid| {
            let def = doc.flex_def(uid).ok()?;
            let name = doc.feature(uid)?.name.clone();
            let length = doc.flex_output(uid).map(|output| output.length);
            let mut row = json!({
                "uid": uid,
                "name": name,
                "item": def.item.name,
                "length": length,
                "warnings": doc.warnings(uid).len(),
            });
            if let Some(origin) = &def.item.origin {
                row["type"] = json!(origin.id);
            }
            // Grams: the length in metres times the grams per metre.
            if let (Some(length), Some(mass)) = (length, def.item.mass_per_length) {
                row["mass"] = json!(length / 1000.0 * mass);
            }
            let body = BodyUid::new(uid, 0);
            if doc.body_shape(body).is_some() {
                row["body"] = json!(body.to_string());
            }
            // The parameter that holds the length, for expressions.
            if let Some(id) = def.length {
                row["length_parameter"] = json!(doc.parameters().name(id));
            }
            Some(row)
        })
        .collect();
    json!(rows)
}

/// The `end_ports` query.
pub(super) fn end_ports_json<K: Kernel>(
    doc: &Document<K>,
    query: &EndPortsQuery,
) -> Result<Value, ApiError> {
    use super::components::ComponentsCrateOps;
    let seen = match &query.occurrence {
        Some(occurrence) => {
            let component = match &query.component {
                Some(text) => doc.component_ref(text)?,
                None => doc.active_component(),
            };
            Some(PortsSeen {
                component,
                occurrence: &occurrence.0,
                context: query.context.as_ref().map(|c| &c.0[..]),
            })
        }
        None => None,
    };
    let ports: Vec<Value> = doc
        .end_ports_seen(query.feature, query.body, seen)?
        .into_iter()
        .map(|found| {
            json!({
                "feature": found.feature,
                "name": found.name,
                "kind": found.kind,
                "end": found.end,
                "ref": {"body": found.body.to_string(), "face": found.face.to_string()},
                "position": found.port.position,
                "direction": found.port.direction,
                "x_axis": found.port.x_axis,
                "size": found.port.size,
            })
        })
        .collect();
    Ok(json!(ports))
}
