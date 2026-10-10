// SPDX-License-Identifier: MIT
//! Commands and views of components and occurrences (F6; the structure is
//! in `assembly.rs`): new components, Component from Bodies, activation,
//! grounding, visibility, names, placements, copies of occurrences (a new
//! occurrence of the same component, or Paste New: a new component), and
//! external components linked from or copied out of other project files.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::{Added, DocState, Document, ModelError, invalid};
use crate::assembly::{ExternalLink, Occurrence, is_rigid};
use crate::exchange::ImportBody;
use crate::features::{
    BaseDef, CapturePositionDef, ComponentFromBodiesDef, FeatureDef, Position, ValueInput,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::recompute::{Cache, recompute};
use crate::transform::Transform;

/// A body as placed in the design (see
/// [`crate::assembly::Assembly::instances`]).
#[derive(Debug, Clone, PartialEq)]
pub struct InstanceView {
    /// The occurrences from the root down to the body's component; empty
    /// for the root's own bodies.
    pub path: Vec<OccurrenceUid>,
    /// Occurrence names joined by `/`; empty for the root's bodies.
    pub path_name: String,
    pub component: ComponentUid,
    pub body: BodyUid,
    pub name: String,
    /// From the component's coordinates to the design's.
    pub transform: Transform,
    /// The body and every occurrence on the path are shown.
    pub visible: bool,
}

/// Options of [`Document::insert_component`].
#[derive(Debug, Clone, PartialEq)]
pub struct InsertOptions {
    /// Linked: the component shows the file's bodies, read only, and is
    /// read again when the file has changed ([`Document::update_links`]).
    /// Not linked: a copy of the file's design with its history, which the
    /// document owns.
    pub link: bool,
    /// Its placement in the active component.
    pub transform: Transform,
    /// The component's name; the file's name when None.
    pub name: Option<String>,
    /// The directory a relative path is relative to (the project's).
    pub base: Option<PathBuf>,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self {
            link: true,
            transform: Transform::IDENTITY,
            name: None,
            base: None,
        }
    }
}

impl DocState {
    /// Refuses changes to a component that does not exist or is linked
    /// from another file.
    pub(crate) fn editable(&self, component: ComponentUid) -> Result<(), ModelError> {
        let a = &self.assembly;
        if !a.exists(component) {
            return Err(invalid(format!("component {component} does not exist")));
        }
        if let Some(link) = a.component(component).and_then(|c| c.link.as_ref()) {
            return Err(invalid(format!(
                "{} is linked from {}; it changes only with that file",
                a.name(component),
                link.path
            )));
        }
        Ok(())
    }

    fn require_component(&self, uid: ComponentUid) -> Result<(), ModelError> {
        if self.assembly.exists(uid) {
            Ok(())
        } else {
            Err(invalid(format!("component {uid} does not exist")))
        }
    }

    pub(crate) fn require_occurrence(&self, uid: OccurrenceUid) -> Result<Occurrence, ModelError> {
        self.assembly
            .occurrence(uid)
            .cloned()
            .ok_or_else(|| invalid(format!("occurrence {uid} does not exist")))
    }

    /// The timeline features before the marker that place an occurrence.
    pub(crate) fn positioning_features(&self, uid: OccurrenceUid) -> Vec<String> {
        self.features[..self.marker]
            .iter()
            .filter(|f| !f.suppressed)
            .filter(|f| match &f.def {
                FeatureDef::MoveOccurrence(m) => m.occurrences.contains(&uid),
                FeatureDef::CapturePosition(c) => c.positions.iter().any(|p| p.occurrence == uid),
                _ => false,
            })
            .map(|f| f.name.clone())
            .collect()
    }

    /// A component name not in use: `name`, else `name (1)`, `name (2)`, ...
    pub(crate) fn free_component_name(&self, name: &str) -> String {
        let taken = |n: &str| {
            n == self.assembly.root_name || self.assembly.components.iter().any(|c| c.name == n)
        };
        std::iter::once(name.to_owned())
            .chain((1..).map(|k| format!("{name} ({k})")))
            .find(|n| !taken(n))
            .expect("a free name exists")
    }

