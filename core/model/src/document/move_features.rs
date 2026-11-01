// SPDX-License-Identifier: MIT
//! Move Features into a Component (mitcad#171): the features that made
//! bodies of a component (the root, where a design often starts) go into
//! new components placed in it at the identity, as if they had been made
//! there from the start. Nothing moves on screen: the features keep their
//! uids, order, parameters and the topological names of what they make,
//! and the references to them from elsewhere (links of sketches and
//! combines, joint origins, Isolate) take the new occurrence into their
//! occurrence paths.
//!
//! - **Everything** (the default): every modelled feature of the component
//!   (not the features that place occurrences, joints and assembly
//!   constraints) goes into one new component, grounded where it is, and
//!   the component keeps only components. A feature that makes a
//!   component (a new-component operation, Component from Bodies) goes too
//!   when it uses what goes; the component it made stays placed where it
//!   is.
//! - **Groups of bodies**: each group of bodies goes into a component of
//!   its own with the features that made them. A feature that changes
//!   bodies of different groups, or a body that stays (a combine, a cut
//!   through both, a sketch on a face of a body that stays), cannot belong
//!   to one of them: the plan lists such features, and the move is refused
//!   unless those bodies are kept together.
//! - **The history of a component made from bodies**: the features that
//!   made the bodies of a `component_from_bodies` feature go into the
//!   component it made, and the step goes.
//!
//! Which features go follows what the last recompute read and changed
//! (bodies set, removed or read by each feature), and the references for
//! features without a result (rolled back, suppressed, failed): a feature
//! goes with the features it refers to, the features that refer to it and
//! the features that work on the same bodies. Each move is one undo step,
//! refused when a feature that worked before would fail after it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::extrude_scope::{ComponentMove, MovedFeature};
use crate::features::{FeatureDef, JointOrigin, OccurrencePath, References, SketchPlane};
use crate::ids::{BodyUid, ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::path_component;
use crate::kernel::Kernel;
use crate::links::OccurrenceLink;
use crate::recompute::{ChangeHead, FeatureStatus, Read};
use crate::transform::Transform;

#[path = "move_destination.rs"]
mod destination;
pub use destination::{MoveDestination, MovedDestination};

/// What one new component is to get: bodies of the source component at
/// the timeline marker, and features besides those that made them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MoveGroup {
    pub bodies: Vec<BodyUid>,
    pub features: Vec<FeatureUid>,
}

/// A group as planned: its bodies and every feature that goes with them,
/// in timeline order.
#[derive(Debug, Clone, PartialEq)]
pub struct PlannedGroup {
    pub bodies: Vec<BodyUid>,
    pub features: Vec<FeatureUid>,
}

/// A feature that cannot go into one group alone: it belongs to several
/// groups, or works on bodies that would stay.
#[derive(Debug, Clone, PartialEq)]
pub struct MoveConflict {
    pub feature: FeatureUid,
    /// Indices of the groups it would belong to.
    pub groups: Vec<usize>,
    /// The bodies it works on that are in no group.
    pub staying: Vec<BodyUid>,
    pub message: String,
}

/// What a move would do.
#[derive(Debug, Clone, PartialEq)]
pub struct MovePlan {
    pub source: ComponentUid,
    /// Every modelled feature goes (no bodies or features were given).
    pub everything: bool,
    pub groups: Vec<PlannedGroup>,
    pub conflicts: Vec<MoveConflict>,
    /// The groups with the bodies the conflicts tie kept together; None
    /// without conflicts.
    pub together: Option<Vec<PlannedGroup>>,
    /// What no grouping solves: features that stay and use what would go.
    pub problems: Vec<String>,
}

/// A component a move made.
#[derive(Debug, Clone, PartialEq)]
pub struct MovedGroup {
    pub component: ComponentUid,
    pub occurrence: OccurrenceUid,
    pub bodies: Vec<BodyUid>,
    pub features: Vec<FeatureUid>,
}

/// Features that place occurrences or relate them: they stay in their
/// component, where the occurrences are.
fn stays(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::MoveOccurrence(_)
            | FeatureDef::CapturePosition(_)
            | FeatureDef::Joint(_)
            | FeatureDef::AsBuiltJoint(_)
            | FeatureDef::RigidGroup(_)
            | FeatureDef::ConstraintSet(_)
    )
}

/// Features whose occurrence paths name geometry: they follow it.
fn has_origins(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::Joint(_)
            | FeatureDef::AsBuiltJoint(_)
            | FeatureDef::ConstraintSet(_)
            | FeatureDef::BoundLength(_)
    )
}

/// The source component's features with what they refer to and work on.
struct Usage {
    source: ComponentUid,
    /// The features that can go, in timeline order.
    movable: Vec<FeatureUid>,
    /// The features that stay whatever goes (they place occurrences).
    fixed: Vec<FeatureUid>,
    refs: BTreeMap<FeatureUid, BTreeSet<FeatureUid>>,
    touched: BTreeMap<FeatureUid, BTreeSet<BodyUid>>,
    /// The source component's bodies at the marker.
    owned: BTreeSet<BodyUid>,
    /// Features that make components.
    makers: BTreeSet<FeatureUid>,
}

