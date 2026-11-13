// SPDX-License-Identifier: MIT
//! Materials kept in the document (mitcad#640, part 2), next to the
//! built-in ones (`crate::material`), and what a body is made of.
//!
//! A body names its material by id (`set_body_material`); one without
//! uses its component's default material (`set_component_material`): the
//! nearest component up the occurrence path that places it, then the
//! root's, then the built-in default, steel. Without a path (a body's mass
//! in the `properties` query) only the body's own component and the root
//! count. A component's drawing puts the component's first path from the
//! root before the body's path from the component (`body_material_under`,
//! mitcad#758).
//!
//! Setting a body's material also sets its appearance to the material's
//! (`appearance: "replace"`), keeps the body's (`"keep"`), or, when left
//! out, replaces it unless the body's appearance was set by hand: one that
//! is not its current material's. The commands are undo steps that
//! recompute nothing; the design file keeps the document's materials and
//! the components' defaults.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::{Value, json};

use super::print_settings::hex_color;
use super::{DocState, Document, ModelError, invalid};
use crate::appearance::{Appearance, library_appearance};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::material::{self, DEFAULT_MATERIAL, Material, MaterialChange};
use crate::print_library::{BasedOn, ItemData, ItemKind};
use crate::print_profile::{copy_name, new_item_name};

/// The material an entry of the bundled material dataset gives a new one
/// (mitcad#1213): refused with `based_on`, or for an entry it has not.
pub(super) fn dataset_material(
    change: &MaterialChange,
    entry: &str,
) -> Result<Material, ModelError> {
    if change.based_on.is_some() {
        return Err(invalid(
            "a new material starts from another material (based_on) or from the dataset, not both",
        ));
    }
    crate::material_dataset::material(entry)
        .ok_or_else(|| invalid(format!("the material dataset has no material {entry}")))
}

/// What `set_body_material` does with the body's appearance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppearanceChoice {
    /// The material's appearance replaces the body's.
    Replace,
    /// The body keeps its appearance.
    Keep,
    /// Replace unless the body's appearance was set by hand.
    #[default]
    Auto,
}

impl AppearanceChoice {
    pub fn parse(text: &str) -> Result<Self, ModelError> {
        match text {
            "replace" => Ok(Self::Replace),
            "keep" => Ok(Self::Keep),
            _ => Err(invalid(format!(
                "appearance must be \"replace\" or \"keep\", got \"{text}\""
            ))),
        }
    }
}

/// Where a body's material comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialSource {
    /// Its own.
    Body,
    /// A component's default: the body's own component's or one up the
    /// occurrence path (the root's last).
    Component(ComponentUid),
    /// None is set anywhere: the built-in default.
    Default,
}

/// What deleting a material changed: the bodies and the components whose
/// default it was, which use another one now (the built-in of the same id
/// when the deleted one took its place, else none of their own).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletedMaterial {
    pub bodies: Vec<BodyUid>,
    pub components: Vec<ComponentUid>,
}

impl DocState {
    /// A material of the document or the built-in one by id; the
    /// document's takes the place of a built-in of the same id.
    pub(crate) fn material(&self, id: &str) -> Option<&Material> {
        self.materials
            .iter()
            .find(|m| m.id == id)
            .or_else(|| material::built_in_material(id))
    }

    /// An appearance of the library or of the design by id.
    pub(crate) fn look(&self, id: &str) -> Option<&Appearance> {
        library_appearance(id).or_else(|| self.appearances.iter().find(|a| a.id == id))
    }

    /// A material's own filament colour (a design or library file from
    /// before mitcad#1055, an import, a command that sets it) as an
    /// appearance of its list: one of its list of that colour, else a new
    /// plastic appearance of the design (`<name> color`). It is the default
    /// when `make_default` (a command that sets the colour and names no
    /// appearance: the colour is what the user wants printed) or when the
    /// material has none; else it joins the list, so that a file from
    /// before keeps the look it showed. The key goes: the colour is the
    /// appearance's.
    pub(crate) fn adopt_filament_colour(&mut self, material: &mut Material, make_default: bool) {
        let Some(value) = material.filament.remove("filament_colour") else {
            return;
        };
        let Some(color) = value.as_str().and_then(crate::print_files::hex_rgb) else {
            return;
        };
        let hex = hex_color(color);
        let found = material.appearance_list().into_iter().find(|id| {
            self.look(id)
                .is_some_and(|a| hex_color(a.display_color()) == hex)
        });
        let id = match found {
            Some(id) => id,
            None => {
                let look =
                    crate::print_files::plastic_appearance(&material.id, &material.name, color);
                self.add_item_appearance(&look)
            }
        };
        material.add_appearance(&id);
        if make_default {
            material.set_default_appearance(&id);
        }
    }

