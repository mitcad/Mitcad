// SPDX-License-Identifier: MIT
//! A whole design linked with its components (mitcad#2209).
//!
//! Insert Component, linked, of a whole design that has components keeps
//! the design's structure, read only: the linked component holds the
//! bodies of the design's root (a base feature that may hold none), each
//! component of the design becomes a component of its own placed in it
//! (or in another of them) as the design places it, holding the
//! component's own bodies as a base feature, and the design's joints,
//! as-built joints and rigid groups come along as features of the
//! components they belong to, their occurrences grounded as the design
//! grounds them. A joint's origins become fixed planes at the frames the
//! design gives them, its values numbers, so that nothing of it depends
//! on the design's history. [`crate::assembly::LinkTree`] records what was
//! made for what, by the design's uids.
//!
//! None of it can be changed here ([`DocState::editable`] refuses the
//! components, so their features and occurrences, and the joints of the
//! linked component itself): it changes only with the file. An occurrence
//! of the linked component can be made Flexible as any subassembly's
//! (mitcad#1793, mitcad#2016): its joints then move in its parent for it
//! alone, and the parent's joints may join the design's parts.
//!
//! Reading the file again (Update Link, Update All Links, opening the
//! design) updates everything in place: bodies, placements, grounding,
//! joints (whose uids stay, so a flexible occurrence keeps its own values
//! of them, within their new limits), new components, occurrences and
//! joints come in, and those the design no longer has go. A part gone
//! that a feature of this design uses (a joint of the parent on it) stays
//! where it was, with its saved bodies and a message; a flexible
//! occurrence's own values of a joint gone are dropped with a message.
//!
//! A design without components, chosen parts (mitcad#2130) and links
//! saved before keep the flat form: the bodies as one base feature.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::copy::CopySource;
use super::linked_looks::SourceLook;
use super::mounts::SourceMount;
use super::{ComponentCopy, DocState, Document, ModelError, invalid};
use crate::assembly::{Flex, Instance, LinkTree, LinkedComponent, LinkedFeature, LinkedOccurrence};
use crate::exchange::ImportBody;
use crate::features::{BaseBody, BaseDef, FeatureDef, JointOrigin, ValueInput};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::Motion;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::policy::Policy;
use crate::recompute::{Cache, recompute};
use crate::transform::Transform;

/// A design read to link with its components: its root's own bodies and
/// mount frames, its components, occurrences and joints.
pub(super) struct TreeRead {
    /// The root's own visible bodies, in its coordinates, and what the
    /// design gives each (mitcad#1921).
    pub root: Vec<BaseBody>,
    pub root_looks: Vec<SourceLook>,
    /// The design's mount frames (mitcad#525), as a flat link has them.
    pub mounts: Vec<SourceMount>,
    pub components: Vec<ReadComponent>,
    pub occurrences: Vec<ReadOccurrence>,
    pub features: Vec<ReadFeature>,
}

/// A component of the design placed in it, with its own visible bodies in
/// its coordinates.
pub(super) struct ReadComponent {
    pub source: ComponentUid,
    pub name: String,
    pub policy: Policy,
    pub bodies: Vec<BaseBody>,
    pub looks: Vec<SourceLook>,
}

/// An occurrence of the design: where the design places it at its marker.
pub(super) struct ReadOccurrence {
    pub source: OccurrenceUid,
    pub component: ComponentUid,
    pub parent: ComponentUid,
    pub transform: Transform,
    pub visible: bool,
    pub transparent: bool,
    pub grounded: bool,
    /// Flexible in the design, with its own values (mitcad#1805), by the
    /// design's uids.
    pub flexible: Option<Flex>,
}

/// A joint, as-built joint or rigid group of the design: its definition
/// with numbers for values and fixed planes for origins, its paths still
/// the design's occurrences.
pub(super) struct ReadFeature {
    pub source: FeatureUid,
    pub name: String,
    pub component: ComponentUid,
    pub def: FeatureDef<ValueInput>,
}

/// A linked design kept with its components, read again: the component,
/// the file's digest and name, and what was read.
pub(super) struct TreeUpdate {
    pub component: ComponentUid,
    pub digest: String,
    pub file: String,
    pub read: TreeRead,
}

/// Whether a whole design linked now keeps its components: it has
/// components placed in its root.
pub(super) fn has_components(source: &DocState) -> bool {
    source
        .assembly
        .children(ComponentUid::ROOT)
        .any(|o| o.pattern.is_none())
}