impl Usage {
    fn closure(&self, seed: impl IntoIterator<Item = FeatureUid>) -> BTreeSet<FeatureUid> {
        let movable: BTreeSet<FeatureUid> = self.movable.iter().copied().collect();
        let mut going: BTreeSet<FeatureUid> =
            seed.into_iter().filter(|f| movable.contains(f)).collect();
        loop {
            let before = going.len();
            // What they refer to.
            let referred: Vec<FeatureUid> = going
                .iter()
                .flat_map(|f| self.refs[f].iter().copied())
                .filter(|f| movable.contains(f))
                .collect();
            going.extend(referred);
            // What refers to them, and what works on their bodies.
            let bodies: BTreeSet<BodyUid> = going
                .iter()
                .flat_map(|f| self.touched[f].iter().copied())
                .collect();
            for f in &self.movable {
                if going.contains(f) {
                    continue;
                }
                if self.refs[f].iter().any(|r| going.contains(r))
                    || self.touched[f].iter().any(|b| bodies.contains(b))
                {
                    going.insert(*f);
                }
            }
            if going.len() == before {
                return going;
            }
        }
    }

    fn touched_by(&self, features: &BTreeSet<FeatureUid>) -> BTreeSet<BodyUid> {
        features
            .iter()
            .flat_map(|f| self.touched[f].iter().copied())
            .collect()
    }

    /// In timeline order.
    fn ordered(&self, features: &BTreeSet<FeatureUid>) -> Vec<FeatureUid> {
        self.movable
            .iter()
            .copied()
            .filter(|f| features.contains(f))
            .collect()
    }
}

/// One group's bodies and the features that go with them.
struct Closure {
    bodies: Vec<BodyUid>,
    features: BTreeSet<FeatureUid>,
}

impl<K: Kernel> Document<K> {
    /// What the features of `source` read and change, from the last
    /// recompute; `except` is left out (a component from bodies whose
    /// history moves).
    fn usage(&self, source: ComponentUid, except: Option<FeatureUid>) -> Usage {
        let state = &self.state;
        let mut known: BTreeSet<BodyUid> = self.result.owners.keys().copied().collect();
        for r in &self.result.results {
            if let Some(output) = &r.output {
                for change in output.changes.heads() {
                    if let ChangeHead::Set(uid, _) = change {
                        known.insert(*uid);
                    }
                }
            }
        }
        let mut usage = Usage {
            source,
            movable: Vec::new(),
            fixed: Vec::new(),
            refs: BTreeMap::new(),
            touched: BTreeMap::new(),
            owned: self
                .result
                .owners
                .iter()
                .filter(|(_, c)| **c == source)
                .map(|(b, _)| *b)
                .collect(),
            makers: BTreeSet::new(),
        };
        for entry in &state.features {
            if entry.component != source || Some(entry.uid) == except {
                continue;
            }
            if stays(&entry.def) {
                usage.fixed.push(entry.uid);
                continue;
            }
            let refs = entry.def.references().features;
            let mut touched = BTreeSet::new();
            match self.feature_result(entry.uid) {
                Some(r) if r.status == FeatureStatus::Ok && r.output.is_some() => {
                    let output = r.output.as_ref().expect("checked");
                    for change in output.changes.heads() {
                        match change {
                            ChangeHead::Set(uid, _) | ChangeHead::Remove(uid) => {
                                touched.insert(*uid);
                            }
                        }
                    }
                    for read in &output.reads {
                        if let Read::Body(uid, _) = read {
                            touched.insert(*uid);
                        }
                    }
                }
                // Not evaluated: the bodies of the features it names.
                _ => {
                    touched.extend(
                        known
                            .iter()
                            .filter(|b| b.feature == entry.uid || refs.contains(&b.feature))
                            .copied(),
                    );
                }
            }
            if entry.def.info().new_component() {
                usage.makers.insert(entry.uid);
            }
            usage.movable.push(entry.uid);
            usage.refs.insert(entry.uid, refs);
            usage.touched.insert(entry.uid, touched);
        }
        usage
    }

