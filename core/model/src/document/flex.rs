// SPDX-License-Identifier: MIT
//! Flex items in the document (mitcad#1713; `features/flex.rs`): what
//! their last evaluation found ([`Document::flex_output`], the previewed
//! one's [`Document::preview_flex`]), their edits as one undo step each
//! ([`Document::edit_flex`]: a node added, moved, attached again, deleted
//! or twisted, the item or its options set), and the end ports of pipes
//! and Flex items ([`Document::end_ports`]), where an item begins or ends.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::library_items::FileWrite;
use super::{DocState, Document, ModelError, invalid};
use crate::features::flex::{FlexDef, FlexOutput};
use crate::features::{FeatureDef, ValueInput};
use crate::flex::{EndPort, FlexType};
use crate::flex_library::{self, LibraryFlexType};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::links::OccurrenceLinkExt;
use crate::parameters::ParamId;
use crate::print_profile::{free_id, slug};
use crate::topo::FaceName;
use crate::transform::Transform;

/// An end of a pipe or of a Flex item (mitcad#1713): the cap face in the
/// body that holds it at the timeline marker, and the port there.
#[derive(Debug, Clone, PartialEq)]
pub struct EndPortView {
    /// The pipe or the Flex item.
    pub feature: FeatureUid,
    pub name: String,
    /// `pipe` or `flex`.
    pub kind: &'static str,
    /// `start` or `end`.
    pub end: &'static str,
    /// What a node's `port` stores: the body and the cap face.
    pub body: BodyUid,
    pub face: FaceName,
    pub port: EndPort,
}

impl<K: Kernel> Document<K> {
    /// The definition of the Flex item `uid`; refused for another feature.
    pub fn flex_def(&self, uid: FeatureUid) -> Result<&FlexDef, ModelError> {
        let position = self.state.require(uid)?;
        let entry = &self.state.features[position];
        match &entry.def {
            FeatureDef::Flex(def) => Ok(def),
            _ => Err(invalid(crate::notice::said(
                format!("{} is not a flex item", entry.name),
                format!("{} ({uid}) is not a flex item", entry.name),
            ))),
        }
    }

    /// What the last evaluation of the Flex item `uid` found: its nodes
    /// where they are, its route and the route's problems, also when its
    /// solid failed. None when it was not evaluated (suppressed, after the
    /// marker) or failed before its nodes were placed.
    pub fn flex_output(&self, uid: FeatureUid) -> Option<Arc<FlexOutput>> {
        self.feature_result(uid)?.output.as_deref()?.flex.clone()
    }

    /// [`Self::flex_output`] of the previewed feature (a previewed
    /// `add_feature` or `edit_feature` of a Flex item): the route under
    /// the pointer.
    pub fn preview_flex(&self) -> Option<Arc<FlexOutput>> {
        let preview = self.preview.as_ref()?;
        preview
            .result
            .result(preview.uid)?
            .output
            .as_deref()?
            .flex
            .clone()
    }

    /// The Flex items that the last recompute evaluated, in timeline
    /// order.
    pub fn flex_items(&self) -> Vec<FeatureUid> {
        self.state
            .features
            .iter()
            .filter(|entry| matches!(entry.def, FeatureDef::Flex(_)))
            .filter(|entry| {
                self.feature_result(entry.uid)
                    .is_some_and(|result| result.output.is_some())
            })
            .map(|entry| entry.uid)
            .collect()
    }

    /// Changes the Flex item `uid` by `change`, as one undo step (`Edit
    /// Flex1`): `change` gets the definition as a command gives it (its
    /// values by their parameters' names, so that they stay) and what it
    /// returns is returned. Refused, with nothing changed, when `change`
    /// refuses or the changed definition is not valid.
    pub fn edit_flex<T>(
        &mut self,
        uid: FeatureUid,
        change: impl FnOnce(&mut FlexDef<ValueInput>) -> Result<T, String>,
    ) -> Result<T, ModelError> {
        self.edit_flex_checked(uid, change, |_| Ok(()))
    }