/// A joint origin at a frame: a fixed plane through it on the same path.
fn fixed_origin(origin: &JointOrigin, frame: &Transform) -> JointOrigin {
    JointOrigin {
        occurrence: origin.occurrence.clone(),
        geometry: super::mounts::fixed_plane(frame),
        frame_override: None,
    }
}

/// A base feature of a design linked with its components: its bodies
/// from the file `file`, none allowed.
fn tree_base(bodies: Vec<BaseBody>, file: &str) -> FeatureDef<ValueInput> {
    FeatureDef::Base(BaseDef {
        source: Some(file.to_owned()),
        linked_tree: true,
        ..BaseDef::new(bodies)
    })
}

/// A read feature's definition with the occurrences made for the design's
/// (`occurrences`), in the component made for its own; None when one of
/// its occurrences was not made.
fn tree_feature_def(
    f: &ReadFeature,
    components: &BTreeMap<ComponentUid, ComponentUid>,
    occurrences: &BTreeMap<OccurrenceUid, OccurrenceUid>,
) -> Option<(ComponentUid, FeatureDef<ValueInput>)> {
    let component = *components.get(&f.component)?;
    let mut def = f.def.clone();
    let map = |path: &mut Vec<OccurrenceUid>| -> Option<()> {
        for o in path.iter_mut() {
            *o = *occurrences.get(o)?;
        }
        Some(())
    };
    match &mut def {
        FeatureDef::Joint(j) => {
            map(&mut j.a.occurrence.0)?;
            map(&mut j.b.occurrence.0)?;
        }
        FeatureDef::AsBuiltJoint(j) => {
            map(&mut j.a.0)?;
            map(&mut j.b.0)?;
            if let Some(origin) = &mut j.origin {
                map(&mut origin.occurrence.0)?;
            }
        }
        FeatureDef::RigidGroup(g) => map(&mut g.occurrences)?,
        _ => return None,
    }
    Some((component, def))
}

impl<K: Kernel> Document<K> {
    /// The design `source` (read from `path`; the images of its
    /// appearances through `origin`) to link with its components: one
    /// evaluation at its marker. Its joints that do not hold there, are
    /// suppressed or rolled back are left out, as are its constraint sets
    /// and motion links. Refused when it has no bodies to show.
    pub(super) fn design_tree(
        &self,
        source: &DocState,
        path: &str,
        origin: &CopySource<'_>,
    ) -> Result<TreeRead, ModelError> {
        let kernel = &self.kernel;
        // A cancel (P7) stops this too.
        let monitor = self.monitor.as_ref();
        let result = recompute(kernel, source, &mut Cache::default(), monitor)?;
        let mounts = super::mounts::evaluated_mounts(source, &result);
        let names = super::display_names(source, &result);
        let a = source.assembly.at_placements(&result.placements);
        // The components placed in the design, in its order.
        let placed: Vec<ComponentUid> = a
            .components
            .iter()
            .map(|c| c.uid)
            .filter(|c| !a.paths_to(*c).is_empty())
            .collect();
        let inside = |c: ComponentUid| c.is_root() || placed.contains(&c);
        // A component's own visible bodies, in its coordinates.
        let own = |c: ComponentUid| -> Result<(Vec<BaseBody>, Vec<SourceLook>), ModelError> {
            let path = a.paths_to(c).into_iter().next().unwrap_or_default();
            let mut bodies = Vec::new();
            let mut looks = Vec::new();
            for (body, owner) in &result.owners {
                let shown = source.body_attributes.get(body).is_none_or(|b| b.visible);
                if *owner != c || !shown {
                    continue;
                }
                let Some(shape) = result.bodies.get(body) else {
                    continue;
                };
                bodies.push(ImportBody {
                    name: names.get(body).cloned(),
                    color: None,
                    shape: shape.value.clone(),
                });
                let instance = Instance {
                    path: path.clone(),
                    component: c,
                    body: *body,
                    transform: Transform::IDENTITY,
                    visible: true,
                };
                looks.push(super::linked_looks::source_look(source, &instance, origin));
            }
            Ok((self.base_bodies(&bodies)?, looks))
        };
        let (root, root_looks) = own(ComponentUid::ROOT)?;
        let mut components = Vec::with_capacity(placed.len());
        for c in &placed {
            let (bodies, looks) = own(*c)?;
            components.push(ReadComponent {
                source: *c,
                name: a.name(*c),
                policy: a.policy(*c),
                bodies,
                looks,
            });
        }
        if root.is_empty() && components.iter().all(|c| c.bodies.is_empty()) {
            return Err(invalid(format!("{path} has no bodies to show")));
        }
        let occurrences = a
            .occurrences
            .iter()
            .filter(|o| inside(o.parent) && inside(o.component))
            .map(|o| ReadOccurrence {
                source: o.uid,
                component: o.component,
                parent: o.parent,
                transform: result
                    .placements
                    .get(&o.uid)
                    .copied()
                    .unwrap_or(o.transform),
                visible: o.visible,
                transparent: o.transparent,
                grounded: o.grounded,
                flexible: o.flexible.clone(),
            })
            .collect();
        // The joints that hold at the marker, with their values as numbers
        // and their origins where the design has them.
        let holding: BTreeSet<FeatureUid> = result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        let mut features = Vec::new();
        for entry in &source.features[..source.marker] {
            let wanted = matches!(
                entry.def,
                FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_) | FeatureDef::RigidGroup(_)
            );
            if !wanted
                || entry.suppressed
                || !inside(entry.component)
                || !holding.contains(&entry.uid)
            {
                continue;
            }
            let number = entry.def.map_params(&mut |_, id: &ParamId| {
                source
                    .parameters
                    .value(*id)
                    .map(ValueInput::Number)
                    .ok_or(())
            });
            let Ok(mut def) = number else {
                continue;
            };
            let fixed = |o: &JointOrigin| {
                crate::joints::resolved_origin_frame(kernel, source, &result, entry.component, o)
                    .ok()
                    .map(|frame| fixed_origin(o, &frame))
            };
            let framed = match (&entry.def, &mut def) {
                (FeatureDef::Joint(j), FeatureDef::Joint(d)) => match (fixed(&j.a), fixed(&j.b)) {
                    (Some(a), Some(b)) => {
                        d.a = a;
                        d.b = b;
                        true
                    }
                    _ => false,
                },
                (FeatureDef::AsBuiltJoint(j), FeatureDef::AsBuiltJoint(d)) => match &j.origin {
                    None => true,
                    Some(o) => {
                        d.origin = fixed(o);
                        d.origin.is_some()
                    }
                },
                _ => true,
            };
            if framed {
                features.push(ReadFeature {
                    source: entry.uid,
                    name: entry.name.clone(),
                    component: entry.component,
                    def,
                });
            }
        }
        Ok(TreeRead {
            root,
            root_looks,
            mounts,
            components,
            occurrences,
            features,
        })
    }
}