    /// What moving features of `source` into new components would do: one
    /// component per group (none given: every modelled feature into one).
    pub fn plan_move_to_components(
        &self,
        source: ComponentUid,
        groups: &[MoveGroup],
    ) -> Result<MovePlan, ModelError> {
        self.state.editable(source)?;
        let usage = self.usage(source, None);
        let everything = groups.is_empty();
        let name = |b: &BodyUid| self.body_name(*b);
        for group in groups {
            for body in &group.bodies {
                if !usage.owned.contains(body) {
                    return Err(invalid(format!(
                        "{} is not a body of {} at the timeline marker",
                        name(body),
                        self.state.assembly.name(source)
                    )));
                }
            }
            for feature in &group.features {
                let entry = self
                    .state
                    .entry(*feature)
                    .ok_or(ModelError::NoSuchFeature(*feature))?;
                if !usage.movable.contains(feature) {
                    return Err(invalid(format!(
                        "{} is not a modeled feature of {}",
                        entry.name,
                        self.state.assembly.name(source)
                    )));
                }
            }
            if group.bodies.is_empty() && group.features.is_empty() {
                return Err(invalid("a group of bodies to move is empty"));
            }
        }
        let asked: Vec<MoveGroup> = if everything {
            let features: Vec<FeatureUid> = usage
                .movable
                .iter()
                .copied()
                .filter(|f| !usage.makers.contains(f))
                .collect();
            if features.is_empty() {
                return Err(invalid(format!(
                    "{} has no modeled features to move",
                    self.state.assembly.name(source)
                )));
            }
            vec![MoveGroup {
                bodies: usage.owned.iter().copied().collect(),
                features,
            }]
        } else {
            groups.to_vec()
        };
        let (planned, conflicts) = self.plan_groups(&usage, &asked);
        let together = if conflicts.is_empty() {
            None
        } else {
            Some(self.keep_together(&usage, &asked))
        };
        let all: BTreeSet<FeatureUid> = planned
            .iter()
            .flat_map(|c| c.features.iter().copied())
            .collect();
        let problems = self.problems(&usage, &all);
        let groups = planned
            .into_iter()
            .map(|c| PlannedGroup {
                bodies: c.bodies,
                features: usage.ordered(&c.features),
            })
            .collect();
        Ok(MovePlan {
            source,
            everything,
            groups,
            conflicts,
            together,
            problems,
        })
    }

    /// Each group's features, and the features that tie groups or bodies
    /// that stay.
    fn plan_groups(
        &self,
        usage: &Usage,
        groups: &[MoveGroup],
    ) -> (Vec<Closure>, Vec<MoveConflict>) {
        let mut closures = Vec::with_capacity(groups.len());
        for group in groups {
            // Features given take the bodies they work on along.
            let given = usage.closure(group.features.iter().copied());
            let mut bodies: BTreeSet<BodyUid> = group.bodies.iter().copied().collect();
            bodies.extend(usage.touched_by(&given).intersection(&usage.owned).copied());
            let seed = usage
                .movable
                .iter()
                .copied()
                .filter(|f| usage.touched[f].iter().any(|b| bodies.contains(b)))
                .chain(given.iter().copied());
            let features = usage.closure(seed);
            closures.push(Closure {
                bodies: bodies.into_iter().collect(),
                features,
            });
        }
        let conflicts = self.conflicts(usage, &closures);
        (closures, conflicts)
    }

    /// The features that tie the groups to each other or to bodies that
    /// stay: a feature that works on bodies of several of them, one that
    /// works on a body of another (a sketch on its face) and is used for a
    /// group's body, and one that is used for bodies of several groups (a
    /// sketch two of them use). When none of them is to blame alone, every
    /// feature of a group that works on another's bodies.
    fn conflicts(&self, usage: &Usage, closures: &[Closure]) -> Vec<MoveConflict> {
        let in_any: BTreeSet<FeatureUid> = closures
            .iter()
            .flat_map(|c| c.features.iter().copied())
            .collect();
        let group_of = |b: &BodyUid| closures.iter().position(|c| c.bodies.contains(b));
        // Where the source component's bodies a feature works on would go:
        // groups, and the bodies that stay.
        let goes = |f: &FeatureUid| -> (BTreeSet<usize>, BTreeSet<BodyUid>) {
            let mut groups = BTreeSet::new();
            let mut staying = BTreeSet::new();
            for b in usage.touched[f].intersection(&usage.owned) {
                match group_of(b) {
                    Some(g) => {
                        groups.insert(g);
                    }
                    None => {
                        staying.insert(*b);
                    }
                }
            }
            (groups, staying)
        };
        let ties = |groups: &BTreeSet<usize>, staying: &BTreeSet<BodyUid>| {
            groups.len() + usize::from(!staying.is_empty()) > 1
        };
        // Whether any group reaches what is not its own.
        let any = closures.iter().enumerate().any(|(i, c)| {
            c.features.iter().any(|f| {
                let (mut groups, staying) = goes(f);
                groups.insert(i);
                ties(&groups, &staying)
                    || closures
                        .iter()
                        .enumerate()
                        .any(|(j, d)| j != i && d.features.contains(f))
            })
        });
        if !any {
            return Vec::new();
        }
        let mut found: BTreeMap<FeatureUid, (BTreeSet<usize>, BTreeSet<BodyUid>)> = BTreeMap::new();
        for f in &in_any {
            let (groups, staying) = goes(f);
            if ties(&groups, &staying) {
                found.insert(*f, (groups, staying));
                continue;
            }
            // The groups of the features that use it.
            let users: Vec<BTreeSet<usize>> = in_any
                .iter()
                .filter(|x| usage.refs[*x].contains(f))
                .map(&goes)
                .filter(|(g, s)| g.len() == 1 && s.is_empty())
                .map(|(g, _)| g)
                .collect();
            let mut all = groups;
            for g in &users {
                all.extend(g.iter().copied());
            }
            if ties(&all, &staying) {
                found.insert(*f, (all, staying));
            }
        }
        if found.is_empty() {
            for (i, c) in closures.iter().enumerate() {
                for f in &c.features {
                    let (mut groups, staying) = goes(f);
                    groups.insert(i);
                    if ties(&groups, &staying) {
                        let entry = found.entry(*f).or_default();
                        entry.0.extend(groups);
                        entry.1.extend(staying);
                    }
                }
            }
        }
        let source = self.state.assembly.name(usage.source);
        found
            .into_iter()
            .map(|(feature, (groups, staying))| {
                let name = self
                    .state
                    .entry(feature)
                    .map_or_else(|| feature.to_string(), |f| f.name.clone());
                let bodies: Vec<String> = groups
                    .iter()
                    .flat_map(|g| closures[*g].bodies.iter())
                    .map(|b| self.body_name(*b))
                    .collect();
                let staying_names: Vec<String> =
                    staying.iter().map(|b| self.body_name(*b)).collect();
                let message = if staying.is_empty() {
                    format!(
                        "{name} ties {}, which would go into different components",
                        bodies.join(", ")
                    )
                } else if bodies.is_empty() {
                    format!(
                        "{name} works on {}, which stays in {source}",
                        staying_names.join(", ")
                    )
                } else {
                    format!(
                        "{name} ties {} to {}, which stays in {source}",
                        bodies.join(", "),
                        staying_names.join(", ")
                    )
                };
                MoveConflict {
                    feature,
                    groups: groups.into_iter().collect(),
                    staying: staying.into_iter().collect(),
                    message,
                }
            })
            .collect()
    }