    pub(crate) fn check_component_name(
        &self,
        name: &str,
        uid: ComponentUid,
    ) -> Result<(), ModelError> {
        if name.trim().is_empty() {
            return Err(invalid("the component name is empty"));
        }
        let a = &self.assembly;
        let taken = (!uid.is_root() && a.root_name == name)
            || a.components.iter().any(|c| c.name == name && c.uid != uid);
        if taken {
            return Err(invalid(format!("a component is already named '{name}'")));
        }
        Ok(())
    }

    /// Deletes components with their features, the features that made
    /// them (a component made from bodies gives them back to its parent)
    /// and what depends on those, and the components only they contained.
    /// Returns the deleted features in timeline order.
    pub(crate) fn delete_components(
        &mut self,
        components: BTreeSet<ComponentUid>,
    ) -> Vec<FeatureUid> {
        let mut gone = components;
        let mut makers = BTreeSet::new();
        loop {
            for c in &gone {
                if let Some(f) = self.assembly.component(*c).and_then(|d| d.created_by) {
                    makers.insert(f);
                }
                self.assembly.remove(*c);
            }
            // Components no longer placed anywhere go too.
            let orphans: BTreeSet<ComponentUid> = self
                .assembly
                .components
                .iter()
                .filter(|c| self.assembly.occurrences_of(c.uid).next().is_none())
                .map(|c| c.uid)
                .collect();
            if orphans.is_empty() {
                break;
            }
            gone = orphans;
        }
        let mut removed: BTreeSet<FeatureUid> = self
            .features
            .iter()
            .filter(|f| !self.assembly.exists(f.component))
            .map(|f| f.uid)
            .collect();
        removed.extend(makers);
        removed.extend(self.dependents(&removed));
        self.remove_features(&removed)
    }
}

impl<K: Kernel> Document<K> {
    pub fn assembly(&self) -> &crate::assembly::Assembly {
        &self.state.assembly
    }

    /// Where new features, components and occurrences go.
    pub fn active_component(&self) -> ComponentUid {
        self.state.assembly.active
    }

    /// An occurrence's placement in its parent at the timeline marker.
    pub fn placement(&self, uid: OccurrenceUid) -> Option<Transform> {
        self.result
            .placements
            .get(&uid)
            .copied()
            .or_else(|| self.state.assembly.occurrence(uid).map(|o| o.transform))
    }

    /// The transform of a path's last component into the design's
    /// coordinates.
    pub fn path_transform(&self, path: &[OccurrenceUid]) -> Transform {
        path.iter().fold(Transform::IDENTITY, |t, o| {
            t.after(&self.placement(*o).unwrap_or(Transform::IDENTITY))
        })
    }

    /// The bodies at the marker of a component.
    pub fn component_bodies(&self, component: ComponentUid) -> Vec<BodyUid> {
        self.result
            .owners
            .iter()
            .filter(|(_, c)| **c == component)
            .map(|(b, _)| *b)
            .collect()
    }

    /// Every body as placed in the design: the root's bodies, then those
    /// of each occurrence, depth first.
    pub fn instances(&self) -> Vec<InstanceView> {
        let a = &self.state.assembly;
        let placement = |o: OccurrenceUid| self.placement(o).unwrap_or(Transform::IDENTITY);
        let bodies = |c: ComponentUid| self.component_bodies(c);
        a.instances(&placement, &bodies)
            .into_iter()
            .map(|i| InstanceView {
                path_name: a.path_name(&i.path),
                name: self.body_name(i.body),
                visible: i.visible && self.body_attributes(i.body).visible,
                path: i.path,
                component: i.component,
                body: i.body,
                transform: i.transform,
            })
            .collect()
    }

