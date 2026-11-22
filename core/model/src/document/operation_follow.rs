// SPDX-License-Identifier: MIT
//! Features that follow a feature whose operation an edit changes
//! (mitcad#2185). An extrude edited from Join (or Cut, Intersect) to New
//! Body or New Component makes bodies of its own: the later features that
//! work only on the geometry it makes (a fillet on its edges, a fillet on
//! that fillet's faces) go with that geometry. Their body references are
//! rewritten to the body that now carries it, and, for New Component, they
//! move into the component the feature makes, as Move to Component takes
//! dependents along. The reverse edit (New Component or New Body back to
//! Join) takes them back to the body the join now merges into, in the
//! feature's own component, so that the component it made can go. New Body
//! and New Component edited into each other move them between the
//! components, their bodies unchanged.
//!
//! A later feature follows when it refers to the edited feature or to a
//! feature that already followed, and nothing else once its references to
//! the bodies the feature changed before the edit are set aside: a fillet
//! of the old body on an edge named after the old body's own faces, or a
//! feature that names only the old body (a Scale of it), stays and reports
//! as before. Joints and the other features that place or relate
//! occurrences stay in their component, as Move to Component leaves them.
//! Which body a feature goes to is decided by
//! [`Document::operation_follow`] from a recompute up to the edited
//! feature: the one of the bodies the edited feature now sets on which the
//! feature's names of the edited feature's faces resolve. A name that
//! resolves on none (an edge that existed only by the join) keeps the
//! feature where it was, with its error. The edit and the moves are one
//! undo step.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde_json::Value;

use super::copy::{PARAM, TEXT_FIELDS};
use super::{DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::recompute::{ChangeHead, FeatureResult};
use crate::topo::TopoName;

/// What an edit that changes a feature's operation takes along: the bodies
/// the feature changed before the edit (the ones the later features name),
/// and the body each later feature that refers to the feature would go to
/// by its own names.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OperationFollow {
    pub old_bodies: BTreeSet<BodyUid>,
    pub targets: BTreeMap<FeatureUid, BodyUid>,
}

impl DocState {
    /// Moves the later features that work only on the geometry `edited`
    /// makes from `from` (the component its bodies were in before the
    /// edit) to `to`, their references to the plan's old bodies set to the
    /// body they follow (see the module comment). Returns the features
    /// that followed, in timeline order.
    pub fn follow_operation(
        &mut self,
        edited: FeatureUid,
        from: ComponentUid,
        to: ComponentUid,
        plan: &OperationFollow,
    ) -> Vec<FeatureUid> {
        let Some(position) = self.position(edited) else {
            return Vec::new();
        };
        let stand_in = BodyUid::new(edited, 0);
        let mut follows = BTreeSet::from([edited]);
        let mut bodies: BTreeMap<FeatureUid, BodyUid> = BTreeMap::new();
        let mut moved = Vec::new();
        for i in position + 1..self.features.len() {
            let entry = self.features[i].clone();
            // Features that place or relate occurrences stay with them, as
            // Move to Component leaves them.
            if entry.component != from || super::move_features::stays(&entry.def) {
                continue;
            }
            let Some(target) = follow_target(&entry, &follows, &bodies, plan, stand_in) else {
                continue;
            };
            let Ok(def) = retarget_bodies(&entry.def, &plan.old_bodies, target) else {
                continue;
            };
            self.features[i] = Arc::new(FeatureEntry {
                def,
                component: to,
                ..(*entry).clone()
            });
            follows.insert(entry.uid);
            bodies.insert(entry.uid, target);
            moved.push(entry.uid);
        }
        moved
    }
}