    /// The groups merged until no feature ties two of them, the bodies
    /// that would stay taken into the group that works on them.
    fn keep_together(&self, usage: &Usage, groups: &[MoveGroup]) -> Vec<PlannedGroup> {
        let mut groups = groups.to_vec();
        // Each round merges at least two groups or takes a body in.
        for _ in 0..=usage.owned.len() + groups.len() {
            let (closures, conflicts) = self.plan_groups(usage, &groups);
            if conflicts.is_empty() {
                return closures
                    .into_iter()
                    .map(|c| PlannedGroup {
                        bodies: c.bodies,
                        features: usage.ordered(&c.features),
                    })
                    .collect();
            }
            let mut parent: Vec<usize> = (0..groups.len()).collect();
            fn root(parent: &mut [usize], mut i: usize) -> usize {
                while parent[i] != i {
                    parent[i] = parent[parent[i]];
                    i = parent[i];
                }
                i
            }
            let mut taken: BTreeMap<usize, BTreeSet<BodyUid>> = BTreeMap::new();
            for conflict in &conflicts {
                let first = conflict.groups[0];
                for g in &conflict.groups[1..] {
                    let (a, b) = (root(&mut parent, first), root(&mut parent, *g));
                    if a != b {
                        parent[b] = a;
                    }
                }
                taken
                    .entry(first)
                    .or_default()
                    .extend(conflict.staying.iter().copied());
            }
            let mut merged: BTreeMap<usize, MoveGroup> = BTreeMap::new();
            for (i, group) in groups.iter().enumerate() {
                let r = root(&mut parent, i);
                let target = merged.entry(r).or_default();
                target.bodies.extend(group.bodies.iter().copied());
                target.features.extend(group.features.iter().copied());
            }
            for (g, bodies) in taken {
                let r = root(&mut parent, g);
                let target = merged.entry(r).or_default();
                target.bodies.extend(bodies);
            }
            groups = merged
                .into_values()
                .map(|mut g| {
                    let mut seen = BTreeSet::new();
                    g.bodies.retain(|b| seen.insert(*b));
                    g
                })
                .collect();
        }
        // Everything in one group has nothing left to tie.
        let all = MoveGroup {
            bodies: usage.owned.iter().copied().collect(),
            features: groups
                .iter()
                .flat_map(|g| g.features.iter().copied())
                .collect(),
        };
        let (closures, _) = self.plan_groups(usage, &[all]);
        closures
            .into_iter()
            .map(|c| PlannedGroup {
                bodies: c.bodies,
                features: usage.ordered(&c.features),
            })
            .collect()
    }

    /// Features that stay and use geometry that would go, which no
    /// occurrence path can follow (a move of occurrences about an edge).
    fn problems(&self, usage: &Usage, going: &BTreeSet<FeatureUid>) -> Vec<String> {
        let source = self.state.assembly.name(usage.source);
        usage
            .fixed
            .iter()
            .filter_map(|uid| self.state.entry(*uid))
            .filter(|entry| !has_origins(&entry.def))
            .filter(|entry| {
                entry
                    .def
                    .references()
                    .features
                    .iter()
                    .any(|f| going.contains(f))
            })
            .map(|entry| {
                format!(
                    "{} stays in {source} and uses geometry that would go into a component; \
                     edit it first",
                    entry.name
                )
            })
            .collect()
    }