    /// [`Self::edit_flex`], refused also when `check` refuses the item's
    /// recomputed result (a hanging item's lock, mitcad#1713: the item made
    /// whole along its new nodes).
    pub(crate) fn edit_flex_checked<T>(
        &mut self,
        uid: FeatureUid,
        change: impl FnOnce(&mut FlexDef<ValueInput>) -> Result<T, String>,
        check: impl FnOnce(&crate::recompute::Recomputed<K::Shape>) -> Result<(), ModelError>,
    ) -> Result<T, ModelError> {
        let params = &self.state.parameters;
        let mut def: FlexDef<ValueInput> = self
            .flex_def(uid)?
            .map_params(&mut |_, id: &ParamId| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
            .expect("names never fail");
        let changed = change(&mut def).map_err(invalid)?;
        // The links of geometry no node is on any more go.
        def.prune_links();
        let def = FeatureDef::Flex(def);
        if let Some(entry) = self.state.entry(uid) {
            self.state.editable(entry.component)?;
        }
        let (made, beside) = self.made_and_beside(uid);
        self.apply_checked(
            |state| {
                state.edit_feature_keeping(uid, &def)?;
                state.place_made_beside(uid, made, beside);
                let name = state.entry(uid).expect("edited").name.clone();
                Ok((format!("Edit {name}"), ()))
            },
            |_, recomputed| check(recomputed),
        )?;
        Ok(changed)
    }

    /// The end ports of the pipes and Flex items at the timeline marker
    /// (of `feature` only, when given): each cap face that a body of the
    /// feature's component still has, with its port as the kernel measures
    /// it ([`Kernel::end_port`]). A cap that is gone, or whose port the
    /// kernel cannot give, is left out.
    pub fn end_ports(&self, feature: Option<FeatureUid>) -> Vec<EndPortView> {
        self.end_ports_in(feature, None)
    }

    /// [`Self::end_ports`], of `body` only when given: only its faces are
    /// looked at then, which is cheap enough for a body under the pointer.
    fn end_ports_in(&self, feature: Option<FeatureUid>, body: Option<BodyUid>) -> Vec<EndPortView> {
        let mut ports = Vec::new();
        for entry in &self.state.features {
            let kind = match &entry.def {
                FeatureDef::Pipe(_) => "pipe",
                FeatureDef::Flex(_) => "flex",
                _ => continue,
            };
            if feature.is_some_and(|wanted| wanted != entry.uid) {
                continue;
            }
            let bodies: Vec<(BodyUid, &K::Shape)> = match body {
                Some(only) => {
                    if self.body_component(only) != Some(entry.component) {
                        continue;
                    }
                    self.body_shape(only)
                        .map(|shape| (only, shape))
                        .into_iter()
                        .collect()
                }
                None => self
                    .bodies_of_components(&[entry.component])
                    .into_iter()
                    .map(|body| (body.uid, body.shape))
                    .collect(),
            };
            for end in ["start", "end"] {
                let cap = FaceName::new(entry.uid, end, None);
                let found = bodies.iter().find_map(|(uid, shape)| {
                    let has = self
                        .kernel
                        .count_faces(shape, &cap)
                        .is_ok_and(|count| count > 0);
                    if !has {
                        return None;
                    }
                    let port = self.kernel.end_port(shape, &cap).ok()?;
                    Some((*uid, port))
                });
                if let Some((body, port)) = found {
                    ports.push(EndPortView {
                        feature: entry.uid,
                        name: entry.name.clone(),
                        kind,
                        end,
                        body,
                        face: cap,
                        port,
                    });
                }
            }
        }
        ports
    }
}

/// What a command that writes a Flex type into a library did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlexTypeWrite {
    pub id: String,
    /// `user` or `project`.
    pub library: &'static str,
    pub version: u32,
    /// The type's file.
    pub path: PathBuf,
}

/// The library's folder of types a command writes in.
fn types_folder(library: &str, project: Option<&Path>) -> Result<PathBuf, ModelError> {
    flex_library::folder(library, project).map_err(invalid)
}

/// The type `id` of the user's or the project's library as its file holds
/// it; a built-in type is refused, as `doing` ("be edited") says.
fn own_type(
    library: &str,
    project: Option<&Path>,
    id: &str,
    doing: &str,
) -> Result<(PathBuf, LibraryFlexType), ModelError> {
    let name = flex_library::library_name(library).map_err(invalid)?;
    if name == flex_library::BUILT_IN {
        return Err(invalid(format!(
            "a built-in flex type cannot {doing}: flex.save_type saves a copy of it \
             to the user's or the project's library"
        )));
    }
    let folder = types_folder(name, project)?;
    let found = flex_library::find(Some(name), id, project).map_err(invalid)?;
    Ok((flex_library::type_path(&folder, &found.id), found))
}

