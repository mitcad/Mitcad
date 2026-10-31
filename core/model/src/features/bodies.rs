// SPDX-License-Identifier: MIT
//! Shared by the features that combine tool bodies with bodies one step
//! after another (patterns, mirrors, combine, primitives): a working copy of
//! the body state, the Join, Cut and Intersect of a tool with participant
//! bodies as an extrude does them, and reading another feature's tool.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use super::base::{Joined, body_lumps, join_pieces};
use super::{BodyChange, EvalContext, FeatureEntry, Operation};
use crate::ids::{BodyUid, FeatureUid};
use crate::kernel::{BooleanOp, Kernel};
use crate::recompute::{Change, ChangeHead, FeatureStatus, Read};

/// A feature's changes of bodies: each body it set with its shape, each
/// body it removed with None ([`EvalContext::feature_body_changes`]).
pub(crate) type BodyChanges<S> = Vec<(BodyUid, Option<S>)>;

/// The bodies as a feature changes them; [`BodySet::changes`] gives the
/// feature's output.
#[derive(Clone)]
pub(crate) struct BodySet<S> {
    current: BTreeMap<BodyUid, S>,
    before: BTreeSet<BodyUid>,
    /// Bodies the feature set, in the order it first set them.
    set: Vec<BodyUid>,
}

impl<S: Clone> BodySet<S> {
    pub fn new(bodies: Vec<(BodyUid, S)>) -> Self {
        Self {
            before: bodies.iter().map(|(uid, _)| *uid).collect(),
            current: bodies.into_iter().collect(),
            set: Vec::new(),
        }
    }

    pub fn get(&self, uid: BodyUid) -> Option<&S> {
        self.current.get(&uid)
    }

    pub fn set(&mut self, uid: BodyUid, shape: S) {
        if !self.set.contains(&uid) {
            self.set.push(uid);
        }
        self.current.insert(uid, shape);
    }

    pub fn remove(&mut self, uid: BodyUid) {
        self.current.remove(&uid);
    }

    /// The bodies there are now, in order of their ids.
    pub fn uids(&self) -> Vec<BodyUid> {
        self.current.keys().copied().collect()
    }

    /// The bodies the feature set so far that are still there, in the
    /// order it first set them.
    pub fn changed(&self) -> Vec<BodyUid> {
        self.set
            .iter()
            .copied()
            .filter(|uid| self.current.contains_key(uid))
            .collect()
    }

    /// The bodies the feature made so far (not there before it) that are
    /// still there, in the order it first set them.
    pub fn made(&self) -> Vec<BodyUid> {
        self.changed()
            .into_iter()
            .filter(|uid| !self.before.contains(uid))
            .collect()
    }

    /// Whether the body was there before the feature.
    pub fn was_there(&self, uid: BodyUid) -> bool {
        self.before.contains(&uid)
    }

    /// The shapes of the participants an operation works on ([`apply_tool`]:
    /// empty means all bodies); bodies that do not exist are left out.
    pub fn participants(&self, participants: &[BodyUid]) -> Vec<&S> {
        if participants.is_empty() {
            self.current.values().collect()
        } else {
            participants
                .iter()
                .filter_map(|uid| self.get(*uid))
                .collect()
        }
    }

    /// Sets of the bodies the feature changed or created, in order, then
    /// removals of the bodies it removed.
    pub fn changes(self) -> Vec<BodyChange<S>> {
        let mut changes: Vec<BodyChange<S>> = self
            .set
            .iter()
            .filter_map(|uid| {
                self.current
                    .get(uid)
                    .map(|shape| BodyChange::Set(*uid, shape.clone()))
            })
            .collect();
        changes.extend(
            self.before
                .iter()
                .filter(|uid| !self.current.contains_key(uid))
                .map(|uid| BodyChange::Remove(*uid)),
        );
        changes
    }
}

pub(crate) fn boolean_op(operation: Operation) -> Option<BooleanOp> {
    match operation {
        Operation::NewBody | Operation::NewComponent => None,
        Operation::Join => Some(BooleanOp::Join),
        Operation::Cut => Some(BooleanOp::Cut),
        Operation::Intersect => Some(BooleanOp::Intersect),
    }
}

pub(crate) fn verb(op: BooleanOp) -> &'static str {
    match op {
        BooleanOp::Join => "join",
        BooleanOp::Cut => "cut",
        BooleanOp::Intersect => "intersect",
    }
}