    /// Moves features of `source` into new components placed in it at the
    /// identity, one per group (none given: every modelled feature into
    /// one, grounded). With `together`, bodies that a feature ties are
    /// kept in one component; without it such a feature refuses the move.
    /// `names` name the components (default: a single body's name, else
    /// the next free `Component<n>`). One undo step.
    pub fn move_to_components(
        &mut self,
        source: ComponentUid,
        groups: &[MoveGroup],
        names: &[String],
        together: bool,
    ) -> Result<Vec<MovedGroup>, ModelError> {
        self.ensure_results()?;
        let plan = self.plan_move_to_components(source, groups)?;
        if let Some(problem) = plan.problems.first() {
            return Err(invalid(problem.clone()));
        }
        let groups = match (&plan.together, together) {
            (None, _) => plan.groups.clone(),
            (Some(merged), true) => merged.clone(),
            (Some(_), false) => {
                let messages: Vec<&str> =
                    plan.conflicts.iter().map(|c| c.message.as_str()).collect();
                return Err(invalid(format!(
                    "{}; keep those bodies together, or change the features first",
                    messages.join("; ")
                )));
            }
        };
        if !names.is_empty() && names.len() != groups.len() {
            return Err(invalid(format!(
                "{} names for {} components",
                names.len(),
                groups.len()
            )));
        }
        let body_names: Vec<Vec<String>> = groups
            .iter()
            .map(|g| g.bodies.iter().map(|b| self.body_name(*b)).collect())
            .collect();
        let everything = plan.everything;
        let label_source = self.state.assembly.name(source);
        // Whether the source still has bodies after each group went.
        let mut staying: BTreeSet<BodyUid> = self
            .result
            .owners
            .iter()
            .filter(|(_, c)| **c == source)
            .map(|(b, _)| *b)
            .collect();
        let keeps: Vec<bool> = groups
            .iter()
            .map(|g| {
                staying.retain(|b| !g.bodies.contains(b));
                !staying.is_empty()
            })
            .collect();
        self.apply_move(|state| {
            let mut moved = Vec::with_capacity(groups.len());
            for (i, group) in groups.iter().enumerate() {
                let name = match names.get(i) {
                    Some(name) => {
                        state.check_component_name(name, ComponentUid(u32::MAX))?;
                        name.clone()
                    }
                    None if !everything && body_names[i].len() == 1 => {
                        state.free_component_name(&body_names[i][0])
                    }
                    None => state.assembly.unused_name("Component"),
                };
                let made = state.assembly.create(Some(&name), None);
                // The link settings of where the features were (mitcad#220).
                let policy = state.assembly.policy(source).for_moved_content();
                state.assembly.set_policy(made, policy);
                let occurrence = state.assembly.place(made, source, Transform::IDENTITY);
                // The root's modelled part stays put, as the root did.
                state
                    .assembly
                    .occurrence_mut(occurrence)
                    .expect("placed")
                    .grounded = everything;
                set_component(state, &group.features, made);
                follow_cut_scopes(
                    state,
                    &group.features,
                    &ComponentMove {
                        from: source,
                        to: made,
                        path: &|p: &OccurrencePath| {
                            let mut p = p.clone();
                            p.0.push(occurrence);
                            p
                        },
                        bodies: &group.bodies,
                        keeps: keeps[i],
                    },
                );
                moved.push(MovedGroup {
                    component: made,
                    occurrence,
                    bodies: group.bodies.clone(),
                    features: group.features.clone(),
                });
            }
            for m in &moved {
                let features: BTreeSet<FeatureUid> = m.features.iter().copied().collect();
                follow_references(
                    state,
                    source,
                    m.occurrence,
                    &features,
                    &m.bodies,
                    everything,
                );
            }
            let label = match moved.as_slice() {
                [one] => format!(
                    "Move Features of {label_source} into {}",
                    state.assembly.name(one.component)
                ),
                _ => format!("Move Features of {label_source} into Components"),
            };
            Ok((label, moved))
        })
    }

