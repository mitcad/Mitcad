// SPDX-License-Identifier: MIT
//! Panelize's panels in the document (mitcad#2005, `features/panelize.rs`):
//! what the feature itself cannot do, since it only makes shapes.
//!
//! - **Materials.** A panel takes the body's material and appearance when
//!   the Panelize is added or edited (each panel that has none of its own
//!   yet); with Result Components also the default material of the body's
//!   component, which the panel's own component would not give it.
//! - **Components.** With Result Components each panel gets a
//!   `component_from_bodies` feature right after the Panelize, which moves
//!   it into a component of its own; an edit adds those of new faces and
//!   removes those of faces gone, or all of them with Result Bodies
//!   (refused while such a component holds features, occurrences or
//!   drawings of its own). All in the command's one undo step.
//! - **Names.** After the recompute, a panel without a name is named by its
//!   face's outward normal ([`panel_name`]: Left, Right, Front, Back,
//!   Bottom, Top, else `Panel <n>`), unique among its component's bodies,
//!   and the component made for it takes the same name.

use std::collections::BTreeSet;

use super::bodies::BodyAttributes;
use super::{DocState, Document, ModelError, invalid};
use crate::features::panelize::{panel_body, panel_name};
use crate::features::{ComponentFromBodiesDef, FeatureDef, PanelResult, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;

/// The `component_from_bodies` features that move one panel of `panelize`
/// each, with that panel.
fn panel_makers(state: &DocState, panelize: FeatureUid) -> Vec<(FeatureUid, BodyUid)> {
    state
        .features
        .iter()
        .filter_map(|f| match &f.def {
            FeatureDef::ComponentFromBodies(c)
                if c.bodies.len() == 1 && c.bodies[0].feature == panelize =>
            {
                Some((f.uid, c.bodies[0]))
            }
            _ => None,
        })
        .collect()
}

impl DocState {
    /// Keeps a Panelize's panels in step with its definition after it was
    /// added or edited (see the module docs); nothing for other features.
    pub(crate) fn sync_panels(&mut self, uid: FeatureUid) -> Result<(), ModelError> {
        let Some(entry) = self.entry(uid) else {
            return Ok(());
        };
        let FeatureDef::Panelize(def) = &entry.def else {
            return Ok(());
        };
        let count = def.faces.len();
        let components = def.result == PanelResult::Components;
        let source = def.body;
        let component = entry.component;
        // Bodies of faces no longer there lose their names and looks.
        let gone = |b: &BodyUid| b.feature == uid && b.index as usize >= count;
        self.body_names.retain(|b, _| !gone(b));
        self.body_attributes.retain(|b, _| !gone(b));
        self.take_source_looks(uid, count, source, component, components);
        // The components of panels that should have none go.
        let makers = panel_makers(self, uid);
        let mut removed = BTreeSet::new();
        for (maker, body) in &makers {
            if components && (body.index as usize) < count {
                continue;
            }
            if let Some(made) = self.assembly.created_by(*maker)
                && (self.features.iter().any(|f| f.component == made)
                    || self.assembly.children(made).next().is_some()
                    || self.has_drawings(made))
            {
                return Err(invalid(format!(
                    "{} has features, occurrences or drawings of its own; delete them first, \
                     or keep Result Components",
                    self.assembly.name(made)
                )));
            }
            removed.insert(*maker);
        }
        if !removed.is_empty() {
            self.remove_features(&removed);
        }
        if !components {
            return Ok(());
        }
        // A component for each panel that has none yet, right after the
        // Panelize.
        for index in 0..count {
            let panel = panel_body(uid, index);
            if makers
                .iter()
                .any(|(m, b)| *b == panel && !removed.contains(m))
            {
                continue;
            }
            // After the Panelize and the components already made for it.
            let position = panel_makers(self, uid)
                .iter()
                .filter_map(|(m, _)| self.position(*m))
                .chain(self.position(uid))
                .max()
                .expect("exists");
            let marker = self.marker;
            self.marker = position + 1;
            let def =
                FeatureDef::<ValueInput>::ComponentFromBodies(ComponentFromBodiesDef::new(vec![
                    panel,
                ]));
            let added = self.add_feature_in(&def, None, component);
            self.marker = match &added {
                Ok(_) if marker > position => marker + 1,
                _ => marker,
            };
            let added = added?;
            // Named after its panel when the panel has a name already (an
            // edit from Result Bodies); else after the recompute names it.
            if let Some(name) = self.body_names.get(&panel).cloned()
                && let Some(made) = self.assembly.created_by(added.uid)
            {
                let free = self.free_component_name(&name);
                if let Some(c) = self.assembly.component_mut(made) {
                    c.name = free;
                }
            }
        }
        Ok(())
    }

    /// The body's material and appearance given to each panel that has no
    /// looks of its own; with Result Components the material its component
    /// gives it too.
    fn take_source_looks(
        &mut self,
        uid: FeatureUid,
        count: usize,
        source: BodyUid,
        component: ComponentUid,
        components: bool,
    ) {
        let own = self.body_attributes.get(&source);
        let mut material = own.and_then(|a| a.material.clone());
        let appearance = own.and_then(|a| a.appearance.clone());
        if material.is_none() && components {
            material = self.component_materials.get(&component).cloned();
        }
        if material.is_none() && appearance.is_none() {
            return;
        }
        for index in 0..count {
            let panel = panel_body(uid, index);
            if self.body_attributes.contains_key(&panel) {
                continue;
            }
            self.body_attributes.insert(
                panel,
                BodyAttributes {
                    material: material.clone(),
                    appearance: appearance.clone(),
                    ..BodyAttributes::default()
                },
            );
        }
    }
}

impl<K: Kernel> Document<K> {
    /// Names the panels that have no name yet by their faces' outward
    /// normals, and the components made for them after them (see the
    /// module docs). Run after a recompute, before the default names.
    pub(super) fn name_panels(&mut self) {
        let mut named: Vec<(BodyUid, String)> = Vec::new();
        for entry in &self.state.features {
            let FeatureDef::Panelize(def) = &entry.def else {
                continue;
            };
            for (index, face) in def.faces.iter().enumerate() {
                let body = panel_body(entry.uid, index);
                if self.state.body_names.contains_key(&body) {
                    continue;
                }
                let Some(shape) = self.result.bodies.get(&body) else {
                    continue;
                };
                let normal = self
                    .kernel
                    .face_plane(&shape.value, face)
                    .ok()
                    .map(|p| p.normal);
                named.push((body, panel_name(normal, index)));
            }
        }
        for (body, name) in named {
            let owner = self.result.owners.get(&body).copied();
            let taken: BTreeSet<&String> = self
                .state
                .body_names
                .iter()
                .filter(|(b, _)| self.result.owners.get(b).copied() == owner)
                .map(|(_, n)| n)
                .collect();
            let mut free = name.clone();
            let mut k = 2;
            while taken.contains(&free) {
                free = format!("{name} ({k})");
                k += 1;
            }
            self.state.body_names.insert(body, free.clone());
            // The component made for it, still with its default name.
            let maker = panel_makers(&self.state, body.feature)
                .into_iter()
                .find(|(_, b)| *b == body)
                .map(|(m, _)| m);
            if let Some(made) = maker.and_then(|m| self.state.assembly.created_by(m)) {
                let component = self.state.free_component_name(&free);
                if let Some(c) = self.state.assembly.component_mut(made) {
                    c.name = component;
                }
            }
        }
    }
}
