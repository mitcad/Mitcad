// SPDX-License-Identifier: MIT
//! Commands and views of components and occurrences (F6; the structure is
//! in `assembly.rs`): new components, Component from Bodies, activation,
//! grounding, visibility, names, placements, copies of occurrences (a new
//! occurrence of the same component, or Paste New: a new component), and
//! external components linked from or copied out of other design files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::str::FromStr;

use super::bodies::faces_named;
use super::copy::CopySource;
use super::{Added, BodyAttributes, ComponentCopy, DocState, Document, ModelError, invalid};
use crate::assembly::{ExternalLink, Occurrence, is_rigid};
use crate::exchange::ImportBody;
use crate::features::{
    BaseBody, BaseDef, CapturePositionDef, ComponentFromBodiesDef, FeatureDef, Position, ValueInput,
};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::kernel::Kernel;
use crate::recompute::{Cache, recompute};
use crate::topo::{FaceName, RoleKey};
use crate::transform::Transform;

/// Attributes paired with a snapshot body. Face indices are in the
/// snapshot's B-rep order, across all bodies of its future base feature.
struct SnapshotAttributes {
    body: BodyAttributes,
    faces: BTreeMap<u32, String>,
    /// The print bed face's index (mitcad#731).
    bed: Option<u32>,
}

struct ComponentSnapshot {
    bodies: Vec<BaseBody>,
    attributes: Vec<SnapshotAttributes>,
}

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
    /// Placed by one of its mount frames on a face, a hole or a grid, with
    /// the joint the frame asks for (mitcad#525); `transform` is then not
    /// used. None: dropped in at `transform`.
    pub mount: Option<super::mounts::MountInput>,
    /// Linked with options (mitcad#801): values of the design's or the
    /// library component's user parameters, expressions of this design's
    /// parameters; the component is then made by a derived part that
    /// follows them. None: its bodies as they are (or as its row makes
    /// them).
    pub parameters: Option<BTreeMap<String, crate::features::ValueInput>>,
}

impl Default for InsertOptions {
    fn default() -> Self {
        Self {
            link: true,
            transform: Transform::IDENTITY,
            name: None,
            base: None,
            mount: None,
            parameters: None,
        }
    }
}