impl<K: Kernel> Document<K> {
    /// The item of the Flex item `uid` as a type of its own: its section
    /// the one it is built with (a sketch profile's region as an
    /// outline), its bend radii the item's own where it has them, and no
    /// origin.
    pub fn flex_item_as_type(&self, uid: FeatureUid) -> Result<FlexType, ModelError> {
        let def = self.flex_def(uid)?;
        let mut item = FlexType {
            origin: None,
            ..def.item.clone()
        };
        let output = self.flex_output(uid);
        if def.profile.is_some() {
            let Some(output) = &output else {
                return Err(invalid(
                    "the item's profile could not be read: the item has not been made",
                ));
            };
            item.section = output.section.clone();
            // An outline is hollow by its own loops.
            item.wall = None;
        }
        if let Some(output) = &output {
            if def.bend_radius.is_some() {
                item.bend_radius = output.bend_radius;
            }
            if def.bend_radius_edge.is_some() && !item.section.is_round() {
                item.bend_radius_edge = output.bend_radius_edge;
            }
        }
        item.check().map_err(invalid)?;
        Ok(item)
    }

    /// Saves `item` as a new type of the user's or the project's library
    /// (`library`; `project` the project's folder), under `id` or the next
    /// free id made from its name. One undo step (`Save Flex Type …`) that
    /// takes the file away again; the design does not change.
    pub fn save_flex_type(
        &mut self,
        library: &str,
        project: Option<&Path>,
        id: Option<&str>,
        item: &FlexType,
    ) -> Result<FlexTypeWrite, ModelError> {
        let library = flex_library::library_name(library).map_err(invalid)?;
        let folder = types_folder(library, project)?;
        item.check().map_err(invalid)?;
        if item.name.trim().is_empty() {
            return Err(invalid("a flex type needs a name"));
        }
        let taken = |id: &str| {
            flex_library::built_in_type(id).is_some()
                || flex_library::type_path(&folder, id).exists()
        };
        let id = match id.map(str::trim) {
            Some(id) => {
                if !flex_library::valid_id(id) {
                    return Err(invalid(
                        "a flex type's id is made of lower-case letters, digits and hyphens",
                    ));
                }
                if taken(id) {
                    return Err(invalid(crate::notice::said(
                        "the library has a flex type of that id already",
                        format!("the {library} library has a flex type {id} already"),
                    )));
                }
                id.to_owned()
            }
            None => free_id(&slug(&item.name), taken),
        };
        let path = flex_library::type_path(&folder, &id);
        let text = flex_library::file_text(item, 1);
        let file = FileWrite::new(path.clone(), Some(text.into_bytes()));
        self.with_files(format!("Save Flex Type {}", item.name), vec![file], |_| {
            Ok(())
        })?;
        Ok(FlexTypeWrite {
            id,
            library,
            version: 1,
            path,
        })
    }

    /// Changes the type `id` of the user's or the project's library by
    /// `change`; its version grows by one when it changed. A built-in type
    /// is refused. One undo step (`Edit Flex Type …`); designs keep their
    /// copies as they are.
    pub fn edit_flex_type(
        &mut self,
        library: &str,
        project: Option<&Path>,
        id: &str,
        change: impl FnOnce(&mut FlexType) -> Result<(), String>,
    ) -> Result<FlexTypeWrite, ModelError> {
        let (path, old) = own_type(library, project, id, "be edited")?;
        let mut item = old.item.clone();
        change(&mut item).map_err(invalid)?;
        item.origin = None;
        item.check().map_err(invalid)?;
        if item.name.trim().is_empty() {
            return Err(invalid("a flex type needs a name"));
        }
        let version = if item == old.item {
            old.version
        } else {
            old.version + 1
        };
        if version != old.version {
            let text = flex_library::file_text(&item, version);
            let file = FileWrite::new(path.clone(), Some(text.into_bytes()));
            self.with_files(format!("Edit Flex Type {}", item.name), vec![file], |_| {
                Ok(())
            })?;
        }
        Ok(FlexTypeWrite {
            id: old.id,
            library: old.library,
            version,
            path,
        })
    }