    /// Moves the features that made the bodies of a `component_from_bodies`
    /// feature into the component it made, which then has their history,
    /// and removes the feature. Refused when one of them also works on a
    /// body that stays. Returns the component and the moved features.
    pub fn move_history_into_component(
        &mut self,
        feature: FeatureUid,
    ) -> Result<(ComponentUid, Vec<FeatureUid>), ModelError> {
        self.ensure_results()?;
        let entry = self
            .state
            .entry(feature)
            .ok_or(ModelError::NoSuchFeature(feature))?
            .clone();
        let FeatureDef::ComponentFromBodies(def) = &entry.def else {
            return Err(invalid(format!(
                "{} is not a component from bodies",
                entry.name
            )));
        };
        let source = entry.component;
        self.state.editable(source)?;
        let a = &self.state.assembly;
        let made = a
            .created_by(feature)
            .ok_or_else(|| invalid(format!("{} made no component", entry.name)))?;
        let occurrence = a
            .occurrences_of(made)
            .find(|o| o.parent == source)
            .map(|o| o.uid)
            .ok_or_else(|| {
                invalid(format!(
                    "{} is not placed in {}",
                    a.name(made),
                    a.name(source)
                ))
            })?;
        let dependents: Vec<String> = self
            .state
            .features
            .iter()
            .filter(|f| f.def.references().features.contains(&feature))
            .map(|f| f.name.clone())
            .collect();
        if !dependents.is_empty() {
            return Err(invalid(format!(
                "{} is used by {}",
                entry.name,
                dependents.join(", ")
            )));
        }
        let usage = self.usage(source, Some(feature));
        let bodies: BTreeSet<BodyUid> = def.bodies.iter().copied().collect();
        let seed: Vec<FeatureUid> = usage
            .movable
            .iter()
            .copied()
            .filter(|f| usage.touched[f].iter().any(|b| bodies.contains(b)))
            .collect();
        let going = usage.closure(seed);
        if going.is_empty() {
            return Err(invalid(format!(
                "no features of {} made the bodies of {}",
                a.name(source),
                entry.name
            )));
        }
        let staying: Vec<String> = usage
            .touched_by(&going)
            .intersection(&usage.owned)
            .map(|b| self.body_name(*b))
            .collect();
        if !staying.is_empty() {
            let names: Vec<String> = usage
                .ordered(&going)
                .iter()
                .filter(|f| usage.touched[f].iter().any(|b| usage.owned.contains(b)))
                .filter_map(|f| self.state.entry(*f).map(|e| e.name.clone()))
                .collect();
            return Err(invalid(format!(
                "{} also work on {}, which stays in {}",
                names.join(", "),
                staying.join(", "),
                a.name(source)
            )));
        }
        if let Some(problem) = self.problems(&usage, &going).first() {
            return Err(invalid(problem.clone()));
        }
        let features = usage.ordered(&going);
        let moved_bodies: Vec<BodyUid> = def.bodies.clone();
        let label = format!("Move History into {}", a.name(made));
        let keeps = !usage.owned.is_empty();
        self.apply_move(|state| {
            set_component(state, &features, made);
            follow_cut_scopes(
                state,
                &features,
                &ComponentMove {
                    from: source,
                    to: made,
                    path: &|p: &OccurrencePath| {
                        let mut p = p.clone();
                        p.0.push(occurrence);
                        p
                    },
                    bodies: &moved_bodies,
                    keeps,
                },
            );
            follow_references(state, source, occurrence, &going, &moved_bodies, false);
            state.remove_features(&BTreeSet::from([feature]));
            Ok((label, (made, features.clone())))
        })
    }

    /// The results of the definition: features the last recompute did not
    /// see (a loaded design) are evaluated first.
    fn ensure_results(&mut self) -> Result<(), ModelError> {
        let same = self.result.results.len() == self.state.features.len()
            && self
                .result
                .results
                .iter()
                .zip(&self.state.features)
                .all(|(r, f)| r.uid == f.uid);
        if !same {
            self.try_recompute()?;
        }
        Ok(())
    }

    /// Runs a move as one undo step, refused when a feature that worked
    /// fails after it or the moved definition does not check.
    fn apply_move<T>(
        &mut self,
        edit: impl FnOnce(&mut DocState) -> Result<(String, T), ModelError>,
    ) -> Result<T, ModelError> {
        let checked: BTreeSet<FeatureUid> = self
            .state
            .features
            .iter()
            .enumerate()
            .filter(|(i, entry)| self.state.check_feature(*i, entry).is_ok())
            .map(|(_, entry)| entry.uid)
            .collect();
        let mut next = self.state.clone();
        let (label, value) = edit(&mut next)?;
        // The destination's policy, and no new cross-component links it or
        // the other side refuses (mitcad#220), before anything changes.
        self.check_policy(&next)?;
        for (i, entry) in next.features.iter().enumerate() {
            if checked.contains(&entry.uid) {
                next.check_feature(i, entry)
                    .map_err(|e| invalid(format!("{} would no longer work: {e}", entry.name)))?;
            }
        }
        next.normalize_groups();
        next.follow_sketch_use(&self.state);
        let result = self.compute(&next)?;
        for r in &result.results {
            let worked = self
                .result
                .result(r.uid)
                .is_some_and(|old| old.status.is_ok());
            if let (true, FeatureStatus::Failed(message)) = (worked, &r.status) {
                let name = next
                    .entry(r.uid)
                    .map_or_else(|| r.uid.to_string(), |f| f.name.clone());
                return Err(invalid(format!("{name} would fail: {message}")));
            }
        }
        self.commit(label, next);
        self.set_result(result);
        self.name_new_bodies();
        Ok(value)
    }

    /// Whether the design may be a hybrid assembly without asking: a
    /// component made where modelled bodies are does not offer to move
    /// them first ("don't ask again for this design").
    pub fn hybrid_assembly(&self) -> bool {
        self.state.hybrid_assembly
    }