    /// The appearances of a material's list that exist here, the others
    /// left out (a library's item names an appearance of the design it
    /// was saved from, mitcad#1055).
    pub(crate) fn known_appearances_only(&self, material: &mut Material) {
        let known: Vec<String> = material
            .appearances
            .iter()
            .filter(|a| self.look(a).is_some())
            .cloned()
            .collect();
        material.appearances = known;
    }

    /// Copies the document materials `needed` (by id) from another
    /// document's state: an id the target has with other fields gets a new
    /// one (`id_1`, ...), a name taken another; returns the source's ids
    /// with the copies' where they differ.
    pub fn copy_materials(
        &mut self,
        source: &DocState,
        needed: &BTreeSet<String>,
    ) -> BTreeMap<String, String> {
        let mut mapped = BTreeMap::new();
        for id in needed {
            let Some(theirs) = source.materials.iter().find(|m| &m.id == id) else {
                continue;
            };
            if self.materials.iter().any(|m| m == theirs) {
                continue;
            }
            let mut copy = theirs.clone();
            if self.materials.iter().any(|m| m.id == copy.id) {
                copy.id = (1..)
                    .map(|n| format!("{id}_{n}"))
                    .find(|candidate| self.materials.iter().all(|m| &m.id != candidate))
                    .expect("a free id");
                mapped.insert(id.clone(), copy.id.clone());
            }
            if self.material_name_taken(&copy.name, &copy.id, None) {
                let base = copy.name.clone();
                copy.name = (1..)
                    .map(|n| format!("{base} ({n})"))
                    .find(|name| !self.material_name_taken(name, &copy.id, None))
                    .expect("a free name");
            }
            self.materials.push(copy);
        }
        mapped
    }

    /// Whether another material shows the name: a document material (but
    /// the one at `own`), or a built-in that `id` does not replace.
    pub(crate) fn material_name_taken(&self, name: &str, id: &str, own: Option<usize>) -> bool {
        let overridden: Vec<&str> = self.materials.iter().map(|m| m.id.as_str()).collect();
        material::built_in()
            .iter()
            .filter(|m| m.id != id && !overridden.contains(&m.id.as_str()))
            .any(|m| m.name == name)
            || self
                .materials
                .iter()
                .enumerate()
                .any(|(j, m)| Some(j) != own && m.id != id && m.name == name)
    }
}

impl<K: Kernel> Document<K> {
    /// The document's own materials, in the order they were made.
    pub fn custom_materials(&self) -> &[Material] {
        &self.state.materials
    }

    /// A material of the document or a built-in one by id.
    pub fn material(&self, id: &str) -> Option<&Material> {
        self.state.material(id)
    }

    /// A component's default material id (C0: the root's).
    pub fn component_material(&self, component: ComponentUid) -> Option<&str> {
        self.state
            .component_materials
            .get(&component)
            .map(String::as_str)
    }

    /// The material a body is made of, and where it comes from: its own,
    /// else the nearest component's default up `path` (the occurrences from
    /// the root to the body's component), else the root's, else steel.
    /// Without a path, the body's own component and the root.
    pub fn body_material(
        &self,
        uid: BodyUid,
        path: Option<&[OccurrenceUid]>,
    ) -> (&Material, MaterialSource) {
        if let Some(found) = self.own_material(uid) {
            return (found, MaterialSource::Body);
        }
        self.inherited_material(uid, path)
    }

    /// The material a body's components give it, its own left out: the
    /// nearest component's default up `path` (as [`Self::body_material`]
    /// takes it), else the root's, else steel; what it is made of once its
    /// own material is cleared.
    pub fn inherited_material(
        &self,
        uid: BodyUid,
        path: Option<&[OccurrenceUid]>,
    ) -> (&Material, MaterialSource) {
        let chain: Vec<ComponentUid> = match path {
            Some(path) => self.path_components_up(path),
            None => self
                .body_component(uid)
                .or_else(|| self.feature_component(uid.feature))
                .into_iter()
                .collect(),
        };
        self.material_up(chain)
    }