impl DocState {
    /// Makes the components, occurrences and joints of a design read to
    /// link with its components in the linked component `made` (its own
    /// base feature and mount frames are made by the insert); `file` names
    /// the file in the base features. Returns what was made for what.
    pub(super) fn build_link_tree(
        &mut self,
        made: ComponentUid,
        read: &TreeRead,
        file: &str,
    ) -> Result<LinkTree, ModelError> {
        let mut tree = LinkTree::default();
        let mut components = BTreeMap::from([(ComponentUid::ROOT, made)]);
        for c in &read.components {
            let record = self.new_tree_component(c, file)?;
            components.insert(c.source, record.component);
            tree.components.push(record);
        }
        let mut occurrences = BTreeMap::new();
        for o in &read.occurrences {
            let (Some(component), Some(parent)) =
                (components.get(&o.component), components.get(&o.parent))
            else {
                continue;
            };
            let uid = self.place_tree_occurrence(*component, *parent, o);
            occurrences.insert(o.source, uid);
            tree.occurrences.push(LinkedOccurrence {
                source: o.source,
                occurrence: uid,
                gone: false,
            });
        }
        for f in &read.features {
            let Some((component, def)) = tree_feature_def(f, &components, &occurrences) else {
                continue;
            };
            let feature = self.add_tree_feature(f, component, &def)?;
            tree.features.push(LinkedFeature {
                source: f.source,
                feature,
            });
        }
        self.take_inner_flex(read, &tree);
        Ok(tree)
    }

    /// A component made for one of the design's, with its bodies.
    fn new_tree_component(
        &mut self,
        c: &ReadComponent,
        file: &str,
    ) -> Result<LinkedComponent, ModelError> {
        let name = self.free_component_name(&c.name);
        let component = self.assembly.create(Some(&name), None);
        self.assembly.set_policy(component, c.policy);
        let added = self.add_feature_in(&tree_base(c.bodies.clone(), file), None, component)?;
        self.give_new_link_looks(added.uid, &c.looks)?;
        Ok(LinkedComponent {
            source: c.source,
            name: c.name.clone(),
            component,
            feature: added.uid,
            gone: false,
        })
    }