    /// Deletes the type `id` of the user's or the project's library; a
    /// built-in type is refused. One undo step (`Delete Flex Type …`) that
    /// brings the file back; designs keep their copies.
    pub fn delete_flex_type(
        &mut self,
        library: &str,
        project: Option<&Path>,
        id: &str,
    ) -> Result<FlexTypeWrite, ModelError> {
        let (path, old) = own_type(library, project, id, "be deleted")?;
        let file = FileWrite::new(path.clone(), None);
        self.with_files(
            format!("Delete Flex Type {}", old.item.name),
            vec![file],
            |_| Ok(()),
        )?;
        Ok(FlexTypeWrite {
            id: old.id,
            library: old.library,
            version: old.version,
            path,
        })
    }
}

/// A Flex item as a row of a parts list or a cut list: what it is and how
/// much of it.
#[derive(Debug, Clone, PartialEq)]
pub struct FlexLength {
    /// The Flex item.
    pub feature: FeatureUid,
    /// The item's name: its type's.
    pub item: String,
    /// The id of the type it was copied from, when it has one.
    pub type_id: Option<String>,
    /// The route's length.
    pub length: f64,
    /// Grams, when the item has a mass per length.
    pub mass: Option<f64>,
    /// The name of the parameter that holds the length.
    pub parameter: Option<String>,
}

impl<K: Kernel> Document<K> {
    /// The Flex item `uid` as a parts list's row: its item, its type and
    /// its routed length. None for another feature, or an item that was
    /// not routed.
    pub fn flex_length(&self, uid: FeatureUid) -> Option<FlexLength> {
        let def = self.flex_def(uid).ok()?;
        let length = self.flex_output(uid)?.length;
        Some(FlexLength {
            feature: uid,
            item: def.item.name.clone(),
            type_id: def.item.origin.as_ref().map(|origin| origin.id.clone()),
            length,
            // The length in metres times the grams per metre.
            mass: def.item.mass_per_length.map(|mass| length / 1000.0 * mass),
            parameter: def.length.map(|id| self.state.parameters.name(id)),
        })
    }

    /// [`Self::flex_length`] of the Flex item whose body a parts list's
    /// row is: `body`, or the one body of `component` when the row is the
    /// whole component.
    pub fn flex_length_of_row(
        &self,
        component: ComponentUid,
        body: Option<BodyUid>,
    ) -> Option<FlexLength> {
        let body = match body {
            Some(body) => body,
            None => match self.bodies_of_components(&[component]).as_slice() {
                [only] => only.uid,
                _ => return None,
            },
        };
        self.flex_length(body.feature)
    }
}

impl DocState {
    /// Gives the body of the Flex item `uid` the material and the
    /// appearance its item names (mitcad#1713), when the item is made
    /// (`old` None) and when an edit changes what the item names: a look
    /// set on the body by hand stays through the item's other edits. A
    /// material brings its own appearance unless the item names one. An
    /// id that is neither the design's nor a built-in one is left out:
    /// the body then has its component's.
    pub(crate) fn settle_flex_look(
        &mut self,
        uid: FeatureUid,
        item: &FlexType,
        old: Option<&FlexType>,
    ) {
        let material_changed = old.is_none_or(|old| old.material != item.material);
        let appearance_changed = old.is_none_or(|old| old.appearance != item.appearance);
        if !material_changed && !appearance_changed {
            return;
        }
        let material = item
            .material
            .as_deref()
            .filter(|id| self.material(id).is_some());
        let appearance = item
            .appearance
            .as_deref()
            .filter(|id| self.look(id).is_some())
            .map(str::to_owned);
        if old.is_none() && material.is_none() && appearance.is_none() {
            return;
        }
        let body = BodyUid::new(uid, 0);
        let mut attributes = self.body_attributes.get(&body).cloned().unwrap_or_default();
        if material_changed {
            attributes.material = material.map(str::to_owned);
            attributes.material_expr = None;
        }
        // The item's appearance; without one, a new material's own.
        let of_material = material
            .and_then(|id| self.material(id))
            .and_then(|material| material.appearance.clone());
        if appearance_changed || material_changed {
            let look = appearance.or(of_material);
            if look.is_some() || appearance_changed {
                attributes.appearance = look;
                attributes.appearance_expr = None;
            }
        }
        if attributes.is_default() {
            self.body_attributes.remove(&body);
        } else {
            self.body_attributes.insert(body, attributes);
        }
    }
}