    /// The material a body is made of as a drawing of `component` shows
    /// it, `path` leading from `component` to the body's component (empty
    /// for its own bodies): its own, else the nearest component's default
    /// up `path`, then `component`'s and the components' above it where the
    /// application shows it (its first path from the root), then the
    /// root's, else steel. A component placed nowhere has only its own
    /// default above `path`.
    pub fn body_material_under(
        &self,
        uid: BodyUid,
        component: ComponentUid,
        path: &[OccurrenceUid],
    ) -> (&Material, MaterialSource) {
        if component.is_root() {
            return self.body_material(uid, (!path.is_empty()).then_some(path));
        }
        match crate::links::first_path(&self.state.assembly, component, None) {
            Ok(mut full) => {
                full.extend_from_slice(path);
                self.body_material(uid, Some(&full))
            }
            Err(_) => {
                if let Some(found) = self.own_material(uid) {
                    return (found, MaterialSource::Body);
                }
                let mut chain = self.path_components_up(path);
                chain.push(component);
                self.material_up(chain)
            }
        }
    }

    /// The material a body names as its own, when it exists.
    fn own_material(&self, uid: BodyUid) -> Option<&Material> {
        self.state
            .body_attributes
            .get(&uid)
            .and_then(|a| a.material.as_deref())
            .and_then(|id| self.material(id))
    }

    /// The components the occurrences of `path` place, nearest the body
    /// first.
    fn path_components_up(&self, path: &[OccurrenceUid]) -> Vec<ComponentUid> {
        path.iter()
            .rev()
            .filter_map(|o| self.state.assembly.occurrence(*o).map(|o| o.component))
            .collect()
    }

    /// The first default material along `chain`, then the root's, else
    /// steel (for a body without a material of its own).
    fn material_up(&self, mut chain: Vec<ComponentUid>) -> (&Material, MaterialSource) {
        chain.push(ComponentUid::ROOT);
        for component in chain {
            if let Some(found) = self
                .component_material(component)
                .and_then(|id| self.material(id))
            {
                return (found, MaterialSource::Component(component));
            }
        }
        (
            self.material(DEFAULT_MATERIAL)
                .expect("the default material is built in"),
            MaterialSource::Default,
        )
    }

    /// The component of a body's creating feature (for bodies not at the
    /// marker).
    fn feature_component(&self, feature: FeatureUid) -> Option<ComponentUid> {
        self.state.entry(feature).map(|f| f.component)
    }

    /// Whether a body's appearance was set by hand: it has one, and it is
    /// none of the appearances the material it is made of offers
    /// (mitcad#1055: its default or one of its list).
    pub fn appearance_set_by_hand(&self, uid: BodyUid) -> bool {
        let Some(own) = self
            .state
            .body_attributes
            .get(&uid)
            .and_then(|a| a.appearance.as_deref())
        else {
            return false;
        };
        !self.body_material(uid, None).0.offers(own)
    }

    /// The appearance a body prints with (mitcad#1055), its filament's
    /// colour: its own; else, when it is made of a component's default
    /// material, that component's appearance; else its material's
    /// default. `path` as `body_material`'s.
    pub fn body_look(&self, uid: BodyUid, path: Option<&[OccurrenceUid]>) -> Option<&str> {
        if let Some(own) = self
            .state
            .body_attributes
            .get(&uid)
            .and_then(|a| a.appearance.as_deref())
        {
            return Some(own);
        }
        let (material, source) = self.body_material(uid, path);
        if let MaterialSource::Component(c) = source
            && let Some(look) = self.state.component_appearances.get(&c)
        {
            return Some(look.as_str());
        }
        material.appearance.as_deref()
    }

    /// A component's default appearance (C0: the root's, mitcad#1055): the
    /// one of its default material that its bodies without one of their
    /// own get; none: the material's default.
    pub fn component_appearance(&self, component: ComponentUid) -> Option<&str> {
        self.state
            .component_appearances
            .get(&component)
            .map(String::as_str)
    }

    /// Sets a body's material by id (None: its component's default), and
    /// its appearance as `choice` says: one undo step, which recomputes
    /// nothing, unless nothing changes.
    pub fn set_body_material_with(
        &mut self,
        uid: BodyUid,
        material: Option<&str>,
        choice: AppearanceChoice,
    ) -> Result<(), ModelError> {
        self.set_body_material_look(uid, material, choice, None)
    }