    /// An occurrence made for one of the design's, as the design has it.
    fn place_tree_occurrence(
        &mut self,
        component: ComponentUid,
        parent: ComponentUid,
        o: &ReadOccurrence,
    ) -> OccurrenceUid {
        let uid = self.assembly.place(component, parent, o.transform);
        if let Some(placed) = self.assembly.occurrence_mut(uid) {
            placed.grounded = o.grounded;
            placed.visible = o.visible;
            placed.transparent = o.transparent;
        }
        uid
    }

    /// A joint, as-built joint or rigid group made for one of the design's,
    /// under its name there when that is free.
    fn add_tree_feature(
        &mut self,
        f: &ReadFeature,
        component: ComponentUid,
        def: &FeatureDef<ValueInput>,
    ) -> Result<FeatureUid, ModelError> {
        let name = (!f.name.trim().is_empty() && self.check_feature_name(&f.name).is_ok())
            .then(|| f.name.clone());
        Ok(self.add_feature_in(def, name.as_deref(), component)?.uid)
    }

    /// The occurrences inside the linked component flexible as the design
    /// has them (mitcad#1805), with its own values of the joints made here.
    fn take_inner_flex(&mut self, read: &TreeRead, tree: &LinkTree) {
        for o in &read.occurrences {
            let Some(uid) = tree.occurrence(o.source) else {
                continue;
            };
            let flex = o
                .flexible
                .as_ref()
                .map(|f| f.remapped(&|j| tree.feature(j), &|x| tree.occurrence(x)));
            if let Some(placed) = self.assembly.occurrence_mut(uid) {
                placed.flexible = flex;
            }
        }
    }

    /// A base feature of the tree takes new bodies, with what the design
    /// gives them (mitcad#1921).
    fn set_tree_bodies(
        &mut self,
        feature: FeatureUid,
        mut bodies: Vec<BaseBody>,
        looks: &[SourceLook],
    ) -> Result<(), ModelError> {
        let position = self.require(feature)?;
        let mut entry = (*self.features[position]).clone();
        if let FeatureDef::Base(base) = &mut entry.def {
            self.give_linked_looks(feature, &base.bodies, &mut bodies, looks);
            base.bodies = bodies;
            base.linked_tree = true;
        }
        self.features[position] = Arc::new(entry);
        Ok(())
    }

    /// The occurrences that features other than the linked design's own
    /// use (a joint of the parent on a part of it): every occurrence on
    /// their paths.
    fn used_outside(&self, tree: &LinkTree, made: ComponentUid) -> BTreeSet<OccurrenceUid> {
        self.features
            .iter()
            .filter(|f| !tree.made(f.uid) && f.component != made && !tree.holds(f.component))
            .flat_map(|f| crate::joints::referenced_occurrences(&f.def))
            .collect()
    }

    /// A joint's limits now: per motion its least and greatest value.
    fn joint_limits(&self, joint: FeatureUid) -> BTreeMap<Motion, (Option<f64>, Option<f64>)> {
        let limits = match self.entry(joint).map(|e| &e.def) {
            Some(FeatureDef::Joint(j)) => &j.limits,
            Some(FeatureDef::AsBuiltJoint(j)) => &j.limits,
            _ => return BTreeMap::new(),
        };
        let value = |id: Option<ParamId>| id.and_then(|id| self.parameters.value(id));
        limits
            .iter()
            .map(|(m, l)| (*m, (value(l.min), value(l.max))))
            .collect()
    }