    /// Makes an empty component placed in the active component, and
    /// activates it when asked (New Component).
    pub fn create_component(
        &mut self,
        name: Option<&str>,
        transform: Transform,
        activate: bool,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        check_transform(&transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        self.apply(|state| {
            let name = match name {
                Some(name) => {
                    state.check_component_name(name, ComponentUid(u32::MAX))?;
                    Some(name)
                }
                None => None,
            };
            let made = state.assembly.create(name, None);
            let occurrence = state.assembly.place(made, parent, transform);
            if activate {
                state.assembly.active = made;
            }
            let label = format!("New Component {}", state.assembly.name(made));
            Ok((label, (made, occurrence)))
        })
    }

    /// Makes a component placed in `parent` (an importer's structure): the
    /// name is made unique (`name (1)`), the active component stays.
    pub fn add_component(
        &mut self,
        name: Option<&str>,
        parent: ComponentUid,
        transform: Transform,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        check_transform(&transform)?;
        self.state.editable(parent)?;
        self.apply_with(|state| {
            let name = name
                .filter(|n| !n.trim().is_empty())
                .map(|n| state.free_component_name(n));
            let made = state.assembly.create(name.as_deref(), None);
            let occurrence = state.assembly.place(made, parent, transform);
            let label = format!("New Component {}", state.assembly.name(made));
            Ok((label, (made, occurrence), false))
        })
    }

    /// Places a component again, in `parent`, which must not be in it.
    pub fn add_occurrence(
        &mut self,
        component: ComponentUid,
        parent: ComponentUid,
        transform: Transform,
    ) -> Result<OccurrenceUid, ModelError> {
        check_transform(&transform)?;
        self.state.editable(parent)?;
        let a = &self.state.assembly;
        if component.is_root() || !a.exists(component) {
            return Err(invalid(format!("{component} cannot be placed")));
        }
        if a.contains(component, parent) {
            return Err(invalid(format!(
                "{} cannot be placed in {}, which is in it",
                a.name(component),
                a.name(parent)
            )));
        }
        self.apply_with(|state| {
            let occurrence = state.assembly.place(component, parent, transform);
            let label = format!("Place {}", state.assembly.occurrence_name(occurrence));
            Ok((label, occurrence, false))
        })
    }

    /// Create Components from Bodies: a component for each body,
    /// named after it, by a `component_from_bodies` feature in the body's
    /// component. Returns the features and the components.
    pub fn components_from_bodies(
        &mut self,
        bodies: &[BodyUid],
    ) -> Result<Vec<(Added, ComponentUid)>, ModelError> {
        if bodies.is_empty() {
            return Err(invalid("no bodies selected"));
        }
        let mut owners = Vec::with_capacity(bodies.len());
        for (i, body) in bodies.iter().enumerate() {
            if bodies[..i].contains(body) {
                return Err(invalid(format!("body {body} is listed more than once")));
            }
            let owner = self.body_component(*body).ok_or_else(|| {
                invalid(format!("body {body} does not exist at the timeline marker"))
            })?;
            self.state.editable(owner)?;
            owners.push((owner, self.body_name(*body)));
        }
        self.apply(|state| {
            let mut made = Vec::new();
            for (body, (owner, name)) in bodies.iter().zip(&owners) {
                let def = FeatureDef::<ValueInput>::ComponentFromBodies(
                    ComponentFromBodiesDef::new(vec![*body]),
                );
                let added = state.add_feature_in(&def, None, *owner)?;
                let component = state.assembly.created_by(added.uid).expect("made");
                let free = state.free_component_name(name);
                if let Some(c) = state.assembly.component_mut(component) {
                    c.name = free;
                }
                made.push((added, component));
            }
            Ok(("Create Components from Bodies".to_owned(), made))
        })
    }

    /// Makes a component active: new features, components and occurrences
    /// go into it.
    pub fn activate_component(&mut self, uid: ComponentUid) -> Result<(), ModelError> {
        self.state.editable(uid)?;
        if self.state.assembly.active == uid {
            return Ok(());
        }
        self.apply_with(|state| {
            state.assembly.active = uid;
            let label = format!("Activate {}", state.assembly.name(uid));
            Ok((label, (), false))
        })
    }

    pub fn rename_component(&mut self, uid: ComponentUid, name: &str) -> Result<(), ModelError> {
        self.state.require_component(uid)?;
        if self.state.assembly.name(uid) == name {
            return Ok(());
        }
        self.apply_with(|state| {
            state.check_component_name(name, uid)?;
            let label = format!("Rename {} to {name}", state.assembly.name(uid));
            if uid.is_root() {
                state.assembly.root_name = name.to_owned();
            } else if let Some(c) = state.assembly.component_mut(uid) {
                c.name = name.to_owned();
            }
            Ok((label, (), false))
        })
    }

    /// Grounds an occurrence (fixes it in place) or frees it.
    pub fn set_occurrence_grounded(
        &mut self,
        uid: OccurrenceUid,
        grounded: bool,
    ) -> Result<(), ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        if occurrence.grounded == grounded {
            return Ok(());
        }
        self.apply(|state| {
            let name = state.assembly.occurrence_name(uid);
            state.assembly.occurrence_mut(uid).expect("exists").grounded = grounded;
            let verb = if grounded { "Ground" } else { "Unground" };
            Ok((format!("{verb} {name}"), ()))
        })
    }