    /// [`Document::set_body_material_with`] with one of the material's
    /// appearances for the body (`look`, mitcad#1055: Material ▸ PLA ▸
    /// Black), `choice` aside. An appearance the design's material does
    /// not offer yet joins its list in the same undo step.
    pub fn set_body_material_look(
        &mut self,
        uid: BodyUid,
        material: Option<&str>,
        choice: AppearanceChoice,
        look: Option<&str>,
    ) -> Result<(), ModelError> {
        if let Some(id) = material
            && self.material(id).is_none()
        {
            let mut known: Vec<&str> = material::built_in().iter().map(|m| m.id.as_str()).collect();
            for m in &self.state.materials {
                if !known.contains(&m.id.as_str()) {
                    known.push(&m.id);
                }
            }
            return Err(invalid(format!(
                "unknown material {id} (known: {})",
                known.join(", ")
            )));
        }
        if self.body_shape(uid).is_none() {
            return Err(invalid(format!(
                "body {uid} does not exist at the timeline marker"
            )));
        }
        let look = look.map(str::trim).filter(|l| !l.is_empty());
        if let Some(look) = look
            && self.appearance(look).is_none()
        {
            return Err(invalid(format!("there is no appearance {look}")));
        }
        let replace = match choice {
            AppearanceChoice::Replace => true,
            AppearanceChoice::Keep => false,
            AppearanceChoice::Auto => !self.appearance_set_by_hand(uid),
        };
        let mut attributes = self.body_attributes(uid);
        attributes.material = material.map(str::to_owned);
        // A material set by hand ends the body's binding (mitcad#800).
        attributes.material_expr = None;
        let made_of = match &attributes.material {
            Some(id) => self.material(id),
            None => Some(self.inherited_material(uid, None).0),
        };
        if let Some(look) = look {
            attributes.appearance = Some(look.to_owned());
            attributes.appearance_expr = None;
        } else if replace {
            // The appearance of the material it is made of now (its own,
            // else its components'): the body's when the material offers
            // it (mitcad#1055), else the material's default.
            let own = attributes.appearance.clone();
            attributes.appearance = match (made_of, own) {
                (Some(m), Some(own)) if m.offers(&own) => Some(own),
                (Some(m), _) => m.appearance.clone(),
                (None, _) => None,
            };
        }
        // The design's material offers the appearance from now on.
        let listed = match (look, &attributes.material) {
            (Some(look), Some(id)) => self.listed_with(id, look),
            _ => None,
        };
        if attributes == self.body_attributes(uid) && listed.is_none() {
            return Ok(());
        }
        let label = format!("Set Material of {}", self.body_name(uid));
        self.apply_with(|state| {
            if attributes.is_default() {
                state.body_attributes.remove(&uid);
            } else {
                state.body_attributes.insert(uid, attributes);
            }
            if let Some((i, m)) = listed {
                state.materials[i] = m;
            }
            Ok((label, (), false))
        })
    }

    /// The design's material `id` with `look` in its list, when it is the
    /// design's own and does not offer it yet (mitcad#1055): its index
    /// and the material with it. A built-in material cannot change.
    fn listed_with(&self, id: &str, look: &str) -> Option<(usize, Material)> {
        let i = self.state.materials.iter().position(|m| m.id == id)?;
        let mut m = self.state.materials[i].clone();
        if m.offers(look) {
            return None;
        }
        m.add_appearance(look);
        Some((i, m))
    }

    /// Sets a body's appearance as the Appearance command does
    /// (mitcad#1055): as `set_body_appearance`, and an appearance outside
    /// the list of the design's material the body is made of joins that
    /// list, in the same undo step. A built-in material cannot change: its
    /// list stays.
    pub fn set_body_appearance_listed(
        &mut self,
        uid: BodyUid,
        appearance: Option<&str>,
    ) -> Result<(), ModelError> {
        let made_of = self.body_material(uid, None).0.id.clone();
        let listed = appearance
            .map(str::trim)
            .filter(|a| !a.is_empty() && self.appearance(a).is_some())
            .and_then(|a| self.listed_with(&made_of, a));
        let Some((i, m)) = listed else {
            return self.set_body_appearance(uid, appearance);
        };
        // The body's step last: the undo step is named after it.
        self.as_one_step(|doc| {
            let label = format!("Edit Material {}", m.name);
            doc.apply_with(|state| {
                state.materials[i] = m;
                Ok((label, (), false))
            })?;
            doc.set_body_appearance(uid, appearance)
        })
    }