    /// A linked design kept with its components read again: everything
    /// made for it updated in place (see the module docs). Adds a line to
    /// `messages` per part gone that stays and per flexible occurrence's
    /// own value dropped.
    fn update_link_tree(
        &mut self,
        update: TreeUpdate,
        messages: &mut Vec<String>,
    ) -> Result<(), ModelError> {
        let TreeUpdate {
            component: made,
            digest,
            file,
            read,
        } = update;
        let Some(link) = self.assembly.component(made).and_then(|c| c.link.clone()) else {
            return Ok(());
        };
        let Some(mut tree) = link.tree.clone() else {
            return Ok(());
        };
        // The root's own bodies and mount frames.
        super::mounts::update_linked_mounts(self, made, &read.mounts)?;
        self.set_tree_bodies(link.feature, read.root.clone(), &read.root_looks)?;

        // The components: taken again, or new.
        let mut components = BTreeMap::from([(ComponentUid::ROOT, made)]);
        for record in &mut tree.components {
            record.gone = true;
        }
        for c in &read.components {
            let found = tree.components.iter().position(|t| {
                t.source == c.source
                    && self.assembly.exists(t.component)
                    && self.entry(t.feature).is_some()
            });
            match found {
                Some(i) => {
                    let (component, feature) =
                        (tree.components[i].component, tree.components[i].feature);
                    self.set_tree_bodies(feature, c.bodies.clone(), &c.looks)?;
                    // Renamed in the design: renamed here.
                    if tree.components[i].name != c.name {
                        let name = self.free_component_name(&c.name);
                        if let Some(def) = self.assembly.component_mut(component) {
                            def.name = name;
                        }
                    }
                    self.assembly.set_policy(component, c.policy);
                    tree.components[i].name = c.name.clone();
                    tree.components[i].gone = false;
                    components.insert(c.source, component);
                }
                None => {
                    let record = self.new_tree_component(c, &file)?;
                    components.insert(c.source, record.component);
                    tree.components.push(record);
                }
            }
        }

        // The occurrences: placed and grounded again, or new. Shown and
        // see-through stay as this design has them.
        for record in &mut tree.occurrences {
            record.gone = true;
        }
        let mut occurrences = BTreeMap::new();
        for o in &read.occurrences {
            let (Some(component), Some(parent)) = (
                components.get(&o.component).copied(),
                components.get(&o.parent).copied(),
            ) else {
                continue;
            };
            let found = tree.occurrences.iter().position(|t| {
                t.source == o.source
                    && self
                        .assembly
                        .occurrence(t.occurrence)
                        .is_some_and(|x| x.component == component)
            });
            let uid = match found {
                Some(i) => {
                    tree.occurrences[i].gone = false;
                    let uid = tree.occurrences[i].occurrence;
                    if let Some(x) = self.assembly.occurrence_mut(uid) {
                        x.parent = parent;
                        x.transform = o.transform;
                        x.grounded = o.grounded;
                    }
                    uid
                }
                None => {
                    let uid = self.place_tree_occurrence(component, parent, o);
                    tree.occurrences.push(LinkedOccurrence {
                        source: o.source,
                        occurrence: uid,
                        gone: false,
                    });
                    uid
                }
            };
            occurrences.insert(o.source, uid);
        }

        // The joints, as-built joints and rigid groups: edited in place
        // (their uids stay), new, or gone.
        let mut kept = Vec::new();
        for f in &read.features {
            let Some((component, def)) = tree_feature_def(f, &components, &occurrences) else {
                continue;
            };
            let same = tree.feature(f.source).filter(|uid| {
                self.entry(*uid).is_some_and(|e| {
                    e.component == component && e.def.type_name() == def.type_name()
                })
            });
            let feature = match same {
                Some(uid) => {
                    self.edit_feature(uid, &def)?;
                    uid
                }
                None => self.add_tree_feature(f, component, &def)?,
            };
            kept.push(LinkedFeature {
                source: f.source,
                feature,
            });
        }
        let gone: BTreeSet<FeatureUid> = tree
            .features
            .iter()
            .map(|f| f.feature)
            .filter(|uid| !kept.iter().any(|k| k.feature == *uid) && self.entry(*uid).is_some())
            .collect();
        let gone_names: BTreeMap<FeatureUid, String> = gone
            .iter()
            .filter_map(|uid| Some((*uid, self.entry(*uid)?.name.clone())))
            .collect();
        if !gone.is_empty() {
            let mut removed = gone.clone();
            removed.extend(self.dependents(&gone));
            self.remove_features(&removed);
        }
        tree.features = kept;

        // A flexible occurrence of the linked component keeps its own
        // values of the joints still there, within their limits now; those
        // of joints gone are dropped, with a line.
        let flexible: Vec<OccurrenceUid> = self
            .assembly
            .occurrences
            .iter()
            .filter(|o| o.flexible.is_some() && o.component == made)
            .map(|o| o.uid)
            .collect();
        for uid in flexible {
            let name = self.assembly.occurrence_name(uid);
            let Some(mut flex) = self
                .assembly
                .occurrence(uid)
                .and_then(|o| o.flexible.clone())
            else {
                continue;
            };
            for (joint, joint_name) in &gone_names {
                if flex.position.contains_key(joint) || flex.start.contains_key(joint) {
                    messages.push(format!(
                        "{name}: its own value of {joint_name} is dropped, as {file} no longer \
                         has the joint"
                    ));
                }
            }
            flex.position.retain(|j, _| self.entry(*j).is_some());
            flex.start.retain(|j, _| self.entry(*j).is_some());
            for (joint, values) in flex.position.iter_mut().chain(flex.start.iter_mut()) {
                let limits = self.joint_limits(*joint);
                for (motion, v) in values.iter_mut() {
                    if let Some((min, max)) = limits.get(motion) {
                        *v = v
                            .max(min.unwrap_or(f64::NEG_INFINITY))
                            .min(max.unwrap_or(f64::INFINITY));
                    }
                }
            }
            if let Some(o) = self.assembly.occurrence_mut(uid) {
                o.flexible = Some(flex);
            }
        }
        // The occurrences inside it flexible as the design has them.
        self.take_inner_flex(&read, &tree);

        // The parts the design no longer has: those a feature of this
        // design uses stay where they are, with their saved bodies and a
        // line; the others go, with the components no longer placed.
        let used = self.used_outside(&tree, made);
        let mut dropped = BTreeSet::new();
        for record in tree.occurrences.iter().filter(|t| t.gone) {
            if self.assembly.occurrence(record.occurrence).is_none() {
                continue;
            }
            if used.contains(&record.occurrence) {
                messages.push(format!(
                    "{} is no longer in {file}; it stays where it was, as this design uses it",
                    self.assembly.occurrence_name(record.occurrence)
                ));
            } else {
                dropped.insert(record.occurrence);
            }
        }
        self.assembly
            .occurrences
            .retain(|o| !dropped.contains(&o.uid));
        let unplaced: BTreeSet<ComponentUid> = tree
            .components
            .iter()
            .map(|t| t.component)
            .filter(|c| {
                self.assembly.exists(*c) && self.assembly.occurrences_of(*c).next().is_none()
            })
            .collect();
        if !unplaced.is_empty() {
            self.delete_components(unplaced);
        }
        let mut orphans = crate::joints::orphaned(self);
        if !orphans.is_empty() {
            orphans.extend(self.dependents(&orphans));
            self.remove_features(&orphans);
        }
        tree.components
            .retain(|t| self.assembly.exists(t.component) && self.entry(t.feature).is_some());
        tree.occurrences
            .retain(|t| self.assembly.occurrence(t.occurrence).is_some());
        tree.features.retain(|t| self.entry(t.feature).is_some());

        if let Some(l) = self
            .assembly
            .component_mut(made)
            .and_then(|c| c.link.as_mut())
        {
            l.digest = digest;
            l.looks = true;
            l.tree = Some(tree);
        }
        Ok(())
    }
}