    /// Shows or hides an occurrence (and what it contains).
    pub fn set_occurrence_visible(
        &mut self,
        uid: OccurrenceUid,
        visible: bool,
    ) -> Result<(), ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        if occurrence.visible == visible {
            return Ok(());
        }
        self.apply_with(|state| {
            let name = state.assembly.occurrence_name(uid);
            state.assembly.occurrence_mut(uid).expect("exists").visible = visible;
            let verb = if visible { "Show" } else { "Hide" };
            Ok((format!("{verb} {name}"), (), false))
        })
    }

    /// Places an occurrence in its parent's coordinates. Without `capture`
    /// it is the occurrence's own placement, which only works while no
    /// timeline feature before the marker places it; with `capture` a
    /// `capture_position` feature at the marker records it (Capture
    /// Position), returned. A grounded occurrence does not move.
    pub fn set_occurrence_transform(
        &mut self,
        uid: OccurrenceUid,
        transform: Transform,
        capture: bool,
    ) -> Result<Option<Added>, ModelError> {
        check_transform(&transform)?;
        let occurrence = self.state.require_occurrence(uid)?;
        let name = self.state.assembly.occurrence_name(uid);
        if occurrence.grounded {
            return Err(invalid(format!("{name} is grounded")));
        }
        if capture {
            return self.capture_positions(&[(uid, transform)]).map(Some);
        }
        let by = self.state.positioning_features(uid);
        if !by.is_empty() {
            return Err(invalid(format!(
                "{name} is placed by {} in the timeline; capture the position instead, or edit \
                 that feature",
                by.join(", ")
            )));
        }
        self.apply(|state| {
            state
                .assembly
                .occurrence_mut(uid)
                .expect("exists")
                .transform = transform;
            Ok((format!("Move {name}"), ()))
        })?;
        Ok(None)
    }

    /// Adds a `capture_position` feature at the marker that puts the
    /// occurrences at these placements; they must share a parent component.
    pub fn capture_positions(
        &mut self,
        positions: &[(OccurrenceUid, Transform)],
    ) -> Result<Added, ModelError> {
        let Some((first, _)) = positions.first() else {
            return Err(invalid("no occurrences to capture"));
        };
        let parent = self.state.require_occurrence(*first)?.parent;
        for (uid, transform) in positions {
            check_transform(transform)?;
            let o = self.state.require_occurrence(*uid)?;
            if o.parent != parent {
                return Err(invalid(
                    "the occurrences of one captured position are placed in one component",
                ));
            }
        }
        let def = FeatureDef::<ValueInput>::CapturePosition(CapturePositionDef::new(
            positions
                .iter()
                .map(|(occurrence, transform)| Position {
                    occurrence: *occurrence,
                    transform: *transform,
                })
                .collect(),
        ));
        self.add_feature_to(&def, None, Some(parent))
    }

    /// Where a copy of an occurrence goes: the active component, which
    /// must not be in the copied component.
    fn paste_target(&self, source: &Occurrence) -> Result<ComponentUid, ModelError> {
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        let a = &self.state.assembly;
        if a.contains(source.component, parent) {
            return Err(invalid(format!(
                "{} cannot be placed in {}, which is in it",
                a.name(source.component),
                a.name(parent)
            )));
        }
        Ok(parent)
    }

    /// A new occurrence of the same component in the active component
    /// (copy and paste of an occurrence), at `transform` or where
    /// the copied one is in its parent.
    pub fn copy_occurrence(
        &mut self,
        uid: OccurrenceUid,
        transform: Option<Transform>,
    ) -> Result<OccurrenceUid, ModelError> {
        let source = self.state.require_occurrence(uid)?;
        let transform =
            transform.unwrap_or_else(|| self.placement(uid).unwrap_or(source.transform));
        check_transform(&transform)?;
        let parent = self.paste_target(&source)?;
        self.apply(|state| {
            let copy = state.assembly.place(source.component, parent, transform);
            let label = format!("Paste {}", state.assembly.occurrence_name(copy));
            Ok((label, copy))
        })
    }

    /// Paste New: a new component copied from an occurrence's
    /// component, with its features and the components placed in it,
    /// placed in the active component. A component whose bodies come from
    /// outside its own features (Component from Bodies, a linked file) is
    /// copied as its bodies. Returns the component and its occurrence.
    pub fn paste_new(
        &mut self,
        uid: OccurrenceUid,
        transform: Option<Transform>,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        let source = self.state.require_occurrence(uid)?;
        let transform =
            transform.unwrap_or_else(|| self.placement(uid).unwrap_or(source.transform));
        check_transform(&transform)?;
        let parent = self.paste_target(&source)?;
        let component = source.component;
        let def = self
            .state
            .assembly
            .component(component)
            .cloned()
            .expect("exists");
        let made_by = def
            .created_by
            .and_then(|f| self.state.entry(f))
            .map(|f| (f.uid, matches!(f.def, FeatureDef::ComponentFromBodies(_))));
        // Bodies from outside the component's features: copied as bodies.
        let as_bodies = def.link.is_some() || made_by.is_some_and(|(_, from_bodies)| from_bodies);
        let snapshot = if as_bodies {
            Some(self.component_snapshot(component)?)
        } else {
            None
        };
        let source_state = self.state.clone();
        let mut source_path = crate::links::first_path(&source_state.assembly, source.parent, None)
            .map_err(invalid)?;
        source_path.push(uid);
        self.apply(|state| {
            let name = state.free_component_name(&def.name);
            let (made, occurrence) = match (snapshot, made_by) {
                (Some(bodies), _) => {
                    let made = state.assembly.create(Some(&name), None);
                    if !bodies.is_empty() {
                        let base = BaseDef {
                            source: Some(format!("copy of {}", def.name)),
                            ..BaseDef::new(bodies)
                        };
                        state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                    }
                    // The components placed in it are shared.
                    let children: Vec<Occurrence> =
                        source_state.assembly.children(component).cloned().collect();
                    for child in children {
                        let copy = state.assembly.place(child.component, made, child.transform);
                        let o = state.assembly.occurrence_mut(copy).expect("placed");
                        o.grounded = child.grounded;
                        o.visible = child.visible;
                    }
                    let occurrence = state.assembly.place(made, parent, transform);
                    (made, occurrence)
                }
                (None, Some((feature, _))) => {
                    // The feature that made it is copied too, in its own
                    // component, and makes the copy.
                    let made = state.assembly.create(Some(&name), None);
                    let occurrence = state.assembly.place(made, parent, transform);
                    state.copy_component(&source_state, &source_path, made, Some(feature), true)?;
                    (made, occurrence)
                }
                (None, None) => {
                    let made = state.assembly.create(Some(&name), None);
                    let occurrence = state.assembly.place(made, parent, transform);
                    state.copy_component(&source_state, &source_path, made, None, true)?;
                    (made, occurrence)
                }
            };
            let label = format!("Paste New {}", state.assembly.name(made));
            Ok((label, (made, occurrence)))
        })
    }

    /// A component's bodies at the marker as base-feature bodies.
    fn component_snapshot(
        &self,
        component: ComponentUid,
    ) -> Result<Vec<crate::features::BaseBody>, ModelError> {
        let bodies: Vec<ImportBody<K::Shape>> = self
            .component_bodies(component)
            .into_iter()
            .filter_map(|uid| {
                Some(ImportBody {
                    name: Some(self.body_name(uid)),
                    color: None,
                    shape: self.body_shape(uid)?.clone(),
                })
            })
            .collect();
        self.base_bodies(&bodies)
    }

    /// Deletes an occurrence. The last occurrence of a component takes the
    /// component with it: its features, the feature that made it, what
    /// depends on them and the components only it contained. Returns the
    /// deleted features.
    pub fn delete_occurrence(&mut self, uid: OccurrenceUid) -> Result<Vec<FeatureUid>, ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        self.state.editable(occurrence.parent)?;
        self.apply(|state| {
            let name = state.assembly.occurrence_name(uid);
            state.assembly.occurrences.retain(|o| o.uid != uid);
            let last = state
                .assembly
                .occurrences_of(occurrence.component)
                .next()
                .is_none();
            let mut deleted = if last {
                state.delete_components(BTreeSet::from([occurrence.component]))
            } else {
                Vec::new()
            };
            // Joints and rigid groups of the occurrences that went go too
            // (mitcad#55), with what depends on them.
            let mut orphans = crate::joints::orphaned(state);
            if !orphans.is_empty() {
                orphans.extend(state.dependents(&orphans));
                deleted.extend(state.remove_features(&orphans));
            }
            Ok((format!("Delete {name}"), deleted))
        })
    }

    /// Inserts another project file as a component placed in the active
    /// component (Insert Component): linked (read only,
    /// its bodies as a base feature, read again when the file changes) or
    /// a copy of its design with its history.
    pub fn insert_component(
        &mut self,
        path: &str,
        options: &InsertOptions,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        check_transform(&options.transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        let resolved = resolve_path(path, options.base.as_deref());
        let text = std::fs::read_to_string(&resolved)
            .map_err(|e| invalid(format!("cannot read {path}: {e}")))?;
        let (source, _) = crate::file::load_state_at(&text, &resolved)
            .map_err(|e| invalid(format!("{path}: {e}")))?;
        let stem = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .filter(|s| !s.trim().is_empty())
            .unwrap_or("Inserted")
            .to_owned();
        let wanted = options.name.clone().unwrap_or(stem);
        let bodies = if options.link {
            Some(self.design_bodies(&source, path)?)
        } else {
            None
        };
        let label = format!("Insert {}", file_label(path));
        self.apply(|state| {
            let name = match &options.name {
                Some(name) => {
                    state.check_component_name(name, ComponentUid(u32::MAX))?;
                    name.clone()
                }
                None => state.free_component_name(&wanted),
            };
            let made = state.assembly.create(Some(&name), None);
            let occurrence = state.assembly.place(made, parent, options.transform);
            match bodies {
                Some(bodies) => {
                    let base = BaseDef {
                        source: Some(file_label(path)),
                        ..BaseDef::new(bodies)
                    };
                    let added = state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                    state.assembly.component_mut(made).expect("made").link = Some(ExternalLink {
                        path: path.to_owned(),
                        digest: digest(&text),
                        feature: added.uid,
                    });
                }
                None => {
                    state.copy_component(&source, &[], made, None, false)?;
                }
            }
            Ok((label, (made, occurrence)))
        })
    }

    /// Copies another document's design (its root component with its
    /// features, parameters and components) into a new component placed in
    /// `parent` (an importer's structure: the `.iam` import copies each
    /// part, imported into a document of its own, once). The name is made
    /// unique; the active component stays.
    pub fn add_component_copy(
        &mut self,
        source: &Document<K>,
        name: Option<&str>,
        parent: ComponentUid,
        transform: Transform,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        check_transform(&transform)?;
        self.state.editable(parent)?;
        let source = &source.state;
        self.apply(|state| {
            let name = name
                .filter(|n| !n.trim().is_empty())
                .map(|n| state.free_component_name(n));
            let made = state.assembly.create(name.as_deref(), None);
            let occurrence = state.assembly.place(made, parent, transform);
            state.copy_component(source, &[], made, None, false)?;
            let label = format!("Copy into {}", state.assembly.name(made));
            Ok((label, (made, occurrence)))
        })
    }

    /// The visible bodies of another design, placed in its coordinates,
    /// as base-feature bodies.
    pub(super) fn design_bodies(
        &self,
        source: &DocState,
        path: &str,
    ) -> Result<Vec<crate::features::BaseBody>, ModelError> {
        let kernel = &self.kernel;
        // A cancel (P7) stops this too.
        let monitor = self.monitor.as_ref();
        let result = recompute(kernel, source, &mut Cache::default(), monitor)?;
        let names = super::display_names(source, &result);
        let a = &source.assembly;
        let placement = |o: OccurrenceUid| {
            result
                .placements
                .get(&o)
                .copied()
                .unwrap_or(Transform::IDENTITY)
        };
        let owned = |c: ComponentUid| {
            result
                .owners
                .iter()
                .filter(|(_, owner)| **owner == c)
                .map(|(b, _)| *b)
                .collect::<Vec<_>>()
        };
        let mut bodies = Vec::new();
        for instance in a.instances(&placement, &owned) {
            let shown = source
                .body_attributes
                .get(&instance.body)
                .is_none_or(|b| b.visible);
            if !(instance.visible && shown) {
                continue;
            }
            let Some(shape) = result.bodies.get(&instance.body) else {
                continue;
            };
            let shape = if instance.transform.is_identity() {
                shape.value.clone()
            } else {
                kernel
                    .transform_shape(&shape.value, &instance.transform, None)
                    .map_err(|e| invalid(format!("{path}: {e}")))?
            };
            bodies.push(ImportBody {
                name: names.get(&instance.body).cloned(),
                color: None,
                shape,
            });
        }
        if bodies.is_empty() {
            return Err(invalid(format!("{path} has no bodies to show")));
        }
        self.base_bodies(&bodies)
    }

    /// Reads linked components' files again and takes their bodies when a
    /// file has changed; `base` is the directory of the project file.
    /// Returns what happened, one line per link that changed or could not
    /// be read (a component whose file is missing keeps the bodies saved
    /// with the project).
    pub fn update_links(&mut self, base: Option<&Path>) -> Result<Vec<String>, ModelError> {
        let links: Vec<(ComponentUid, ExternalLink)> = self
            .state
            .assembly
            .components
            .iter()
            // Library parts are read from their libraries (mitcad#64).
            .filter(|c| c.library.is_none())
            .filter_map(|c| Some((c.uid, c.link.clone()?)))
            .collect();
        let mut messages = Vec::new();
        let mut updates = self.library_link_updates(&mut messages)?;
        for (component, link) in links {
            let name = self.state.assembly.name(component);
            let resolved = resolve_path(&link.path, base);
            let text = match std::fs::read_to_string(&resolved) {
                Ok(text) => text,
                Err(e) => {
                    messages.push(format!(
                        "{name}: cannot read {} ({e}); it keeps the bodies saved with the project",
                        link.path
                    ));
                    continue;
                }
            };
            let new_digest = digest(&text);
            if new_digest == link.digest {
                continue;
            }
            let bodies = match crate::file::load_state_at(&text, &resolved) {
                Ok((source, _)) => self.design_bodies(&source, &link.path),
                Err(e) => Err(invalid(e.to_string())),
            };
            match bodies {
                Ok(bodies) => {
                    messages.push(format!("{name} is updated from {}", link.path));
                    updates.push((component, link, new_digest, bodies));
                }
                // Cancelled (P7): the document stays as it was.
                Err(ModelError::Cancelled) => return Err(ModelError::Cancelled),
                Err(e) => messages.push(format!(
                    "{name}: {e}; it keeps the bodies saved with the project"
                )),
            }
        }
        // Part of opening the file: no undo step, but a new revision. The
        // new state is computed before it is taken.
        if !updates.is_empty() {
            let mut state = self.state.clone();
            for (component, link, digest, bodies) in updates {
                let position = state.require(link.feature)?;
                let mut entry = (*state.features[position]).clone();
                if let FeatureDef::Base(base) = &mut entry.def {
                    base.bodies = bodies;
                }
                state.features[position] = std::sync::Arc::new(entry);
                if let Some(c) = state.assembly.component_mut(component)
                    && let Some(l) = c.link.as_mut()
                {
                    l.digest = digest;
                }
            }
            let result = self.compute(&state)?;
            self.state = state;
            self.state_changed();
            self.set_result(result);
            self.name_new_bodies();
        }
        self.warnings.extend(messages.iter().cloned());
        Ok(messages)
    }
}

/// Occurrences are placed by rotations and translations only.
pub(super) fn check_transform(transform: &Transform) -> Result<(), ModelError> {
    if transform.is_finite() && is_rigid(transform) {
        Ok(())
    } else {
        Err(invalid(
            "an occurrence transform must be a rotation and a translation",
        ))
    }
}

fn resolve_path(path: &str, base: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    match base {
        Some(base) if p.is_relative() => base.join(p),
        _ => p.to_path_buf(),
    }
}

/// The file name of a path, for labels.
pub(super) fn file_label(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_owned()
}

/// FNV-1a of the text, to tell whether a linked file changed.
pub(super) fn design_digest(text: &str) -> String {
    digest(text)
}

fn digest(text: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}")
}