    /// The bodies of `bodies` whose appearance was set by hand and would
    /// change if they got `material` (None: their component's default)
    /// with its appearance: those the application asks about before
    /// replacing.
    pub fn appearance_conflicts(
        &self,
        bodies: &[BodyUid],
        material: Option<&str>,
    ) -> Result<Vec<BodyUid>, ModelError> {
        let new = match material {
            Some(id) => Some(
                self.material(id)
                    .ok_or_else(|| invalid(format!("unknown material {id}")))?,
            ),
            None => None,
        };
        Ok(bodies
            .iter()
            .copied()
            .filter(|uid| {
                let own = self
                    .state
                    .body_attributes
                    .get(uid)
                    .and_then(|a| a.appearance.as_deref());
                let target = match new {
                    Some(m) => m,
                    None => self.inherited_material(*uid, None).0,
                };
                // One the new material offers stays (mitcad#1055).
                self.appearance_set_by_hand(*uid) && !own.is_some_and(|own| target.offers(own))
            })
            .collect())
    }

    /// Sets a component's default material (C0: the root's; None takes it
    /// away): one undo step, which recomputes nothing, unless nothing
    /// changes.
    pub fn set_component_material(
        &mut self,
        component: ComponentUid,
        material: Option<&str>,
    ) -> Result<(), ModelError> {
        self.set_component_material_look(component, material, None)
    }

    /// [`Document::set_component_material`] with one of the material's
    /// appearances for its bodies without one of their own (`look`,
    /// mitcad#1055; none: the material's default). An appearance the
    /// design's material does not offer yet joins its list in the same
    /// undo step.
    pub fn set_component_material_look(
        &mut self,
        component: ComponentUid,
        material: Option<&str>,
        look: Option<&str>,
    ) -> Result<(), ModelError> {
        if !component.is_root() && self.state.assembly.component(component).is_none() {
            return Err(invalid(format!("there is no component {component}")));
        }
        if let Some(id) = material
            && self.material(id).is_none()
        {
            return Err(invalid(format!("unknown material {id}")));
        }
        let look = look.map(str::trim).filter(|l| !l.is_empty());
        if let Some(look) = look {
            if material.is_none() {
                return Err(invalid("an appearance is given without a material"));
            }
            if self.appearance(look).is_none() {
                return Err(invalid(format!("there is no appearance {look}")));
            }
        }
        let listed = match (material, look) {
            (Some(id), Some(look)) => self.listed_with(id, look),
            _ => None,
        };
        if self.component_material(component) == material
            && self.component_appearance(component) == look
            && listed.is_none()
        {
            return Ok(());
        }
        let label = format!(
            "Set Default Material of {}",
            self.state.assembly.name(component)
        );
        let material = material.map(str::to_owned);
        let look = look.map(str::to_owned);
        self.apply_with(|state| {
            match material {
                Some(id) => {
                    state.component_materials.insert(component, id);
                }
                None => {
                    state.component_materials.remove(&component);
                }
            }
            match look {
                Some(look) => {
                    state.component_appearances.insert(component, look);
                }
                None => {
                    state.component_appearances.remove(&component);
                }
            }
            if let Some((i, m)) = listed {
                state.materials[i] = m;
            }
            Ok((label, (), false))
        })
    }