/// Combines a tool with bodies as an extrude does: New Body makes a body
/// of each solid; Join fuses the tool with every participant it touches
/// (merged bodies go into the first, a body of several solids keeps those
/// the tool does not reach, a tool solid touching nothing becomes a body);
/// Cut and Intersect work on each participant they reach (a body
/// cut away is removed, pieces after the first become bodies). Empty
/// `participants` means all bodies. Returns false when a Join, Cut or
/// Intersect reaches no participant; the bodies are then unchanged.
pub(crate) fn apply_tool<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &mut BodySet<K::Shape>,
    operation: Operation,
    participants: &[BodyUid],
    tool: &K::Shape,
) -> Result<bool, String> {
    apply_tool_strays(ctx, bodies, operation, participants, tool, true)
}

/// [`apply_tool`]; without `strays` a Join's tool solids that touch
/// nothing are left out instead of becoming bodies (a pattern's copies
/// applied at once, as Adjust skips copies that reach no body).
pub(crate) fn apply_tool_strays<K: Kernel>(
    ctx: &mut EvalContext<'_, K>,
    bodies: &mut BodySet<K::Shape>,
    operation: Operation,
    participants: &[BodyUid],
    tool: &K::Shape,
    strays: bool,
) -> Result<bool, String> {
    let Some(op) = boolean_op(operation) else {
        for solid in ctx.kernel.solids(tool).map_err(|e| e.to_string())? {
            let uid = ctx.new_body();
            bodies.set(uid, solid);
        }
        return Ok(true);
    };
    // Mesh bodies are left out of all bodies and refused when listed
    // (mitcad#399).
    let uids: Vec<BodyUid> = if participants.is_empty() {
        bodies
            .current
            .iter()
            .filter(|(_, shape)| !ctx.is_mesh(shape))
            .map(|(uid, _)| *uid)
            .collect()
    } else {
        participants.to_vec()
    };
    // A join works on each lump of a body of several and keeps the lumps it
    // does not reach, as an extrusion's does (mitcad#1003).
    let mut lumps = Vec::with_capacity(uids.len());
    for uid in &uids {
        let shape = bodies
            .get(*uid)
            .cloned()
            .ok_or_else(|| format!("body {uid} does not exist at this point of the timeline"))?;
        ctx.require_surfaces(*uid, &shape)?;
        lumps.push(match op {
            BooleanOp::Join => body_lumps(ctx.kernel, &shape),
            BooleanOp::Cut | BooleanOp::Intersect => vec![shape],
        });
    }
    if lumps.is_empty() {
        return Err(format!("there is no body to {}", verb(op)));
    }
    let refs: Vec<&K::Shape> = lumps.iter().flatten().collect();
    let result = ctx.boolean(op, &refs, tool).map_err(|e| e.to_string())?;
    if !result.touched.iter().any(|t| *t) {
        return Ok(false);
    }
    if op == BooleanOp::Join {
        for joined in join_pieces(ctx.kernel, result.pieces, &lumps, &result.touched) {
            match joined {
                Joined::Body(i, shape) => bodies.set(uids[i], shape),
                Joined::Merged(i) => bodies.remove(uids[i]),
                Joined::Stray(_) if !strays => {}
                Joined::Stray(shape) | Joined::Apart(shape) => {
                    let uid = ctx.new_body();
                    bodies.set(uid, shape);
                }
            }
        }
        return Ok(true);
    }
    let mut pieces: Vec<Vec<K::Shape>> = vec![Vec::new(); uids.len()];
    for piece in result.pieces {
        if let Some(source) = piece.sources.first() {
            pieces[*source].push(piece.shape);
        }
    }
    for (i, uid) in uids.iter().enumerate() {
        if !result.touched[i] {
            continue;
        }
        // The pieces' ids (mitcad#408); none left for the body removes it.
        let named = ctx.piece_ids(*uid, std::mem::take(&mut pieces[i]));
        if !named.iter().any(|(id, _)| id == uid) {
            bodies.remove(*uid);
        }
        for (id, piece) in named {
            bodies.set(id, piece);
        }
    }
    Ok(true)
}