/// The body a later feature goes to, or None when it stays: it must refer
/// to what follows, and to nothing else once its old bodies stand for the
/// edited feature's; a feature that names features that followed goes to
/// their body, another to the plan's.
fn follow_target(
    entry: &FeatureEntry,
    follows: &BTreeSet<FeatureUid>,
    bodies: &BTreeMap<FeatureUid, BodyUid>,
    plan: &OperationFollow,
    stand_in: BodyUid,
) -> Option<BodyUid> {
    let before = entry.def.info().references().features;
    if before.is_disjoint(follows) {
        return None;
    }
    let probe = retarget_bodies(&entry.def, &plan.old_bodies, stand_in).ok()?;
    if !probe.info().references().features.is_subset(follows) {
        return None;
    }
    let named: BTreeSet<BodyUid> = before
        .iter()
        .filter_map(|f| bodies.get(f))
        .copied()
        .collect();
    let mut named = named.into_iter();
    match (named.next(), named.next()) {
        (None, _) => plan.targets.get(&entry.uid).copied(),
        (Some(body), None) => Some(body),
        // Features that followed to different bodies: it stays.
        (Some(_), Some(_)) => None,
    }
}

/// The definition's JSON form with its parameters as markers, and the
/// parameters by marker (as copies of definitions write it).
fn marked_json(def: &FeatureDef) -> Result<(Value, Vec<crate::parameters::ParamId>), ModelError> {
    let mut slots = Vec::new();
    let marked = def
        .map_params(&mut |_, id: &crate::parameters::ParamId| {
            slots.push(*id);
            Ok::<_, ()>(format!("{PARAM}{}", slots.len() - 1))
        })
        .expect("never fails");
    let value = serde_json::to_value(&marked).map_err(|e| invalid(e.to_string()))?;
    Ok((value, slots))
}

/// `def` with every reference to a body of `old` set to `body`; the same
/// definition when it names none of them.
pub(crate) fn retarget_bodies(
    def: &FeatureDef,
    old: &BTreeSet<BodyUid>,
    body: BodyUid,
) -> Result<FeatureDef, ModelError> {
    let (mut value, slots) = marked_json(def)?;
    let new = body.to_string();
    if !replace_bodies(&mut value, old, &new) {
        return Ok(def.clone());
    }
    let written: FeatureDef<String> =
        serde_json::from_value(value).map_err(|e| invalid(format!("retarget: {e}")))?;
    written.map_params(&mut |slot, marker: &String| {
        marker
            .strip_prefix(PARAM)
            .and_then(|i| i.parse::<usize>().ok())
            .and_then(|i| slots.get(i).copied())
            .ok_or_else(|| invalid(format!("retarget: {slot} lost its parameter")))
    })
}

/// Replaces the body ids of `old` in the JSON form; whether any was.
fn replace_bodies(value: &mut Value, old: &BTreeSet<BodyUid>, new: &str) -> bool {
    match value {
        Value::String(s) => {
            if s.parse::<BodyUid>().is_ok_and(|b| old.contains(&b)) {
                *s = new.to_owned();
                true
            } else {
                false
            }
        }
        Value::Array(items) => items
            .iter_mut()
            .fold(false, |any, item| replace_bodies(item, old, new) | any),
        Value::Object(fields) => fields
            .iter_mut()
            .filter(|(key, _)| !TEXT_FIELDS.contains(&key.as_str()))
            .fold(false, |any, (_, item)| replace_bodies(item, old, new) | any),
        _ => false,
    }
}

/// The topological names in a definition that name only `feature`'s faces.
fn own_names(def: &FeatureDef, feature: FeatureUid) -> Vec<TopoName> {
    let Ok((value, _)) = marked_json(def) else {
        return Vec::new();
    };
    let mut names = Vec::new();
    collect_names(&value, feature, &mut names);
    names
}

fn collect_names(value: &Value, feature: FeatureUid, names: &mut Vec<TopoName>) {
    match value {
        Value::String(s) if !s.starts_with(PARAM) => {
            let Ok(name) = s.parse::<TopoName>() else {
                return;
            };
            let features = match &name {
                TopoName::Face(face) => face.features(),
                TopoName::Edge(edge) => edge.features(),
                TopoName::Vertex(vertex) => {
                    vertex.faces().iter().flat_map(|f| f.features()).collect()
                }
            };
            if features.len() == 1 && features.contains(&feature) {
                names.push(name);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_names(item, feature, names);
            }
        }
        Value::Object(fields) => {
            for (key, item) in fields {
                if !TEXT_FIELDS.contains(&key.as_str()) {
                    collect_names(item, feature, names);
                }
            }
        }
        _ => {}
    }
}