    /// The `materials` query: the built-in materials the document does not
    /// replace, then the document's, each with its fields, `built_in`,
    /// `replaces_built_in` (a document material with a built-in's id), its
    /// `class` (stored or worked out, mitcad#655), the bodies that name it
    /// and the components whose default it is.
    pub fn materials_json(&self) -> Value {
        let replaced: Vec<&str> = self.state.materials.iter().map(|m| m.id.as_str()).collect();
        let built_in = material::built_in()
            .iter()
            .filter(|m| !replaced.contains(&m.id.as_str()))
            .map(|m| (m, true));
        let own = self.state.materials.iter().map(|m| (m, false));
        Value::Array(
            built_in
                .chain(own)
                .map(|(m, built_in)| {
                    let mut value = serde_json::to_value(m).expect("materials serialize");
                    value["built_in"] = json!(built_in);
                    value["class"] = json!(material::material_class(m).id());
                    // Print settings make it an FDM plastic (mitcad#1102).
                    value["class_fixed"] = json!(m.has_print_settings());
                    if !built_in && material::built_in_material(&m.id).is_some() {
                        value["replaces_built_in"] = json!(true);
                    }
                    if let Some(color) = m
                        .appearance
                        .as_deref()
                        .and_then(|a| self.appearance(a))
                        .map(|a| a.display_color())
                    {
                        value["display_color"] = json!(color);
                    }
                    // Every appearance it offers, its default first
                    // (mitcad#1055).
                    value["appearances"] = json!(m.appearance_list());
                    value["bodies"] = json!(self.bodies_with_material(&m.id));
                    let components = self.components_with_material(&m.id);
                    // The appearances its components chose (mitcad#1055).
                    let looks: serde_json::Map<String, Value> = components
                        .iter()
                        .filter_map(|c| {
                            self.component_appearance(*c)
                                .map(|a| (c.to_string(), json!(a)))
                        })
                        .collect();
                    if !looks.is_empty() {
                        value["component_appearances"] = Value::Object(looks);
                    }
                    value["components"] = json!(components);
                    value
                })
                .collect(),
        )
    }

    /// The `body_material` query: the material a body is made of (in the
    /// occurrence `path` when given), its fields and where it comes from.
    pub fn body_material_json(&self, uid: BodyUid, path: Option<&[OccurrenceUid]>) -> Value {
        let (m, source) = self.body_material(uid, path);
        let mut value = serde_json::to_value(m).expect("materials serialize");
        match source {
            MaterialSource::Body => value["source"] = json!("body"),
            MaterialSource::Component(c) => {
                value["source"] = json!("component");
                value["component"] = json!(c);
            }
            MaterialSource::Default => {
                value["source"] = json!("default");
                // What a slicer gets for it meanwhile (mitcad#1727): the
                // print default, PLA, not the design's default.
                if let Some(print) = self.material(crate::print_analysis::DEFAULT_MATERIAL) {
                    value["print_material"] = json!({"id": print.id, "name": print.name});
                }
            }
        }
        value["appearance_set_by_hand"] = json!(self.appearance_set_by_hand(uid));
        // The variant the body's printer picks (mitcad#1054).
        if !m.variants.is_empty() {
            let effective =
                self.effective_print(&super::PrintTarget::Body(uid, path.map(<[_]>::to_vec)));
            value["variant"] = effective
                .variant
                .filter(|_| effective.material == m.id)
                .and_then(|(i, how)| {
                    m.variants
                        .get(i)
                        .map(|v| super::print_settings::variant_json(v, how))
                })
                .unwrap_or(Value::Null);
        }
        value
    }

    pub(super) fn bodies_with_material(&self, id: &str) -> Vec<BodyUid> {
        self.state
            .body_attributes
            .iter()
            .filter(|(_, a)| a.material.as_deref() == Some(id))
            .map(|(uid, _)| *uid)
            .collect()
    }

    pub(super) fn components_with_material(&self, id: &str) -> Vec<ComponentUid> {
        self.state
            .component_materials
            .iter()
            .filter(|(_, m)| m.as_str() == id)
            .map(|(c, _)| *c)
            .collect()
    }