    pub fn set_hybrid_assembly(&mut self, allowed: bool) -> Result<(), ModelError> {
        if self.state.hybrid_assembly == allowed {
            return Ok(());
        }
        self.apply_with(|state| {
            state.hybrid_assembly = allowed;
            let label = if allowed {
                "Allow Hybrid Assembly"
            } else {
                "Ask Before a Hybrid Assembly"
            };
            Ok((label.to_owned(), (), false))
        })
    }
}

fn set_component(state: &mut DocState, features: &[FeatureUid], component: ComponentUid) {
    for f in &mut state.features {
        if features.contains(&f.uid) {
            Arc::make_mut(f).component = component;
        }
    }
}

/// The extrusions' cut scopes follow `moved`, features of `m.from` that
/// went into `m.to` (mitcad#859): a moved extrusion's context and its
/// targets in `m.from`, and every scope's body targets of the bodies that
/// went. Saved scopes of extrusions that do not cut follow too, so that
/// the extrusion can be made a cut again. See `CutScope::follow_move`.
fn follow_cut_scopes(state: &mut DocState, moved: &[FeatureUid], m: &ComponentMove<'_>) {
    for entry in &mut state.features {
        let FeatureDef::Extrude(extrude) = &entry.def else {
            continue;
        };
        let Some(scope) = &extrude.cut_scope else {
            continue;
        };
        let feature = if moved.contains(&entry.uid) {
            MovedFeature::Moved
        } else if entry.component == m.from {
            MovedFeature::StaysInSource
        } else {
            MovedFeature::Elsewhere
        };
        let mut scope = scope.clone();
        if scope.follow_move(m, feature)
            && let FeatureDef::Extrude(extrude) = &mut Arc::make_mut(entry).def
        {
            extrude.cut_scope = Some(scope);
        }
    }
}