impl DocState {
    /// Takes where a recompute found the attached nodes of the Flex items
    /// into their definitions (`at`, mitcad#1713): a node that follows its
    /// face, edge, point or port is remembered where it last was, so that
    /// it stays there, not where it was once put, when what it is attached
    /// to is gone later. A lost node keeps what it has. (The item is
    /// routed once more at the next recompute, to the same route: its
    /// nodes are at those places.)
    pub(crate) fn take_flex_places<S>(&mut self, result: &crate::recompute::Recomputed<S>) {
        for i in 0..self.features.len() {
            let entry = &self.features[i];
            let FeatureDef::Flex(def) = &entry.def else {
                continue;
            };
            let found = result
                .results
                .get(i)
                .filter(|found| found.uid == entry.uid)
                .and_then(|found| found.output.as_deref())
                .and_then(|output| output.flex.as_deref());
            let Some(found) = found else {
                continue;
            };
            let mut nodes = def.nodes.clone();
            let mut moved = false;
            for node in &mut nodes {
                if node.on.is_none() && node.port.is_none() {
                    continue;
                }
                let now = found.nodes.iter().find(|now| now.id == node.id);
                if let Some(now) = now.filter(|now| !now.lost)
                    && now.point.iter().all(|v| v.is_finite())
                    && mitcad_vec3::distance(now.point, node.at) > 1e-6
                {
                    node.at = now.point;
                    moved = true;
                }
            }
            if moved {
                let mut entry = (**entry).clone();
                if let FeatureDef::Flex(def) = &mut entry.def {
                    def.nodes = nodes;
                }
                self.features[i] = Arc::new(entry);
            }
        }
    }
}

/// Where end ports are asked for (mitcad#1713): the component whose item
/// will use them, and the occurrence the ports' body was picked in.
#[derive(Debug, Clone, Copy)]
pub struct PortsSeen<'a> {
    /// The component of the item that begins or ends at a port.
    pub component: ComponentUid,
    /// The occurrence path (from the root) the ports' component was
    /// picked at.
    pub occurrence: &'a [OccurrenceUid],
    /// The path `component` is seen at; its first when None.
    pub context: Option<&'a [OccurrenceUid]>,
}

impl<K: Kernel> Document<K> {
    /// [`Self::end_ports`] of `feature` and of `body` only (each when
    /// given: a body under the pointer asks for its own ports, and only
    /// its faces are looked at), and with `seen` the ports of the
    /// component picked at that occurrence, moved into the coordinates of
    /// the component that will use them.
    pub fn end_ports_seen(
        &self,
        feature: Option<FeatureUid>,
        body: Option<BodyUid>,
        seen: Option<PortsSeen<'_>>,
    ) -> Result<Vec<EndPortView>, ModelError> {
        let assembly = &self.state.assembly;
        let mut ports = self.end_ports_in(feature, body);
        let Some(seen) = seen else {
            return Ok(ports);
        };
        let source = crate::joints::path_component(assembly, ComponentUid::ROOT, seen.occurrence)
            .map_err(invalid)?;
        ports.retain(|port| {
            self.state
                .entry(port.feature)
                .is_some_and(|entry| entry.component == source)
        });
        let link = crate::links::link_for(assembly, seen.component, seen.occurrence, seen.context)
            .map_err(invalid)?;
        if let Some(link) = link {
            let transform = link
                .transform(assembly, seen.component, &mut |o| {
                    self.placement(o).unwrap_or(Transform::IDENTITY)
                })
                .map_err(invalid)?;
            for found in &mut ports {
                let port = &mut found.port;
                port.position = transform.apply_point(port.position);
                port.direction = transform.apply_vector(port.direction);
                port.x_axis = transform.apply_vector(port.x_axis);
            }
        }
        Ok(ports)
    }
}