impl<K: Kernel> EvalContext<'_, K> {
    /// An entry of the timeline.
    pub(crate) fn feature_entry(&self, uid: FeatureUid) -> Result<Arc<FeatureEntry>, String> {
        self.env
            .feature(uid)
            .cloned()
            .ok_or_else(|| format!("feature {uid} does not exist"))
    }

    /// The tool body an earlier feature left ([`super::FeatureOutput::tool`]).
    /// The read covers the feature's whole result, so a change of the
    /// feature evaluates the reader again.
    pub fn feature_tool(&mut self, uid: FeatureUid) -> Result<K::Shape, String> {
        let result = self.env.result_of(uid);
        let output = result.and_then(|r| r.output.as_ref());
        self.reads
            .push(Read::Output(uid, output.map(|o| o.version)));
        let name = self.feature_name(uid);
        match (result.map(|r| &r.status), output) {
            (Some(FeatureStatus::Suppressed), _) => Err(format!("{name} is suppressed")),
            (Some(FeatureStatus::Failed(_)), _) => Err(format!("{name} failed")),
            (_, Some(output)) => output
                .tool
                .clone()
                .ok_or_else(|| format!("{name} has no tool body")),
            _ => Err(format!("{name} has no result")),
        }
    }

    /// A body as it was just before an earlier feature `before` (the shape
    /// the last feature before it set); None when no feature before it set
    /// the body. The read covers that feature's whole result.
    pub(crate) fn body_before(&mut self, before: FeatureUid, body: BodyUid) -> Option<K::Shape> {
        let at = self.env.history.iter().position(|r| r.uid == before)?;
        let result = self.env.history[..at]
            .iter()
            .rev()
            .find(|r| r.status.is_ok() && r.shape_set(body).is_some())?;
        self.reads.push(Read::Output(
            result.uid,
            result.output.as_ref().map(|o| o.version),
        ));
        result.shape_set(body).cloned()
    }

    /// A body as an earlier feature `after` left it (mitcad#998): the shape
    /// the last feature up to and including it set; None when `after` is
    /// not before this feature or no feature up to it set the body. The
    /// reads cover `after`'s result and that feature's.
    pub(crate) fn body_after(&mut self, after: FeatureUid, body: BodyUid) -> Option<K::Shape> {
        let history = self.env.history;
        let at = history.iter().position(|r| r.uid == after)?;
        self.reads.push(Read::Output(
            after,
            history[at].output.as_ref().map(|o| o.version),
        ));
        let result = history[..=at]
            .iter()
            .rev()
            .find(|r| r.status.is_ok() && r.shape_set(body).is_some())?;
        self.reads.push(Read::Output(
            result.uid,
            result.output.as_ref().map(|o| o.version),
        ));
        result.shape_set(body).cloned()
    }

    /// The bodies an earlier feature's result set or removed. The read
    /// covers the feature's whole result.
    pub(crate) fn feature_changed_bodies(
        &mut self,
        uid: FeatureUid,
    ) -> Result<Vec<BodyUid>, String> {
        let result = self.env.result_of(uid);
        let output = result.and_then(|r| r.output.as_ref());
        self.reads
            .push(Read::Output(uid, output.map(|o| o.version)));
        let name = self.feature_name(uid);
        match (result.map(|r| &r.status), output) {
            (Some(FeatureStatus::Suppressed), _) => Err(format!("{name} is suppressed")),
            (Some(FeatureStatus::Failed(_)), _) => Err(format!("{name} failed")),
            // (Without unpacking restored shapes, mitcad#858.)
            (_, Some(output)) => Ok(output
                .changes
                .heads()
                .iter()
                .map(|change| match change {
                    ChangeHead::Set(body, _) | ChangeHead::Remove(body) => *body,
                })
                .collect()),
            _ => Err(format!("{name} has no result")),
        }
    }

    /// The changes of an earlier feature's result in their order: each body
    /// it set with its shape, each body it removed with None. The read
    /// covers the feature's whole result.
    pub(crate) fn feature_body_changes(
        &mut self,
        uid: FeatureUid,
    ) -> Result<BodyChanges<K::Shape>, String> {
        let result = self.env.result_of(uid);
        let output = result.and_then(|r| r.output.as_ref());
        self.reads
            .push(Read::Output(uid, output.map(|o| o.version)));
        let name = self.feature_name(uid);
        match (result.map(|r| &r.status), output) {
            (Some(FeatureStatus::Suppressed), _) => Err(format!("{name} is suppressed")),
            (Some(FeatureStatus::Failed(_)), _) => Err(format!("{name} failed")),
            (_, Some(output)) => Ok(output
                .changes
                .iter()
                .map(|change| match change {
                    Change::Set(body, shape) => (*body, Some(shape.value.clone())),
                    Change::Remove(body) => (*body, None),
                })
                .collect()),
            _ => Err(format!("{name} has no result")),
        }
    }

    /// The elements an earlier pattern or mirror placed its objects at
    /// (element 0 the original, suppressed ones too; empty for a mirror
    /// whose stored result predates them). The read covers the feature's
    /// whole result.
    pub(crate) fn feature_elements(
        &mut self,
        uid: FeatureUid,
    ) -> Result<Vec<crate::transform::Transform>, String> {
        let result = self.env.result_of(uid);
        let output = result.and_then(|r| r.output.as_ref());
        self.reads
            .push(Read::Output(uid, output.map(|o| o.version)));
        let name = self.feature_name(uid);
        match (result.map(|r| &r.status), output) {
            (Some(FeatureStatus::Suppressed), _) => Err(format!("{name} is suppressed")),
            (Some(FeatureStatus::Failed(_)), _) => Err(format!("{name} failed")),
            (_, Some(output)) => Ok(output.elements.clone()),
            _ => Err(format!("{name} has no result")),
        }
    }
}