    /// Makes a material of the document from `based_on`'s fields (a
    /// material of the document or built in, or a library's item with the
    /// appearance it carries, mitcad#655; none: `Material 1` of 1 g/cm³)
    /// and the given ones. Without an id it is `custom1`, `custom2`,
    /// ... A built-in's id makes the document's take its place. Returns
    /// the id.
    pub fn create_material(&mut self, change: &MaterialChange) -> Result<String, ModelError> {
        if change.library.is_some() {
            return Err(invalid(
                "a material of a library is made there (create_library_material)",
            ));
        }
        // An entry of the bundled material dataset (mitcad#1213).
        if let Some(entry) = &change.dataset {
            return self.create_material_from(Some(dataset_material(change, entry)?), change);
        }
        let base = match change.based_on.as_ref() {
            Some(BasedOn::Design(id)) => {
                let mut base = self
                    .material(id)
                    .cloned()
                    .ok_or_else(|| invalid(format!("there is no material {id}")))?;
                base.based_on = Some(id.to_owned());
                base
            }
            Some(BasedOn::Library(item)) => {
                // A library's item duplicated into the design: with the
                // appearances it carries (mitcad#1067), in the same undo
                // step.
                let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
                let found = super::library_items::library_item(
                    &libraries,
                    super::library_kind(&item.library)?,
                    ItemKind::Material,
                    &item.id,
                )?;
                let ItemData::Material {
                    material,
                    appearances,
                } = found.data.clone()
                else {
                    unreachable!("a material's folder")
                };
                let mut base = material;
                base.based_on = Some(item.id.clone());
                return self.as_one_step(|doc| {
                    for look in &appearances {
                        let label = format!("Add Appearance {}", look.name);
                        let id = doc.apply_with(|state| {
                            Ok((label, state.add_item_appearance(look), false))
                        })?;
                        base.rename_appearance(&look.id, &id);
                    }
                    doc.create_material_from(Some(base), change)
                });
            }
            None => {
                return self.create_material_from(None, change);
            }
        };
        self.create_material_from(Some(base), change)
    }

    /// [`Document::create_material`] from a material whose fields it
    /// starts from (its `based_on` set), or none.
    fn create_material_from(
        &mut self,
        base: Option<Material>,
        change: &MaterialChange,
    ) -> Result<String, ModelError> {
        // The libraries' names count too (mitcad#655): the lists never show
        // two materials alike.
        let libraries = self.print_libraries(change.project.as_deref().map(Path::new));
        let library_names: Vec<String> = libraries
            .items
            .iter()
            .filter(|i| i.data.kind() == ItemKind::Material && !i.shadowed)
            .map(|i| i.data.name().to_lowercase())
            .collect();
        let taken = |name: &str| {
            self.state.material_name_taken(name, "", None)
                || library_names.contains(&name.to_lowercase())
        };
        let free = (1..)
            .find(|n| {
                self.material(&format!("custom{n}")).is_none()
                    && !taken(&new_item_name("Material", *n))
            })
            .expect("a free number");
        // A copy keeps its source's class (mitcad#655).
        let source_class = base.as_ref().map(material::material_class);
        let mut new = match base {
            Some(mut base) => {
                base.origin = None;
                base.version = None;
                base
            }
            None => Material {
                id: String::new(),
                name: String::new(),
                density: 1.0,
                appearance: None,
                filament: BTreeMap::new(),
                slicer: BTreeMap::new(),
                variants: Vec::new(),
                appearances: Vec::new(),
                based_on: None,
                class: None,
                version: None,
                origin: None,
                properties: Default::default(),
                reference: None,
            },
        };
        new.id = match change.id.as_deref().map(str::trim) {
            Some(id) => id.to_owned(),
            None => format!("custom{free}"),
        };
        // A copy is named after its source, `<name> (copy)` (mitcad#655:
        // the user's own copy of a built-in or library material;
        // mitcad#828), a new one `Material 1`; a dataset's material as its
        // entry (mitcad#1213).
        new.name = match source_class {
            Some(_) if change.dataset.is_some() => material::numbered_name(&new.name, taken),
            Some(_) => copy_name(&new.name, taken),
            None => new_item_name("Material", free),
        };
        // A library's item may name appearances of another design
        // (mitcad#1055): those it offers here.
        self.state.known_appearances_only(&mut new);
        change.apply_to(&mut new);
        self.check_listed(change)?;
        // The class is stored (mitcad#655): the one chosen, else the source's
        // of a copy, else the one its fields give. Print settings make it an
        // FDM plastic whatever it stores (mitcad#1102).
        let class = change
            .class
            .or(source_class)
            .unwrap_or_else(|| material::material_class(&new));
        // A filament keeps `plastic`, which older versions read (mitcad#1102).
        new.class = Some(material::stored_class(&new, class));
        // A new Wood material looks like wood (mitcad#1552).
        if source_class.is_none() {
            material::give_class_appearance(&mut new, class, change);
        }
        if self.state.materials.iter().any(|m| m.id == new.id) {
            return Err(invalid(format!("a material {} exists", new.id)));
        }
        self.check_material(&new, None)?;
        let label = format!("Create Material {}", new.name);
        let id = new.id.clone();
        self.apply_with(|state| {
            // Its own filament colour: an appearance of its list
            // (mitcad#1055).
            // The default unless the command names an appearance.
            state.adopt_filament_colour(&mut new, change.appearance.is_none());
            state.materials.push(new);
            Ok((label, (), false))
        })?;
        Ok(id)
    }