/// The bodies a feature's result sets, in its order; none without a
/// result that succeeded.
fn set_bodies<S>(result: Option<&FeatureResult<S>>) -> Vec<BodyUid> {
    result
        .filter(|r| r.status.is_ok())
        .and_then(|r| r.output.as_ref())
        .map(|output| {
            output
                .changes
                .heads()
                .iter()
                .filter_map(|head| match head {
                    ChangeHead::Set(body, _) => Some(*body),
                    ChangeHead::Remove(_) => None,
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether a name resolves on a shape: an edge or a face found there (a
/// kernel that cannot look faces up, and vertices, are taken as found).
fn resolves<K: Kernel>(kernel: &K, shape: &K::Shape, name: &TopoName) -> bool {
    match name {
        TopoName::Edge(edge) => kernel.count_edges(shape, edge) > 0,
        TopoName::Face(face) => !matches!(kernel.count_faces(shape, face), Ok(0)),
        TopoName::Vertex(_) => true,
    }
}

impl<K: Kernel> Document<K> {
    /// What an edit of `uid` to `def` takes along (see the module
    /// comment): None when the edit keeps the feature's operation kind (it
    /// joins, cuts or intersects before and after, or makes bodies in the
    /// same component), when no later feature refers to it, or when the
    /// feature does not build with the new definition. Evaluates the
    /// edited feature with the features before it (the cache keeps them),
    /// not the features after it.
    pub(crate) fn operation_follow(
        &mut self,
        uid: FeatureUid,
        def: &FeatureDef<ValueInput>,
    ) -> Option<OperationFollow> {
        let position = self.state.position(uid)?;
        let old = self.state.features[position].clone();
        let before = old.def.info().tool_use()?;
        let mut trial = self.state.clone();
        let (resolved, _, _) = trial.resolve(def, uid, &old.name, Some(&old.def)).ok()?;
        let after = resolved.info().tool_use()?;
        if before.operation.makes_bodies() == after.operation.makes_bodies()
            && old.def.info().new_component() == resolved.info().new_component()
        {
            return None;
        }
        let later: Vec<Arc<FeatureEntry>> = self.state.features[position + 1..]
            .iter()
            .filter(|f| f.def.info().references().features.contains(&uid))
            .cloned()
            .collect();
        if later.is_empty() {
            return None;
        }
        // The bodies the later features name: those the feature set in the
        // last recompute, else what its definition tells.
        let mut old_bodies: BTreeSet<BodyUid> =
            set_bodies(self.feature_result(uid)).into_iter().collect();
        if old_bodies.is_empty() {
            old_bodies = if before.operation.makes_bodies() {
                BTreeSet::from([BodyUid::new(uid, 0)])
            } else {
                before.participants.iter().copied().collect()
            };
        }
        if old_bodies.is_empty() {
            return None;
        }
        trial.features[position] = Arc::new(FeatureEntry {
            def: resolved,
            ..(*old).clone()
        });
        trial.marker = position + 1;
        let result = self.compute_after(&trial).ok()?;
        let made = set_bodies(result.results.get(position).filter(|r| r.uid == uid));
        let candidates: Vec<(BodyUid, &K::Shape)> = made
            .iter()
            .filter_map(|body| result.bodies.get(body).map(|shape| (*body, &shape.value)))
            .collect();
        if candidates.is_empty() {
            return None;
        }
        let mut targets = BTreeMap::new();
        for entry in later {
            let names = own_names(&entry.def, uid);
            let found = candidates.iter().find(|(_, shape)| {
                candidates.len() == 1 || names.iter().all(|n| resolves(&self.kernel, shape, n))
            });
            if let Some((body, _)) = found {
                targets.insert(entry.uid, *body);
            }
        }
        Some(OperationFollow {
            old_bodies,
            targets,
        })
    }
}