/// The occurrence paths that lead to `source` and name geometry of the
/// moved features take the new occurrence: sketch planes and projections
/// of other components, combines' tools, joint origins and Isolate. Links
/// of the moved features to geometry elsewhere are now seen from the new
/// occurrence. In a move of everything, a joint's side that is `source`
/// itself becomes the new occurrence.
fn follow_references(
    state: &mut DocState,
    source: ComponentUid,
    occurrence: OccurrenceUid,
    moved: &BTreeSet<FeatureUid>,
    bodies: &[BodyUid],
    everything: bool,
) {
    let assembly = state.assembly.clone();
    let ends_in_source = |from: ComponentUid, path: &[OccurrenceUid]| {
        path_component(&assembly, from, path) == Ok(source)
    };
    let follow = |link: &mut OccurrenceLink, names: &BTreeSet<FeatureUid>, own: bool| {
        if names.iter().any(|f| moved.contains(f))
            && ends_in_source(ComponentUid::ROOT, &link.source.0)
        {
            link.source.0.push(occurrence);
        }
        if own && ends_in_source(ComponentUid::ROOT, &link.target.0) {
            link.target.0.push(occurrence);
        }
    };
    for f in &mut state.features {
        let own = moved.contains(&f.uid);
        let component = f.component;
        let changes = match &f.def {
            FeatureDef::Sketch(s) => {
                s.plane_link.is_some() || s.projections.iter().any(|p| p.link.is_some())
            }
            FeatureDef::Combine(c) => c.has_links(),
            FeatureDef::Sweep(s) => !s.body_links.is_empty(),
            FeatureDef::SplitBody(s) => !s.tool_links.is_empty(),
            FeatureDef::Extrude(x) => !x.object_links.is_empty(),
            FeatureDef::Revolve(r) => !r.object_links.is_empty(),
            FeatureDef::ConstructionPlane(p) => !p.links.is_empty() || !p.datum_links.is_empty(),
            FeatureDef::ConstructionAxis(a) => !a.links.is_empty() || !a.datum_links.is_empty(),
            FeatureDef::ConstructionPoint(p) => !p.links.is_empty() || !p.datum_links.is_empty(),
            FeatureDef::Derive(d) => !d.links.is_empty(),
            FeatureDef::BodyTransfer(_) => true,
            // A seat on another component's shaft (mitcad#578).
            FeatureDef::Pulley(_) | FeatureDef::Gear(_) | FeatureDef::PrintableBearing(_) => {
                f.def.seat_link().is_some()
            }
            def => has_origins(def),
        };
        if !changes {
            continue;
        }
        let mut def = f.def.clone();
        match &mut def {
            FeatureDef::Sketch(sketch) => {
                let plane = plane_features(&sketch.plane);
                if let Some(link) = sketch.plane_link.as_mut() {
                    follow(link, &plane, own);
                }
                for projection in &mut sketch.projections {
                    let mut names = crate::features::sketch::topo_features(&projection.source);
                    if let Some(body) = projection.body {
                        names.insert(body.feature);
                    }
                    if let Some(link) = projection.link.as_mut() {
                        follow(link, &names, own);
                    }
                }
            }
            FeatureDef::Combine(combine) => {
                for (tool, link) in combine.links_mut() {
                    follow(link, &BTreeSet::from([tool.feature]), own);
                }
            }
            FeatureDef::Sweep(sweep) => {
                for (body, link) in &mut sweep.body_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::SplitBody(split) => {
                for (body, link) in &mut split.tool_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Extrude(extrude) => {
                for (body, link) in &mut extrude.object_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Revolve(revolve) => {
                for (body, link) in &mut revolve.object_links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::ConstructionPlane(plane) => {
                for (body, link) in &mut plane.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut plane.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::ConstructionAxis(axis) => {
                for (body, link) in &mut axis.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut axis.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::ConstructionPoint(point) => {
                for (body, link) in &mut point.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
                for (datum, link) in &mut point.datum_links {
                    follow(link, &BTreeSet::from([*datum]), own);
                }
            }
            FeatureDef::Derive(derive) => {
                for (body, link) in &mut derive.links {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            // A body transfer's source and destination (mitcad#465).
            FeatureDef::BodyTransfer(transfer) => {
                let names = transfer.bodies.iter().map(|b| b.feature).collect();
                follow(&mut transfer.link, &names, own);
            }
            // A seat on another component's shaft (mitcad#578).
            seated @ (FeatureDef::Pulley(_)
            | FeatureDef::Gear(_)
            | FeatureDef::PrintableBearing(_)) => {
                if let Some((body, link)) = seated.seat_link_mut() {
                    follow(link, &BTreeSet::from([body.feature]), own);
                }
            }
            FeatureDef::Joint(joint) => {
                for origin in [&mut joint.a, &mut joint.b] {
                    follow_origin(origin, component, &ends_in_source, moved, occurrence);
                }
            }
            // BUILD's bound lengths (mitcad#375).
            FeatureDef::BoundLength(bound) => {
                let [mut from, mut to] = bound.origins();
                for origin in [&mut from, &mut to] {
                    follow_origin(origin, component, &ends_in_source, moved, occurrence);
                }
                bound.from.occurrence = from.occurrence;
                bound.from.geometry = from.geometry;
                bound.to.occurrence = to.occurrence;
                bound.to.geometry = to.geometry;
            }
            FeatureDef::AsBuiltJoint(joint) => {
                if let Some(origin) = joint.origin.as_mut() {
                    follow_origin(origin, component, &ends_in_source, moved, occurrence);
                }
                if everything {
                    for side in [&mut joint.a, &mut joint.b] {
                        if ends_in_source(component, &side.0) {
                            side.0.push(occurrence);
                        }
                    }
                }
            }
            FeatureDef::ConstraintSet(set) => {
                for row in &mut set.rows {
                    for origin in [&mut row.a, &mut row.b]
                        .into_iter()
                        .chain(row.a2.as_mut())
                        .chain(row.b2.as_mut())
                    {
                        follow_origin(origin, component, &ends_in_source, moved, occurrence);
                    }
                }
            }
            _ => {}
        }
        if def != f.def {
            Arc::make_mut(f).def = def;
        }
    }
    // Isolate names bodies where an occurrence shows them.
    let isolated: Vec<_> = state
        .display
        .isolated
        .iter()
        .map(|item| {
            let mut item = item.clone();
            if let Some(body) = item.body
                && bodies.contains(&body)
            {
                let path: Option<Vec<OccurrenceUid>> = if item.occurrence.is_empty() {
                    Some(Vec::new())
                } else {
                    item.occurrence.split('/').map(|p| p.parse().ok()).collect()
                };
                if let Some(mut path) = path
                    && ends_in_source(ComponentUid::ROOT, &path)
                {
                    path.push(occurrence);
                    item.occurrence = OccurrencePath(path).to_string();
                }
            }
            item
        })
        .collect();
    state.display.isolated = isolated;
}

fn follow_origin(
    origin: &mut JointOrigin,
    component: ComponentUid,
    ends_in_source: &dyn Fn(ComponentUid, &[OccurrenceUid]) -> bool,
    moved: &BTreeSet<FeatureUid>,
    occurrence: OccurrenceUid,
) {
    let mut references = References::default();
    origin.geometry.add_to(&mut references);
    if references.features.iter().any(|f| moved.contains(f))
        && ends_in_source(component, &origin.occurrence.0)
    {
        origin.occurrence.0.push(occurrence);
    }
}

/// The features a sketch plane names.
fn plane_features(plane: &SketchPlane) -> BTreeSet<FeatureUid> {
    let mut references = References::default();
    match plane {
        SketchPlane::Face { face, body } => {
            references.features.extend(face.features());
            if let Some(body) = body {
                references.body(*body);
            }
        }
        SketchPlane::Construction(uid) => {
            references.features.insert(*uid);
        }
        SketchPlane::Xy | SketchPlane::Xz | SketchPlane::Yz => {}
    }
    references.features
}