impl DocState {
    /// Refuses changes to a component that does not exist or is linked
    /// from another file.
    pub fn editable(&self, component: ComponentUid) -> Result<(), ModelError> {
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
                FeatureDef::PatternAtPoints(_) => self
                    .assembly
                    .occurrence(uid)
                    .and_then(|o| o.pattern.as_ref())
                    .is_some_and(|p| p.feature == f.uid),
                _ => false,
            })
            .map(|f| f.name.clone())
            .collect()
    }

    /// A component name not in use: `name`, else `name (1)`, `name (2)`, ...
    pub fn free_component_name(&self, name: &str) -> String {
        let taken = |n: &str| {
            n == self.assembly.root_name || self.assembly.components.iter().any(|c| c.name == n)
        };
        std::iter::once(name.to_owned())
            .chain((1..).map(|k| format!("{name} ({k})")))
            .find(|n| !taken(n))
            .expect("a free name exists")
    }

    /// The free name a feature gives the component its new-component
    /// operation makes (mitcad#577), or None for `Component<n>`.
    pub(crate) fn made_component_name(&self, def: &crate::features::FeatureDef) -> Option<String> {
        def.info()
            .component_name(&|id| self.parameters.value(id).unwrap_or(f64::NAN))
            .map(|name| self.free_component_name(&name))
    }

    pub fn check_component_name(&self, name: &str, uid: ComponentUid) -> Result<(), ModelError> {
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
    pub fn delete_components(&mut self, components: BTreeSet<ComponentUid>) -> Vec<FeatureUid> {
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
        // Their drawings go with them (mitcad#707).
        self.drop_orphan_drawings();
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
        self.result.placements.get(&uid).copied().or_else(|| {
            self.state
                .assembly
                .occurrence(uid)
                .filter(|o| o.pattern.is_none())
                .map(|o| o.transform)
        })
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
        let a = self.state.assembly.at_placements(&self.result.placements);
        let placement = |o: OccurrenceUid| self.placement(o).unwrap_or(Transform::IDENTITY);
        let bodies = |c: ComponentUid| self.component_bodies(c);
        // The bodies' names at once: a name per instance would be
        // quadratic in the bodies (mitcad#413: an imported building).
        let names = super::display_names(&self.state, &self.result);
        a.instances(&placement, &bodies)
            .into_iter()
            .map(|i| InstanceView {
                path_name: a.path_name(&i.path),
                name: names
                    .get(&i.body)
                    .cloned()
                    .unwrap_or_else(|| i.body.to_string()),
                visible: i.visible && self.body_attributes(i.body).visible,
                path: i.path,
                component: i.component,
                body: i.body,
                transform: i.transform,
            })
            .collect()
    }

    /// Makes an empty component placed in the active component, and
    /// activates it when asked (New Component). While policy checks are on,
    /// an active component whose policy takes no components has it placed
    /// in the nearest owner above that does (mitcad#292,
    /// `Document::component_parent_beside`).
    pub fn create_component(
        &mut self,
        name: Option<&str>,
        transform: Transform,
        activate: bool,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        self.create_component_with(name, transform, activate, crate::policy::Policy::GENERAL)
    }

    /// [`Document::create_component`] with the new component's policy
    /// (mitcad#220).
    pub fn create_component_with(
        &mut self,
        name: Option<&str>,
        transform: Transform,
        activate: bool,
        policy: crate::policy::Policy,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        check_transform(&transform)?;
        let active = self.state.assembly.active;
        self.state.editable(active)?;
        // An active component that takes no components: beside it, where
        // it shows (mitcad#292).
        let (parent, transform) = match self.component_parent_beside(active) {
            Some((parent, placement)) => (parent, placement.after(&transform)),
            None => (active, transform),
        };
        self.apply(|state| {
            let name = match name {
                Some(name) => {
                    state.check_component_name(name, ComponentUid(u32::MAX))?;
                    Some(name)
                }
                None => None,
            };
            let made = state.assembly.create(name, None);
            state.assembly.set_policy(made, policy);
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

    /// Gives an occurrence a component of its own: a new empty component
    /// (named `name`, made unique) that the occurrence places from now on,
    /// so that it can be arranged differently from the other occurrences
    /// of the component it placed (an importer's: the occurrences of one
    /// other document whose insides are not the same, mitcad#196). The
    /// occurrence keeps its uid and placement and becomes the new
    /// component's first (`Name:1`). Nothing is copied into the new
    /// component ([`Document::fill_component_copy`] does that). An
    /// occurrence that a pattern owns shares its source's component.
    pub fn give_occurrence_own_component(
        &mut self,
        occurrence: OccurrenceUid,
        name: Option<&str>,
    ) -> Result<ComponentUid, ModelError> {
        let placed = self.state.require_occurrence(occurrence)?;
        self.state.editable(placed.parent)?;
        if placed.pattern.is_some() {
            return Err(invalid(format!(
                "{} is owned by a pattern",
                self.state.assembly.occurrence_name(occurrence)
            )));
        }
        self.apply(|state| {
            let name = name
                .filter(|n| !n.trim().is_empty())
                .map(|n| state.free_component_name(n));
            let made = state.assembly.create(name.as_deref(), None);
            let label = format!(
                "Own component {} for {}",
                state.assembly.name(made),
                state.assembly.occurrence_name(occurrence)
            );
            let o = state
                .assembly
                .occurrence_mut(occurrence)
                .expect("the occurrence exists");
            o.component = made;
            o.number = 1;
            Ok((label, made))
        })
    }

    /// Makes an occurrence place another component than it placed (an
    /// importer's: the occurrences of one other document that share the
    /// arrangement inside, mitcad#196, join the component made for it by
    /// [`Document::give_occurrence_own_component`]). The occurrence keeps
    /// its uid and placement and takes the next number of the component.
    pub fn place_other_component(
        &mut self,
        occurrence: OccurrenceUid,
        component: ComponentUid,
    ) -> Result<(), ModelError> {
        let placed = self.state.require_occurrence(occurrence)?;
        self.state.editable(placed.parent)?;
        let a = &self.state.assembly;
        if component.is_root() || !a.exists(component) {
            return Err(invalid(format!("{component} cannot be placed")));
        }
        if a.contains(component, placed.parent) {
            return Err(invalid(format!(
                "{} cannot be placed in {}, which is in it",
                a.name(component),
                a.name(placed.parent)
            )));
        }
        if placed.pattern.is_some() {
            return Err(invalid(format!(
                "{} is owned by a pattern",
                a.occurrence_name(occurrence)
            )));
        }
        self.apply(|state| {
            let number = state
                .assembly
                .occurrences_of(component)
                .map(|o| o.number)
                .max()
                .unwrap_or(0)
                + 1;
            let label = format!(
                "Place {} instead of {}",
                state.assembly.name(component),
                state.assembly.occurrence_name(occurrence)
            );
            let o = state
                .assembly
                .occurrence_mut(occurrence)
                .expect("the occurrence exists");
            o.component = component;
            o.number = number;
            Ok((label, ()))
        })
    }

    /// Make Unique: gives an occurrence a copy of its component, with the
    /// features and the components placed in it (those are copied too),
    /// which the occurrence places from now on, so that it can be changed
    /// without changing the other occurrences of the component (an
    /// importer's: occurrences of a component inside a copy of another
    /// document that the design arranges differently, mitcad#196). The
    /// occurrence keeps its uid and placement. Returns the new component
    /// and what was copied, each occurrence and feature of the original
    /// with its copy.
    pub fn make_unique(
        &mut self,
        uid: OccurrenceUid,
    ) -> Result<(ComponentUid, ComponentCopy), ModelError> {
        let source = self.state.require_occurrence(uid)?;
        self.state.editable(source.parent)?;
        if source.pattern.is_some() {
            return Err(invalid(format!(
                "{} is owned by a pattern",
                self.state.assembly.occurrence_name(uid)
            )));
        }
        let def = self
            .state
            .assembly
            .component(source.component)
            .cloned()
            .ok_or_else(|| invalid("the component does not exist"))?;
        let made_from_bodies = def
            .created_by
            .and_then(|f| self.state.entry(f))
            .is_some_and(|f| matches!(f.def, FeatureDef::ComponentFromBodies(_)));
        if def.link.is_some() || made_from_bodies || self.state.transfers_into(source.component) {
            return Err(invalid(format!(
                "{} has bodies from outside its own features",
                def.name
            )));
        }
        let source_state = self.state.clone();
        let mut source_path = crate::links::first_path(&source_state.assembly, source.parent, None)
            .map_err(invalid)?;
        source_path.push(uid);
        self.apply(|state| {
            let name = state.free_component_name(&def.name);
            let made = state.assembly.create(Some(&name), None);
            let o = state
                .assembly
                .occurrence_mut(uid)
                .expect("the occurrence exists");
            o.component = made;
            o.number = 1;
            let (_, copy) = state.copy_component_mapped(
                &source_state,
                &source_path,
                made,
                None,
                CopySource::SameDocument,
            )?;
            let label = format!("Make {name} unique");
            Ok((label, (made, copy)))
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

    /// The component a new-component feature made becomes `component`
    /// instead: an existing component with nothing in it yet (no features,
    /// no occurrences placed in it, not linked, made by no feature), which
    /// keeps its name and occurrences. The component the feature made goes,
    /// with its occurrence; the feature's bodies go into `component` from
    /// then on. For an import that places a file's components before the
    /// timeline item that made one (a component from bodies, mitcad#156).
    pub fn move_made_component(
        &mut self,
        feature: FeatureUid,
        component: ComponentUid,
    ) -> Result<(), ModelError> {
        let a = &self.state.assembly;
        let made = a
            .created_by(feature)
            .ok_or_else(|| invalid(format!("{feature} made no component")))?;
        if made == component {
            return Ok(());
        }
        let empty = |c: ComponentUid| {
            !self.state.features.iter().any(|f| f.component == c) && a.children(c).next().is_none()
        };
        let free = a
            .component(component)
            .is_some_and(|c| c.created_by.is_none() && c.link.is_none() && c.library.is_none());
        if !free || !empty(component) {
            return Err(invalid(format!(
                "{} is not an empty component of this design",
                a.name(component)
            )));
        }
        if !empty(made) {
            return Err(invalid(format!(
                "{} has features or occurrences of its own",
                a.name(made)
            )));
        }
        self.apply(|state| {
            let label = format!(
                "Move {} into {}",
                state.assembly.name(made),
                state.assembly.name(component)
            );
            state.move_drawings(made, component);
            state.assembly.remove(made);
            if let Some(c) = state.assembly.component_mut(component) {
                c.created_by = Some(feature);
            }
            Ok((label, ()))
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

    /// Makes a component active as the document opens (mitcad#426): the
    /// application activates the root of every design it opens, whatever
    /// active component the file stores. No undo step and no new revision,
    /// so a design opened unmodified stays unmodified and the undo and redo
    /// steps stay as they were; nothing evaluation reads changes, so
    /// nothing is recomputed. Saving stores the component active then.
    pub fn activate_component_on_open(&mut self, uid: ComponentUid) -> Result<(), ModelError> {
        self.state.editable(uid)?;
        self.state.assembly.active = uid;
        Ok(())
    }

    /// Renames a component. A component (not the root) whose only body
    /// still has a default name (`Body<n>`) gives the body its new name too,
    /// unless another body has that name (mitcad#291): one undo step.
    /// Returns the body renamed with it.
    pub fn rename_component(
        &mut self,
        uid: ComponentUid,
        name: &str,
    ) -> Result<Option<BodyUid>, ModelError> {
        self.state.require_component(uid)?;
        if self.state.assembly.name(uid) == name {
            return Ok(None);
        }
        let body = self.body_renamed_with(uid, name);
        self.apply_with(|state| {
            state.check_component_name(name, uid)?;
            let label = format!("Rename {} to {name}", state.assembly.name(uid));
            if uid.is_root() {
                state.assembly.root_name = name.to_owned();
            } else if let Some(c) = state.assembly.component_mut(uid) {
                c.name = name.to_owned();
            }
            if let Some(body) = body {
                state.body_names.insert(body, name.to_owned());
            }
            Ok((label, body, false))
        })
    }

    /// The body that a component's rename to `name` renames too: its only
    /// body, while that has a default name and no other body is `name`.
    fn body_renamed_with(&self, uid: ComponentUid, name: &str) -> Option<BodyUid> {
        if uid.is_root() || name.trim().is_empty() {
            return None;
        }
        let bodies = self.component_bodies(uid);
        let [body] = bodies[..] else {
            return None;
        };
        let names = super::display_names(&self.state, &self.result);
        let default = names.get(&body).is_some_and(|n| is_default_body_name(n));
        let taken = names.iter().any(|(other, n)| *other != body && n == name);
        (default && !taken).then_some(body)
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

    /// Shows or hides an occurrence (and what it contains). A condition its
    /// visibility followed (mitcad#800) is removed in the same undo step.
    pub fn set_occurrence_visible(
        &mut self,
        uid: OccurrenceUid,
        visible: bool,
    ) -> Result<(), ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        if occurrence.visible == visible
            && !self
                .state
                .bindings
                .occurrence_visible_when
                .contains_key(&uid)
        {
            return Ok(());
        }
        self.apply_with(|state| {
            state.bindings.occurrence_visible_when.remove(&uid);
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
        let as_bodies = def.link.is_some()
            || made_by.is_some_and(|(_, from_bodies)| from_bodies)
            || self.state.transfers_into(component);
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
                (Some(snapshot), _) => {
                    let made = state.assembly.create(Some(&name), None);
                    // The copy keeps the policy (mitcad#220).
                    state.assembly.set_policy(made, def.policy);
                    if !snapshot.bodies.is_empty() {
                        let base = BaseDef {
                            source: Some(format!("copy of {}", def.name)),
                            ..BaseDef::new(snapshot.bodies)
                        };
                        let added = state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                        for (i, attributes) in snapshot.attributes.into_iter().enumerate() {
                            let index = u32::try_from(i)
                                .map_err(|_| invalid("too many snapshot bodies"))?;
                            let mut body = attributes.body;
                            body.face_appearances = attributes
                                .faces
                                .into_iter()
                                .map(|(index, appearance)| {
                                    let face = FaceName::new(
                                        added.uid,
                                        "import",
                                        Some(RoleKey::Index(index)),
                                    );
                                    (face.to_string(), appearance)
                                })
                                .collect();
                            // The print bed face by its new name (mitcad#731).
                            body.print_bed_face = attributes.bed.map(|index| {
                                FaceName::new(added.uid, "import", Some(RoleKey::Index(index)))
                                    .to_string()
                            });
                            if !body.is_default() {
                                state
                                    .body_attributes
                                    .insert(BodyUid::new(added.uid, index), body);
                            }
                        }
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
                    state.copy_component(
                        &source_state,
                        &source_path,
                        made,
                        Some(feature),
                        CopySource::SameDocument,
                    )?;
                    (made, occurrence)
                }
                (None, None) => {
                    let made = state.assembly.create(Some(&name), None);
                    let occurrence = state.assembly.place(made, parent, transform);
                    state.copy_component(
                        &source_state,
                        &source_path,
                        made,
                        None,
                        CopySource::SameDocument,
                    )?;
                    (made, occurrence)
                }
            };
            let label = format!("Paste New {}", state.assembly.name(made));
            Ok((label, (made, occurrence)))
        })
    }

    /// A component's bodies at the marker with their attributes. The base
    /// feature gives faces new import names, so resolve current overrides
    /// to the B-rep's face order before their old names are lost.
    fn component_snapshot(&self, component: ComponentUid) -> Result<ComponentSnapshot, ModelError> {
        let source = self.component_bodies(component);
        let map_faces = source.iter().any(|uid| {
            let attributes = self.body_attributes(*uid);
            !attributes.face_appearances.is_empty() || attributes.print_bed_face.is_some()
        });
        let mut bodies = Vec::with_capacity(source.len());
        let mut attributes = Vec::with_capacity(source.len());
        let mut first_face = 0u32;
        for uid in source {
            let Some(shape) = self.body_shape(uid) else {
                continue;
            };
            let mut body = self.body_attributes(uid);
            let mut mapped = BTreeMap::new();
            let mut bed = None;
            if map_faces {
                let faces = self.kernel().faces(shape).map_err(|e| {
                    invalid(format!(
                        "cannot copy appearances of {}: {e}",
                        self.body_name(uid)
                    ))
                })?;
                let names: Vec<Vec<FaceName>> = faces
                    .iter()
                    .map(|face| {
                        face.names
                            .iter()
                            .filter_map(|name| FaceName::from_str(name).ok())
                            .collect()
                    })
                    .collect();
                for (reference, appearance) in &body.face_appearances {
                    let reference =
                        FaceName::from_str(reference).map_err(|e| invalid(e.to_string()))?;
                    let resolved: BTreeSet<String> =
                        faces_named(&names, &reference).into_iter().collect();
                    for (i, names) in names.iter().enumerate() {
                        if names
                            .iter()
                            .any(|name| resolved.contains(&name.to_string()))
                        {
                            let index = u32::try_from(i)
                                .ok()
                                .and_then(|i| first_face.checked_add(i))
                                .ok_or_else(|| invalid("too many snapshot faces"))?;
                            mapped.insert(index, appearance.clone());
                        }
                    }
                }
                // The print bed face (mitcad#731): the first face it finds.
                if let Some(reference) = body
                    .print_bed_face
                    .as_deref()
                    .and_then(|face| FaceName::from_str(face).ok())
                {
                    let resolved: BTreeSet<String> =
                        faces_named(&names, &reference).into_iter().collect();
                    bed = names
                        .iter()
                        .position(|names| {
                            names
                                .iter()
                                .any(|name| resolved.contains(&name.to_string()))
                        })
                        .and_then(|i| u32::try_from(i).ok())
                        .and_then(|i| first_face.checked_add(i));
                }
                first_face = u32::try_from(faces.len())
                    .ok()
                    .and_then(|count| first_face.checked_add(count))
                    .ok_or_else(|| invalid("too many snapshot faces"))?;
            }
            body.face_appearances.clear();
            body.print_bed_face = None;
            attributes.push(SnapshotAttributes {
                body,
                faces: mapped,
                bed,
            });
            bodies.push(ImportBody {
                name: Some(self.body_name(uid)),
                color: None,
                shape: shape.clone(),
            });
        }
        Ok(ComponentSnapshot {
            bodies: self.base_bodies(&bodies)?,
            attributes,
        })
    }

    /// Deletes an occurrence. The last occurrence of a component takes the
    /// component with it: its features, the feature that made it, what
    /// depends on them and the components only it contained. Returns the
    /// deleted features.
    pub fn delete_occurrence(&mut self, uid: OccurrenceUid) -> Result<Vec<FeatureUid>, ModelError> {
        let occurrence = self.state.require_occurrence(uid)?;
        if occurrence.pattern.is_some() {
            return Err(invalid(
                "edit or delete the Pattern at Points feature to remove its instances",
            ));
        }
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

    /// Inserts another design file as a component placed in the active
    /// component (Insert Component): linked (read only,
    /// its bodies as a base feature, read again when the file changes) or
    /// a copy of its design with its history.
    pub fn insert_component(
        &mut self,
        path: &str,
        options: &InsertOptions,
    ) -> Result<(ComponentUid, OccurrenceUid), ModelError> {
        self.insert_file(path, options)
            .map(|inserted| (inserted.component, inserted.occurrence))
    }

    /// [`Document::insert_component`], with the mount frames the new
    /// component has and the joint a mount placement made (mitcad#525).
    pub fn insert_file(
        &mut self,
        path: &str,
        options: &InsertOptions,
    ) -> Result<super::mounts::Inserted, ModelError> {
        if let Some(parameters) = &options.parameters {
            return self.insert_file_with_options(path, options, parameters);
        }
        check_transform(&options.transform)?;
        let parent = self.state.assembly.active;
        self.state.editable(parent)?;
        let resolved = std::path::absolute(resolve_path(path, options.base.as_deref()))
            .map_err(|e| invalid(format!("cannot resolve {path}: {e}")))?;
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
        // Its mount frames (mitcad#525): a linked part's are joint origins
        // of its own where the design has them; a copy has the design's.
        let (bodies, mounts) = if options.link {
            let (bodies, mounts) = self.design_parts(&source, path)?;
            (Some(bodies), mounts)
        } else if options.mount.is_some() {
            (None, self.design_mounts(&source)?)
        } else {
            (None, Vec::new())
        };
        let listed = super::mounts::listed_mounts(&source);
        let plan = match &options.mount {
            Some(mount) => Some(self.plan_mount(&mounts, mount, &wanted)?),
            None => None,
        };
        let transform = plan.as_ref().map_or(options.transform, |p| p.placement);
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
            // The design's root policy comes with it (mitcad#220).
            state.assembly.set_policy(made, source.assembly.root_policy);
            let occurrence = state.assembly.place(made, parent, transform);
            let frames = match bodies {
                Some(bodies) => {
                    let base = BaseDef {
                        source: Some(file_label(path)),
                        ..BaseDef::new(bodies)
                    };
                    let added = state.add_feature_in(&FeatureDef::Base(base), None, made)?;
                    let frames = super::mounts::add_linked_mounts(state, made, &mounts)?;
                    state.assembly.component_mut(made).expect("made").link = Some(ExternalLink {
                        path: recorded_link_path(path, &resolved, options.base.as_deref()),
                        digest: digest(&text),
                        feature: added.uid,
                    });
                    frames
                }
                None => {
                    let (_, copy) = state.copy_component_mapped(
                        &source,
                        &[],
                        made,
                        None,
                        CopySource::Document(resolved.parent()),
                    )?;
                    super::mounts::copied_mounts(state, made, &listed, &copy)
                }
            };
            let joint = match &plan {
                Some(plan) => Some(super::mounts::attach_mount(
                    state, parent, occurrence, &frames, plan,
                )?),
                None => None,
            };
            let inserted = super::mounts::Inserted {
                component: made,
                occurrence,
                joint,
                frames,
                plan,
                moves: Vec::new(),
                broken: None,
            };
            Ok((label, inserted))
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
        let directory = source.source_directory();
        let source = &source.state;
        self.apply(|state| {
            let name = name
                .filter(|n| !n.trim().is_empty())
                .map(|n| state.free_component_name(n));
            let made = state.assembly.create(name.as_deref(), None);
            let occurrence = state.assembly.place(made, parent, transform);
            state.copy_component(
                source,
                &[],
                made,
                None,
                CopySource::Document(directory.as_deref()),
            )?;
            let label = format!("Copy into {}", state.assembly.name(made));
            Ok((label, (made, occurrence)))
        })
    }

    /// Copies another document's design (its root component with its
    /// features, parameters and components) into `into`, an empty
    /// component of this document that is already placed (an importer's:
    /// the `.f3d` import fills the components standing for components of
    /// other documents from those documents' files, mitcad#196), and names
    /// it `name` (made unique). Returns the copied occurrences and
    /// features, each of the source's with its copy (a body of the source
    /// `F5.b0` is the copy's `F<copy of 5>.b0`). The active component stays.
    pub fn fill_component_copy(
        &mut self,
        source: &Document<K>,
        into: ComponentUid,
        name: Option<&str>,
    ) -> Result<ComponentCopy, ModelError> {
        self.state.editable(into)?;
        if into.is_root() {
            return Err(invalid("the root component cannot be filled with a copy"));
        }
        if self.state.features.iter().any(|f| f.component == into)
            || self.state.assembly.children(into).next().is_some()
        {
            return Err(invalid(format!(
                "{} is not empty",
                self.state.assembly.name(into)
            )));
        }
        let directory = source.source_directory();
        let source = &source.state;
        self.apply(|state| {
            if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                let name = state.free_component_name(name);
                if let Some(c) = state.assembly.component_mut(into) {
                    c.name = name;
                }
            }
            let (_, copy) = state.copy_component_mapped(
                source,
                &[],
                into,
                None,
                CopySource::Document(directory.as_deref()),
            )?;
            let label = format!("Copy into {}", state.assembly.name(into));
            Ok((label, copy))
        })
    }

    /// The visible bodies of another design, placed in its coordinates,
    /// as base-feature bodies, with the design's mount frames
    /// (mitcad#525), from one evaluation.
    pub(super) fn design_parts(
        &self,
        source: &DocState,
        path: &str,
    ) -> Result<
        (
            Vec<crate::features::BaseBody>,
            Vec<super::mounts::SourceMount>,
        ),
        ModelError,
    > {
        let (bodies, mounts) = self.design_shapes(source, path)?;
        Ok((self.base_bodies(&bodies)?, mounts))
    }

    /// The visible bodies of another design as shapes placed in its
    /// coordinates, and its mount frames (mitcad#525).
    pub(super) fn design_shapes(
        &self,
        source: &DocState,
        path: &str,
    ) -> Result<DesignShapes<K::Shape>, ModelError> {
        let kernel = &self.kernel;
        // A cancel (P7) stops this too.
        let monitor = self.monitor.as_ref();
        let result = recompute(kernel, source, &mut Cache::default(), monitor)?;
        let mounts = super::mounts::evaluated_mounts(source, &result);
        let names = super::display_names(source, &result);
        let a = source.assembly.at_placements(&result.placements);
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
        Ok((bodies, mounts))
    }

    /// The design of a file to insert and the name its component takes
    /// (mitcad#525: a mount placement's preview).
    pub(super) fn file_design(
        &self,
        path: &str,
        base: Option<&Path>,
    ) -> Result<(DocState, String), ModelError> {
        let resolved = std::path::absolute(resolve_path(path, base))
            .map_err(|e| invalid(format!("cannot resolve {path}: {e}")))?;
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
        Ok((source, stem))
    }

    /// Reads linked components' files again and takes their bodies when a
    /// file has changed; `base` is the directory of the design file.
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
            // A design linked with options is a source (mitcad#801).
            .filter(|c| self.state.options_part(c.uid).is_none())
            .filter_map(|c| Some((c.uid, c.link.clone()?)))
            .collect();
        let mut messages = Vec::new();
        let mut updates = self.library_link_updates(&mut messages)?;
        let sources = self.options_file_updates(base, &mut messages);
        for (component, link) in links {
            let name = self.state.assembly.name(component);
            // Only a file in the design's project is read (mitcad#929).
            let resolved = match followed_link(&link.path, base) {
                Ok(resolved) => resolved,
                Err(why) => {
                    messages.push(format!(
                        "{name}: {why}; it keeps the bodies saved with the project"
                    ));
                    continue;
                }
            };
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
            let parts = match crate::file::load_state_at(&text, &resolved) {
                Ok((source, _)) => self.design_parts(&source, &link.path),
                Err(e) => Err(invalid(e.to_string())),
            };
            match parts {
                Ok((bodies, mounts)) => {
                    messages.push(format!("{name} is updated from {}", link.path));
                    updates.push((component, link, new_digest, bodies, mounts));
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
        if !updates.is_empty() || !sources.is_empty() {
            let mut state = self.state.clone();
            super::component_options::apply_file_updates(&mut state, sources)?;
            for (component, link, digest, bodies, mounts) in updates {
                // Its mount frames follow the file (mitcad#525).
                super::mounts::update_linked_mounts(&mut state, component, &mounts)?;
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
            // A file read again leaves its old source unused (mitcad#801).
            state.prune_library_sources(&self.state);
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

pub(super) fn resolve_path(path: &str, base: Option<&Path>) -> PathBuf {
    let p = Path::new(path);
    match base {
        Some(base) if p.is_relative() => base.join(p),
        _ => p.to_path_buf(),
    }
}

/// What a linked component's file must be in (mitcad#929): the project of
/// the design in `base`, its folder when it is in no project.
fn link_root(base: &Path) -> Option<PathBuf> {
    let base = normalized(&std::path::absolute(base).ok()?);
    // `find` looks from a file's folder upwards: a name in `base`.
    Some(match crate::Project::find(&base.join("design")) {
        Some(project) => normalized(project.root()),
        None => base,
    })
}

/// The file a saved link is followed to when a design opens (mitcad#929):
/// its path against the design's folder `base`, when it stays in the
/// design's project (or, for a design in no project, its folder). A link
/// elsewhere, an absolute path or a `..` that leaves the project, is not
/// followed, and nothing is read from it: the design file does not choose
/// which files outside its project the application opens. Without a
/// folder (a design not saved) no link is followed.
pub(super) fn followed_link(path: &str, base: Option<&Path>) -> Result<PathBuf, String> {
    let refused = || format!("the link outside the project is not followed ({path})");
    let base = base.ok_or_else(refused)?;
    let root = link_root(base).ok_or_else(refused)?;
    let joined = std::path::absolute(base.join(path)).map_err(|_| refused())?;
    let target = normalized(&joined);
    if target != root && target.starts_with(&root) {
        Ok(target)
    } else {
        Err(refused())
    }
}

/// The path a new link records (mitcad#929): relative to the design's
/// folder when the file is in the design's project, so that the link is
/// followed when the design opens again (also on another computer, or
/// with the project moved); otherwise the path as given.
pub(super) fn recorded_link_path(path: &str, resolved: &Path, base: Option<&Path>) -> String {
    let Some(base) = base else {
        return path.to_owned();
    };
    let (Some(root), Ok(folder)) = (link_root(base), std::path::absolute(base)) else {
        return path.to_owned();
    };
    let target = normalized(resolved);
    if !target.starts_with(&root) {
        return path.to_owned();
    }
    relative_path(&target, &normalized(&folder)).unwrap_or_else(|| path.to_owned())
}

/// `target` relative to the folder `from`, with `/` between the names;
/// None when they share no root.
fn relative_path(target: &Path, from: &Path) -> Option<String> {
    use std::path::Component;
    let target: Vec<Component> = target.components().collect();
    let from: Vec<Component> = from.components().collect();
    let common = target.iter().zip(&from).take_while(|(a, b)| a == b).count();
    if common == 0 || common == target.len() {
        return None;
    }
    let mut names = vec!["..".to_owned(); from.len() - common];
    for c in &target[common..] {
        match c {
            Component::Normal(name) => names.push(name.to_str()?.to_owned()),
            _ => return None,
        }
    }
    Some(names.join("/"))
}

/// A path without `.` and `..` (taken away lexically, without reading the
/// file system; `..` at the root stays at the root).
fn normalized(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => match out.components().next_back() {
                Some(Component::Normal(_)) => {
                    out.pop();
                }
                Some(Component::ParentDir) | None => out.push(".."),
                Some(_) => {}
            },
            other => out.push(other.as_os_str()),
        }
    }
    out
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

/// Whether a body name is a generated one, `Body<n>` (mitcad#291).
fn is_default_body_name(name: &str) -> bool {
    name.strip_prefix("Body")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// The visible bodies of another design as shapes in its coordinates, and its
/// mount frames (`design_shapes`, mitcad#525).
type DesignShapes<S> = (Vec<ImportBody<S>>, Vec<super::mounts::SourceMount>);