    /// Changes the given fields of a document material; a built-in one
    /// cannot change (`create_material` with `based_on`, or with its id,
    /// makes one the design can change). A change that changes nothing is
    /// no undo step.
    pub fn edit_material(&mut self, id: &str, change: &MaterialChange) -> Result<(), ModelError> {
        if change.based_on.is_some() {
            return Err(invalid("based_on is only for create_material"));
        }
        if change.dataset.is_some() {
            return Err(invalid("dataset is only for create_material"));
        }
        if change.id.as_deref().is_some_and(|new| new != id) {
            return Err(invalid("a material's id cannot change"));
        }
        let i = self.custom_material_index(id)?;
        let mut edited = self.state.materials[i].clone();
        change.apply_to(&mut edited);
        self.check_listed(change)?;
        if edited == self.state.materials[i] {
            return Ok(());
        }
        self.check_material(&edited, Some(i))?;
        let label = format!("Edit Material {}", self.state.materials[i].name);
        self.apply_with(|state| {
            state.adopt_filament_colour(&mut edited, change.appearance.is_none());
            state.materials[i] = edited;
            Ok((label, (), false))
        })
    }

    /// Deletes a document material: the bodies and components that name it
    /// use the built-in of the same id when the deleted one took its place,
    /// else they have none of their own. Returns them.
    pub fn delete_material(&mut self, id: &str) -> Result<DeletedMaterial, ModelError> {
        let i = self.custom_material_index(id)?;
        let bodies = self.bodies_with_material(id);
        let components = self.components_with_material(id);
        let keep = material::built_in_material(id).is_some();
        let label = format!("Delete Material {}", self.state.materials[i].name);
        self.apply_with(|state| {
            state.materials.remove(i);
            if !keep {
                for attributes in state.body_attributes.values_mut() {
                    if attributes.material.as_deref() == Some(id) {
                        attributes.material = None;
                    }
                }
                state.body_attributes.retain(|_, a| !a.is_default());
                let gone: Vec<ComponentUid> = state
                    .component_materials
                    .iter()
                    .filter(|(_, m)| m.as_str() == id)
                    .map(|(c, _)| *c)
                    .collect();
                for c in gone {
                    state.component_appearances.remove(&c);
                }
                state.component_materials.retain(|_, m| m != id);
                // Its purge volumes set by hand (mitcad#758).
                state.drop_purge_overrides_of(id);
            }
            Ok((label, (), false))
        })?;
        Ok(DeletedMaterial { bodies, components })
    }

    /// The fields are valid, the appearance exists and the name is not
    /// another material's (`own`: the index of the one being edited).
    /// The appearances a change lists exist (mitcad#1055).
    pub(super) fn check_listed(&self, change: &MaterialChange) -> Result<(), ModelError> {
        for id in change.appearances.iter().flatten() {
            if self.appearance(id.trim()).is_none() {
                return Err(invalid(format!("there is no appearance {}", id.trim())));
            }
        }
        Ok(())
    }

    pub(super) fn check_material(
        &self,
        m: &Material,
        own: Option<usize>,
    ) -> Result<(), ModelError> {
        m.check().map_err(invalid)?;
        if let Some(a) = &m.appearance
            && self.appearance(a).is_none()
        {
            return Err(invalid(format!(
                "material {}: there is no appearance {a}",
                m.id
            )));
        }
        // A material keeps the name it has (mitcad#1706): a design's own
        // `Wood - Oak`, made from the dataset before the built-in one of
        // that name came, still changes.
        let kept = own.is_some_and(|i| self.state.materials[i].name == m.name);
        if !kept && self.state.material_name_taken(&m.name, &m.id, own) {
            return Err(invalid(format!("a material is named {}", m.name)));
        }
        Ok(())
    }

    pub(super) fn custom_material_index(&self, id: &str) -> Result<usize, ModelError> {
        self.state
            .materials
            .iter()
            .position(|m| m.id == id)
            .ok_or_else(|| {
                invalid(if material::built_in_material(id).is_some() {
                    format!(
                        "{id} is a built-in material, which cannot change: \
                         create_material with based_on copies it"
                    )
                } else {
                    format!("there is no material {id}")
                })
            })
    }
}