/// Puts linked designs kept with their components, read again, into a
/// state (`update_links`, `update_link`); the lines they give.
pub(super) fn apply_tree_updates(
    state: &mut DocState,
    updates: Vec<TreeUpdate>,
) -> Result<Vec<String>, ModelError> {
    let mut messages = Vec::new();
    for update in updates {
        state.update_link_tree(update, &mut messages)?;
    }
    Ok(messages)
}

/// A linked design's record of what was made for its components
/// (mitcad#2209) after a copy of the linked component into `design`
/// (Save Component, the links inside the saved one): by the copies'
/// uids, leaving out what was not copied.
pub(super) fn copied_tree(
    tree: &LinkTree,
    source: &DocState,
    design: &DocState,
    copy: &ComponentCopy,
) -> LinkTree {
    let components: BTreeMap<ComponentUid, ComponentUid> = copy
        .occurrences
        .iter()
        .filter_map(|(from, to)| {
            Some((
                source.assembly.occurrence(*from)?.component,
                design.assembly.occurrence(*to)?.component,
            ))
        })
        .collect();
    LinkTree {
        components: tree
            .components
            .iter()
            .filter_map(|c| {
                Some(LinkedComponent {
                    component: *components.get(&c.component)?,
                    feature: *copy.features.get(&c.feature)?,
                    ..c.clone()
                })
            })
            .collect(),
        occurrences: tree
            .occurrences
            .iter()
            .filter_map(|o| {
                Some(LinkedOccurrence {
                    occurrence: *copy.occurrences.get(&o.occurrence)?,
                    ..o.clone()
                })
            })
            .collect(),
        features: tree
            .features
            .iter()
            .filter_map(|f| {
                Some(LinkedFeature {
                    feature: *copy.features.get(&f.feature)?,
                    ..f.clone()
                })
            })
            .collect(),
    }
}
