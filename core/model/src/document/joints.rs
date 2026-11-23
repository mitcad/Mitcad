// SPDX-License-Identifier: MIT
//! Commands and views of joints (mitcad#55; the kinematics are in
//! `crate::joints`, the features in `features/joint.rs`): joints, as-built
//! joints that record where their occurrences are, rigid groups, the
//! joints' state and degrees of freedom at the marker, driving their
//! motions and dragging occurrences while they hold.

use std::collections::{BTreeMap, BTreeSet};

use super::{Added, Document, ModelError, invalid};
use crate::assembly::{inverse, solved_placement};
use crate::contact::{ContactHit, ContactPairs, sweep};
use crate::datum::Vec3;
use crate::features::{AsBuiltJointDef, FeatureDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{DofReport, Dragged, JointResult, Motion, dof_report, drag, path_component};
use crate::kernel::Kernel;
// The joint origins and constraint rows before they are added (mitcad#778).
use crate::constraints::{
    Geo, RowCheck, RowReference, constraint_fits, fitting_types, measures, resolve_geo,
};
use crate::features::{ConstraintType, FrameOverride, GeomRef, JointOrigin, OccurrencePath};
use crate::joints::{OriginFrame, apply_override, origin_frame};

/// The largest step (mm) of the grabbed point between two samples of a
/// drag checked against the contact sets, and the most samples.
const DRAG_STEP: f64 = 2.0;
const DRAG_SAMPLES: usize = 16;

/// What [`Document::drive_joint`] did.
#[derive(Debug, Clone, PartialEq)]
pub struct Driven {
    /// The parameters it created.
    pub parameters: Vec<String>,
    /// The contact that stopped the joint short of the values (mitcad#454)
    /// and how far it went toward them, 0 to 1.
    pub contact: Option<(ContactHit, f64)>,
}

/// One frame of [`Document::joint_poses`] (mitcad#1701).
#[derive(Debug, Clone, PartialEq)]
pub struct JointPose {
    /// The occurrences placed elsewhere in the design than the document
    /// places them: each path from the root with its placement in the
    /// design (an occurrence inside a moved one too).
    pub placements: Vec<(Vec<OccurrenceUid>, crate::transform::Transform)>,
    /// Per joint of the component, its motions' values.
    pub values: BTreeMap<FeatureUid, BTreeMap<Motion, f64>>,
}

impl<K: Kernel> Document<K> {
    /// What a joint or as-built joint did in the last recompute; None when
    /// it failed, is suppressed or after the marker.
    pub fn joint_result(&self, uid: FeatureUid) -> Option<&JointResult> {
        self.result.joints.joints.get(&uid)
    }

    /// What a constraint set did in the last recompute (mitcad#148); None
    /// when it failed, is suppressed or after the marker.
    pub fn constraint_result(
        &self,
        uid: FeatureUid,
    ) -> Option<&crate::constraints::ConstraintSetResult> {
        self.result.joints.constraints.get(&uid)
    }

    /// The rigid groups in effect at the marker: feature, component and
    /// occurrences.
    pub fn rigid_groups(&self) -> &[(FeatureUid, ComponentUid, Vec<OccurrenceUid>)] {
        &self.result.joints.groups
    }

    /// The remaining degrees of freedom of a component's top-level
    /// occurrences at the marker (the rank of the joint equations; see
    /// [`DofReport`]).
    pub fn joint_dof(&self, component: ComponentUid) -> DofReport {
        dof_report(
            &self.state.assembly,
            &self.result.joints,
            &self.result.placements,
            component,
        )
    }

    /// Adds an as-built joint to `component` (the active one when None):
    /// with `relative` left out, it records where `a` is in `b`'s
    /// coordinates at the marker (paths from `component`: the product of
    /// their placements is in its coordinates).
    pub fn add_as_built_joint(
        &mut self,
        def: &AsBuiltJointDef<ValueInput>,
        name: Option<&str>,
        component: Option<ComponentUid>,
    ) -> Result<Added, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        let a = &self.state.assembly;
        for (label, path) in [("a", &def.a.0), ("b", &def.b.0)] {
            path_component(a, component, path).map_err(|e| invalid(format!("{label}: {e}")))?;
        }
        let mut def = def.clone();
        if def.relative.is_none() {
            let pa = self.path_transform(&def.a.0);
            let pb = self.path_transform(&def.b.0);
            def.relative = Some(inverse(&pb).after(&pa));
        }
        self.add_feature_to(&FeatureDef::AsBuiltJoint(def), name, Some(component))
    }

    /// A joint feature's definition with its parameters by name, to edit.
    fn joint_input(&self, uid: FeatureUid) -> Result<FeatureDef<ValueInput>, ModelError> {
        super::motion::joint_def(&self.state, uid)
    }

    /// Drives a joint's free motions: each value (mm, angles; parameters and
    /// expressions allowed) is stored as the joint's position and the
    /// motion is held there; None takes the position away (the rest value,
    /// else free). With contact sets (mitcad#454) the joint moves from
    /// where it is toward the values and stops at the first contact: the
    /// positions are then where it stopped (numbers). One undo step.
    pub fn drive_joint(
        &mut self,
        uid: FeatureUid,
        values: &BTreeMap<Motion, Option<ValueInput>>,
    ) -> Result<Driven, ModelError> {
        let mut def = self.joint_input(uid)?;
        // A cam's follower follows the cam (mitcad#466).
        for (motion, value) in values {
            if value.is_none() {
                continue;
            }
            for f in self.state.features.iter().filter(|f| !f.suppressed) {
                if let FeatureDef::MotionLink(l) = &f.def
                    && matches!(l.coupling, crate::features::Coupling::Cam { .. })
                    && l.b.joint == uid
                    && l.b.motion == *motion
                {
                    let cam = self
                        .state
                        .entry(l.a.joint)
                        .map_or_else(|| l.a.joint.to_string(), |e| e.name.clone());
                    return Err(invalid(format!(
                        "{motion} follows the cam of {}: drive {cam} instead",
                        f.name
                    )));
                }
            }
        }
        // Where the given values take it.
        let mut to = BTreeMap::new();
        for (motion, value) in values {
            if let Some(v) = value {
                to.insert(*motion, self.motion_value(v, *motion)?);
            }
        }
        let mut contact = None;
        if !to.is_empty() && self.has_contact_pairs() {
            let outcome = self.drive_to(&BTreeMap::from([(uid, to)]))?;
            if outcome.contact.is_some() {
                contact = outcome.contact.map(|hit| (hit, outcome.fraction));
                let reached = outcome.values.get(&uid).cloned().unwrap_or_default();
                let position = match &mut def {
                    FeatureDef::Joint(j) => &mut j.position,
                    FeatureDef::AsBuiltJoint(j) => &mut j.position,
                    _ => unreachable!("a joint"),
                };
                for (motion, v) in reached {
                    position.insert(motion, ValueInput::Number(v));
                }
            }
        }
        let position = match &mut def {
            FeatureDef::Joint(j) => &mut j.position,
            FeatureDef::AsBuiltJoint(j) => &mut j.position,
            _ => unreachable!("a joint"),
        };
        for (motion, value) in values {
            match value {
                Some(v) if contact.is_none() => {
                    position.insert(*motion, v.clone());
                }
                Some(_) => {}
                None => {
                    position.remove(motion);
                }
            };
        }
        let entry = self.state.entry(uid).expect("found");
        self.state.editable(entry.component)?;
        let name = entry.name.clone();
        // The motions linked to the driven ones follow them (mitcad#466).
        let driven: Vec<(FeatureUid, Motion)> = values
            .iter()
            .filter(|(_, v)| v.is_some())
            .map(|(m, _)| (uid, *m))
            .collect();
        let parameters = self.apply(|state| {
            let created = state.edit_feature(uid, &def)?;
            state.release_linked(&driven)?;
            Ok((format!("Drive {name}"), created))
        })?;
        Ok(Driven {
            parameters,
            contact,
        })
    }

    /// Where dragging `occurrence` would put the occurrences of its parent
    /// component: `grab` (in the parent's coordinates, the occurrence's
    /// origin when None) pulled toward `target` while the joints in effect
    /// at the marker hold, the other occurrences moving as little as
    /// possible. Driven motions follow like free ones, but those of the
    /// joints BUILD placed its parts with (mitcad#2088): a part BUILD placed
    /// does not drag, and its joint keeps its Offset while another part
    /// drags (a carriage's slide is the exception, within its limits).
    /// Changes nothing.
    pub fn joint_drag(
        &self,
        occurrence: OccurrenceUid,
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        self.joint_drag_at(&[occurrence], grab, target)
    }

    /// [`Document::joint_drag`] of the occurrence a path from the root
    /// ends in: one inside a flexible occurrence that its joints move
    /// (mitcad#1793) is dragged in the flexible occurrence's parent, for
    /// that occurrence alone (`grab` and `target` still in its own
    /// parent's coordinates).
    pub fn joint_drag_at(
        &self,
        path: &[OccurrenceUid],
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        let occurrence = *path
            .last()
            .ok_or_else(|| invalid("no occurrence to drag"))?;
        self.state.require_occurrence(occurrence)?;
        // A part BUILD placed moves with Placement (mitcad#2088).
        if let Some(message) = self.build_drag_refusal(occurrence) {
            return Err(invalid(message));
        }
        let held = self.build_held_joints();
        let prefix = &path[..path.len() - 1];
        let grab = grab.unwrap_or_else(|| {
            self.placement_after(prefix, occurrence)
                .map_or([0.0; 3], |p| p.translation)
        });
        if !grab.iter().chain(&target).all(|v| v.is_finite()) {
            return Err(invalid("the points must be finite"));
        }
        let drag_to = |t: f64| {
            let at: Vec3 = std::array::from_fn(|i| grab[i] + (target[i] - grab[i]) * t);
            drag(
                &self.state.assembly,
                &self.result.joints,
                &self.result.placements,
                path,
                grab,
                at,
                &held,
            )
            .map_err(invalid)
        };
        // Contact sets (mitcad#454): the drag stops where bodies touch.
        let pairs = ContactPairs::new(&self.kernel, &self.state, &self.result);
        if pairs.is_empty() {
            return drag_to(1.0);
        }
        let failed = |e: crate::kernel::KernelError| invalid(format!("contact check: {e}"));
        let skip: BTreeSet<usize> = pairs
            .overlapping(
                &self.kernel,
                &self.result.placements,
                &self.result.joints.flexed,
                &BTreeSet::new(),
            )
            .map_err(failed)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let distance = (0..3)
            .map(|i| (target[i] - grab[i]).powi(2))
            .sum::<f64>()
            .sqrt();
        let samples = ((distance / DRAG_STEP).ceil() as usize).clamp(1, DRAG_SAMPLES);
        let stop = sweep(
            samples,
            |t| -> Result<(Dragged, Option<ContactHit>), ModelError> {
                let dragged = drag_to(t)?;
                let mut placements = self.result.placements.clone();
                placements.extend(dragged.placements.iter().copied());
                let mut flexed = self.result.joints.flexed.clone();
                flexed.extend(dragged.flexed.iter().cloned());
                let deepest = pairs
                    .overlapping(&self.kernel, &placements, &flexed, &skip)
                    .map_err(failed)?
                    .into_iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                Ok((dragged, deepest.map(|(p, v)| pairs.hit(p, v))))
            },
        )?;
        let mut dragged = stop.pose;
        dragged.contact = stop.hit;
        Ok(dragged)
    }

    /// Where a drag ([`Document::joint_drag_at`]) shows the occurrences:
    /// those it places elsewhere in the design than the document does, each
    /// path from the root with its placement in the design (an occurrence
    /// inside a moved one too; inside flexible ones where the drag puts
    /// them, mitcad#1793).
    pub fn dragged_in_design(
        &self,
        dragged: &Dragged,
    ) -> Vec<(Vec<OccurrenceUid>, crate::transform::Transform)> {
        let mut placements = self.result.placements.clone();
        placements.extend(dragged.placements.iter().copied());
        let mut flexed = self.result.joints.flexed.clone();
        flexed.extend(dragged.flexed.iter().cloned());
        self.moved_placements(&self.state, &placements, &flexed)
    }

    /// Animate Joint's frames (mitcad#1701): where `motion` of `joint`
    /// driven to each of `values` in turn puts the occurrences, as a drag
    /// puts them (the joint solver on the placements at the marker, each
    /// frame from the one before; no recompute, the shapes as they are).
    /// Per frame the occurrences placed elsewhere in the design than the
    /// document places them (paths from the root and placements in the
    /// design, as a preview's `placements`) and the joints' values.
    /// Refused where only a recompute shows the motion: with contact sets
    /// (their checks need the bodies), and when joints of other components
    /// hold occurrences inside the ones that move. Changes nothing.
    pub fn joint_poses(
        &self,
        joint: FeatureUid,
        motion: Motion,
        values: &[f64],
    ) -> Result<Vec<JointPose>, ModelError> {
        self.joint_poses_in(joint, None, motion, values)
    }

    /// [`Document::joint_poses`] of a joint inside a flexible `occurrence`
    /// (mitcad#1805), for it alone: a joint of its component, or of a
    /// flexible occurrence nested in it, driven as `drive_joint` with that
    /// occurrence drives it, solved in the occurrence's parent with the
    /// occurrence held; the values per frame are the occurrence's joints'.
    /// Without an occurrence, the joint's own.
    pub fn joint_poses_in(
        &self,
        joint: FeatureUid,
        occurrence: Option<OccurrenceUid>,
        motion: Motion,
        values: &[f64],
    ) -> Result<Vec<JointPose>, ModelError> {
        super::motion::joint_def(&self.state, joint)?;
        if self.has_contact_pairs() {
            return Err(invalid(
                "contact sets apply: only a recompute checks where the bodies touch",
            ));
        }
        let within = match occurrence {
            None => Vec::new(),
            Some(o) => {
                let name = self.state.assembly.occurrence_name(o);
                let joint_name = self
                    .feature(joint)
                    .map_or_else(String::new, |f| f.name.clone());
                if !self.is_flexible(o) {
                    return Err(invalid(format!(
                        "{name} is not flexible: make it Flexible to animate {joint_name} in it \
                         alone"
                    )));
                }
                self.flexible_chain(o, joint)
                    .ok_or_else(|| invalid(format!("{joint_name} does not move inside {name}")))?
            }
        };
        let component = self
            .result
            .joints
            .active
            .iter()
            .find(|j| j.uid == joint && j.row.is_none() && j.under == within)
            .map(|j| j.component)
            .ok_or_else(|| invalid("the joint is not in effect at the marker"))?;
        // Joints of other components that hold occurrences inside this
        // component's are solved by a recompute only.
        let inside = |path: &[OccurrenceUid]| {
            path.iter().skip(1).any(|o| {
                self.state
                    .assembly
                    .occurrence(*o)
                    .is_some_and(|o| o.parent == component)
            })
        };
        if self
            .result
            .joints
            .active
            .iter()
            .any(|j| j.component != component && (inside(&j.a) || inside(&j.b)))
        {
            return Err(invalid(
                "joints of other components hold occurrences that move: only a recompute \
                 solves them",
            ));
        }
        let frames = crate::joints::poses(
            &self.state.assembly,
            &self.result.joints,
            &self.result.placements,
            joint,
            &within,
            motion,
            values,
        )
        .map_err(invalid)?;
        Ok(frames
            .into_iter()
            .map(|mut frame| {
                let placements: BTreeMap<OccurrenceUid, crate::transform::Transform> =
                    frame.placements.into_iter().collect();
                let flexed: crate::joints::Flexed = frame.flexed.into_iter().collect();
                // Inside a flexible occurrence, its joints' values there.
                let values = match within.first() {
                    Some(o) => frame.flex_values.remove(o).unwrap_or_default(),
                    None => frame.values,
                };
                JointPose {
                    placements: self.moved_placements(&self.state, &placements, &flexed),
                    values,
                }
            })
            .collect())
    }

    /// Drags `occurrence` (see [`Document::joint_drag`]) and keeps the
    /// result: the moved occurrences' own placements become where the drag
    /// put them, each made a rotation and a translation, and the joints'
    /// driven motions are driven to their new values. One undo step; a
    /// placement with no rotation near it moves nothing, and so does a
    /// drag after which a joint that held would fail in the timeline
    /// (mitcad#1666).
    pub fn drag_occurrence(
        &mut self,
        occurrence: OccurrenceUid,
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        self.drag_occurrence_at(&[occurrence], grab, target)
    }

    /// [`Document::drag_occurrence`] of the occurrence a path from the root
    /// ends in ([`Document::joint_drag_at`]): the joints of a flexible
    /// occurrence's component that the drag moved keep their new values as
    /// that occurrence's own (mitcad#1793), driven ones as its positions,
    /// free ones as where they start; the component's design stays.
    pub fn drag_occurrence_at(
        &mut self,
        path: &[OccurrenceUid],
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        let mut dragged = self.joint_drag_at(path, grab, target)?;
        let occurrence = *path.last().expect("a path to drag");
        let mut edits = Vec::new();
        let changed = |v: f64, was: f64| (v - was).abs() > 1e-12 * (1.0 + v.abs());
        // A carriage that follows another (mitcad#2274) takes its offset
        // from its leader's in this step (`build/follow.rs`).
        let released = crate::build::follow::released_joints(&self.state, &self.result.joints);
        for joint in self
            .result
            .joints
            .active
            .iter()
            .filter(|j| j.under.is_empty() && !released.contains(&j.uid))
        {
            let Some(values) = dragged.values.get(&joint.uid) else {
                continue;
            };
            let moved: BTreeMap<Motion, Option<ValueInput>> = joint
                .motions
                .iter()
                .filter_map(|spec| {
                    let target = spec.target?;
                    // Within its limits, also by a rounding past one where
                    // the drag stopped at it (mitcad#2088).
                    let v = values
                        .get(&spec.motion)?
                        .max(spec.min.unwrap_or(f64::NEG_INFINITY))
                        .min(spec.max.unwrap_or(f64::INFINITY));
                    changed(v, target).then_some((spec.motion, Some(ValueInput::Number(v))))
                })
                .collect();
            // A joint's free motions that moved (mitcad#1745): the joint
            // keeps them as its start values, so that it places its moving
            // side where the drag left it, also when a feature before it
            // places that side.
            let now = self
                .result
                .joints
                .joints
                .get(&joint.uid)
                .and_then(|r| r.values.as_ref());
            let free: BTreeMap<Motion, f64> = joint
                .motions
                .iter()
                .filter(|spec| spec.target.is_none())
                .filter_map(|spec| Some((spec.motion, *values.get(&spec.motion)?)))
                .collect();
            let free_moved = free.iter().any(|(m, v)| {
                let was = joint
                    .motions
                    .iter()
                    .find(|s| s.motion == *m)
                    .and_then(|s| s.start)
                    .or_else(|| now.and_then(|n| n.get(m).copied()));
                was.is_none_or(|was| changed(*v, was))
            });
            let mut def = self.joint_input(joint.uid)?;
            let keeps_start = free_moved && matches!(def, FeatureDef::Joint(_));
            if moved.is_empty() && !keeps_start {
                continue;
            }
            let position = match &mut def {
                FeatureDef::Joint(j) => {
                    if keeps_start {
                        j.start.extend(free);
                    }
                    &mut j.position
                }
                FeatureDef::AsBuiltJoint(j) => &mut j.position,
                _ => unreachable!("a joint"),
            };
            for (m, v) in moved {
                position.insert(m, v.expect("a value"));
            }
            edits.push((joint.uid, def));
        }
        // The joints of flexible occurrences that moved (mitcad#1793): their
        // new values become the occurrences' own.
        let mut flex_edits: Vec<(OccurrenceUid, crate::assembly::Flex)> = Vec::new();
        for (f, by_joint) in &dragged.flex_values {
            let Some(mut flex) = self
                .state
                .assembly
                .occurrence(*f)
                .and_then(|o| o.flexible.clone())
            else {
                continue;
            };
            let now = self.result.joints.flex_values.get(f);
            let mut any = false;
            for joint in self
                .result
                .joints
                .active
                .iter()
                .filter(|j| j.under.first() == Some(f) && j.row.is_none())
            {
                let Some(values) = by_joint.get(&joint.uid) else {
                    continue;
                };
                for spec in &joint.motions {
                    let Some(v) = values.get(&spec.motion).copied() else {
                        continue;
                    };
                    let kept = match spec.target {
                        Some(target) => changed(v, target).then_some(&mut flex.position),
                        None => {
                            let was = spec.start.or_else(|| {
                                now.and_then(|n| n.get(&joint.uid))
                                    .and_then(|n| n.get(&spec.motion).copied())
                            });
                            was.is_none_or(|was| changed(v, was))
                                .then_some(&mut flex.start)
                        }
                    };
                    if let Some(kept) = kept {
                        kept.entry(joint.uid).or_default().insert(spec.motion, v);
                        any = true;
                    }
                }
            }
            if any {
                flex_edits.push((*f, flex));
            }
        }
        // The free parts of flexible occurrences that the drag moved
        // (mitcad#2016): where it left them is kept by the outermost
        // flexible occurrence of their chain, for it alone, as a drag keeps
        // a placed occurrence's own placement.
        for ((chain, part), transform) in &dragged.flexed {
            let Some(path) = self.free_part_moved(chain, *part, transform) else {
                continue;
            };
            let outer = chain[0];
            let at = match flex_edits.iter().position(|(o, _)| *o == outer) {
                Some(at) => at,
                None => {
                    let Some(flex) = self
                        .state
                        .assembly
                        .occurrence(outer)
                        .and_then(|o| o.flexible.clone())
                    else {
                        continue;
                    };
                    flex_edits.push((outer, flex));
                    flex_edits.len() - 1
                }
            };
            // Where it is before the Move features that move it
            // (mitcad#2025), so that they move it to where the drag left it.
            let rigid = solved_placement(transform).unwrap_or(*transform);
            let base = self.free_part_base(*part, &rigid);
            flex_edits[at].1.keep_placement(path, base);
        }
        let name = self.state.assembly.occurrence_name(occurrence);
        // Kept as rotations and translations to rounding (mitcad#1664,
        // mitcad#1666): rounding that piled up over many drags once made
        // designs that no longer opened, and links that came apart.
        let mut placements = Vec::with_capacity(dragged.placements.len());
        for (o, transform) in &dragged.placements {
            let rigid = solved_placement(transform).ok_or_else(|| {
                invalid(format!(
                    "the joints would place {} by a transform that is not a rotation and a \
                     translation; nothing was moved",
                    self.state.assembly.occurrence_name(*o)
                ))
            })?;
            placements.push((*o, rigid));
        }
        // A part of a linked design moves only inside a flexible occurrence
        // of it, for that occurrence alone (mitcad#2209): its own placement
        // and its joints stay the design's.
        for (o, _) in &placements {
            if let Some(placed) = self.state.assembly.occurrence(*o) {
                self.state.editable(placed.parent)?;
            }
        }
        for (uid, _) in &edits {
            if let Some(entry) = self.state.entry(*uid) {
                self.state.editable(entry.component)?;
            }
        }
        // The joints that hold now (mitcad#1666): the recompute in timeline
        // order must keep them holding where the drag leaves the
        // occurrences, else the drag is refused and nothing moves.
        let held: BTreeSet<FeatureUid> = self
            .result
            .results
            .iter()
            .filter(|r| r.status.is_ok())
            .map(|r| r.uid)
            .collect();
        // An occurrence a feature before the marker places keeps its own
        // placement: the drag is kept only when the joints, with their new
        // start values, put it where the drag left it (mitcad#1745).
        let positioned: Vec<(OccurrenceUid, String)> = placements
            .iter()
            .filter_map(|(o, _)| {
                let by = self.state.positioning_features(*o);
                (!by.is_empty()).then(|| (*o, by.join(", ")))
            })
            .collect();
        self.apply_checked(
            |state| {
                for (o, transform) in &placements {
                    if positioned.iter().any(|(p, _)| p == o) {
                        continue;
                    }
                    state.assembly.occurrence_mut(*o).expect("exists").transform = *transform;
                }
                for (uid, def) in &edits {
                    state.edit_feature(*uid, def)?;
                }
                for (o, flex) in &flex_edits {
                    state.assembly.occurrence_mut(*o).expect("exists").flexible =
                        Some(flex.clone());
                }
                Ok((format!("Drag {name}"), ()))
            },
            |state, result| {
                for (o, by) in &positioned {
                    let wanted = placements.iter().find(|(p, _)| p == o).map(|(_, t)| t);
                    let kept = match (wanted, result.placements.get(o)) {
                        (Some(w), Some(at)) => same_placement(w, at),
                        _ => false,
                    };
                    if !kept {
                        return Err(invalid(format!(
                            "{} is placed by {by} in the timeline; capture the position instead",
                            state.assembly.occurrence_name(*o)
                        )));
                    }
                }
                joints_still_hold(state, result, &held, &name)
            },
        )?;
        dragged.placements = placements;
        Ok(dragged)
    }

    /// The path under which the first flexible occurrence of `chain`
    /// keeps where `part` is (mitcad#2016, [`crate::assembly::Flex`]), when
    /// `part` is a free part of the last one (it has free parts, and no
    /// joint of its component is on the part or on what a rigid group of
    /// the component ties to it) that `transform` puts elsewhere than it is
    /// at the marker; None otherwise. The parts the component's joints
    /// move keep their joints' values instead.
    fn free_part_moved(
        &self,
        chain: &[OccurrenceUid],
        part: OccurrenceUid,
        transform: &crate::transform::Transform,
    ) -> Option<Vec<OccurrenceUid>> {
        let assembly = &self.state.assembly;
        let last = assembly.occurrence(*chain.last()?)?;
        if !last.flexible.as_ref().is_some_and(|f| f.free_parts) {
            return None;
        }
        let joints = &self.result.joints;
        let members = crate::joints::group_of(&joints.groups, last.component, part);
        if crate::joints::joined_in(joints, last.component, &members) {
            return None;
        }
        let mut path = chain[1..].to_vec();
        path.push(part);
        if !assembly.flex_path_holds(chain[0], &path) {
            return None;
        }
        let now = crate::joints::flexed_step(&joints.flexed, chain, part)
            .or_else(|| self.placement(part))?;
        (!same_placement(&now, transform)).then_some(path)
    }
}

/// Whether two placements agree as a solve repeats them (1e-3 mm, 1e-6 in
/// the rotation's entries): a drag the timeline does not keep is off by far
/// more.
fn same_placement(a: &crate::transform::Transform, b: &crate::transform::Transform) -> bool {
    let rotation = (0..3).all(|r| (0..3).all(|c| (a.linear[r][c] - b.linear[r][c]).abs() <= 1e-6));
    let translation = (0..3).all(|i| (a.translation[i] - b.translation[i]).abs() <= 1e-3);
    rotation && translation
}

/// Refuses a drag after which a joint, as-built joint, rigid group,
/// constraint set or motion link that held before fails when the design
/// is recomputed in timeline order (mitcad#1666), naming them and why the
/// first fails.
fn joints_still_hold<S>(
    state: &super::DocState,
    result: &crate::recompute::Recomputed<S>,
    held: &BTreeSet<FeatureUid>,
    dragged: &str,
) -> Result<(), ModelError> {
    let mut failed = Vec::new();
    for r in &result.results {
        let Some(error) = r.status.failure() else {
            continue;
        };
        let Some(entry) = state.entry(r.uid) else {
            continue;
        };
        if held.contains(&r.uid) && crate::joints::applies(&entry.def) {
            failed.push((entry.name.clone(), error.message.clone()));
        }
    }
    let Some((first, why)) = failed.first() else {
        return Ok(());
    };
    let names: Vec<&str> = failed.iter().map(|(n, _)| n.as_str()).collect();
    let list = match names.as_slice() {
        [one] => (*one).to_owned(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
        [] => unreachable!("one at least"),
    };
    Err(invalid(format!(
        "{dragged} was not moved: where the drag would leave it, {list} would no longer hold \
         ({first}: {why})"
    )))
}

// Joint origins and constraint rows before they are added (mitcad#778: the
// api's `joint_frame` and `constraint_row` queries answer with these, and
// the import calls them without the api).

impl<K: Kernel> Document<K> {
    /// An occurrence path from `component`: uids (`O1/O4`), names
    /// (`Arm:1/Pin:2`), or empty for the component's own geometry.
    pub fn occurrence_path_from(
        &self,
        component: ComponentUid,
        text: &str,
    ) -> Result<Vec<OccurrenceUid>, String> {
        let a = self.assembly();
        crate::joints::find_path_from(a, component, text)
            .ok_or_else(|| format!("no occurrence {text} in {}", a.name(component)))
    }

    /// The frame a joint origin gives at the marker, as a joint would
    /// resolve it: `geometry` on the occurrence path `occurrence` from
    /// `from` ([`Document::occurrence_path_from`]), with `frame_override`,
    /// placed by the path's occurrences into the coordinates of `from`.
    /// The geometry must be the component's the path ends in.
    pub fn joint_frame(
        &self,
        from: ComponentUid,
        occurrence: &str,
        geometry: &GeomRef,
        frame_override: Option<&FrameOverride>,
    ) -> Result<OriginFrame, String> {
        let a = self.assembly();
        let path = self.occurrence_path_from(from, occurrence)?;
        let at = path_component(a, from, &path)?;
        let owner = match geometry {
            GeomRef::Face { body, .. }
            | GeomRef::Edge { body, .. }
            | GeomRef::Vertex { body, .. }
            | GeomRef::Body(body) => Some(
                self.body_component(*body)
                    .ok_or_else(|| format!("body {body} does not exist at the marker"))?,
            ),
            GeomRef::Datum(uid)
            | GeomRef::SketchCurves { sketch: uid, .. }
            | GeomRef::SketchPoint { sketch: uid, .. }
            | GeomRef::Mount { feature: uid, .. } => Some(
                self.feature(*uid)
                    .ok_or_else(|| format!("feature {uid} does not exist"))?
                    .component,
            ),
            _ => None,
        };
        if let Some(owner) = owner
            && owner != at
        {
            return Err(format!(
                "{} is in {}, not in {}",
                geometry,
                a.name(owner),
                a.name(at)
            ));
        }
        let mut local = origin_frame(&mut self.resolver(), geometry)?;
        if let Some(o) = frame_override {
            local = apply_override(&local, o)?;
        }
        Ok(OriginFrame {
            component: at,
            frame: self.path_transform(&path).after(&local),
            local,
        })
    }

    /// A reference of a constraint row at the marker, its frame placed by
    /// its path into `component`'s coordinates.
    fn row_geo(
        &self,
        component: ComponentUid,
        reference: &RowReference<'_>,
    ) -> Result<Geo, String> {
        let path = self.occurrence_path_from(component, reference.occurrence)?;
        let origin = JointOrigin {
            occurrence: OccurrencePath(path.clone()),
            geometry: reference.geometry.clone(),
            frame_override: reference.frame_override,
        };
        let mut geo = resolve_geo(&mut self.resolver(), &origin)?;
        geo.frame = self.path_transform(&path).after(&geo.frame);
        Ok(geo)
    }

    /// A constraint row's references before the row is added, in
    /// `component` (paths from it): their kinds, the types they fit (the
    /// likeliest first), what they measure where they are (an align's
    /// `offset`, the `angle` between their z axes, an insert's `twist`: the
    /// turn a locked insert records), and for a `kind` whether they fit
    /// it. A reference that does not resolve fails with its key (`a: …`).
    pub fn constraint_row(
        &self,
        component: ComponentUid,
        a: &RowReference<'_>,
        b: &RowReference<'_>,
        a2: Option<&RowReference<'_>>,
        b2: Option<&RowReference<'_>>,
        kind: Option<ConstraintType>,
    ) -> Result<RowCheck, String> {
        let geo = |key: &str, reference: &RowReference<'_>| {
            self.row_geo(component, reference)
                .map_err(|e| format!("{key}: {e}"))
        };
        let a = geo("a", a)?;
        let b = geo("b", b)?;
        let a2 = a2.map(|r| geo("a2", r)).transpose()?;
        let b2 = b2.map(|r| geo("b2", r)).transpose()?;
        Ok(RowCheck {
            types: fitting_types(&a, &b),
            measures: measures(&a, &b),
            fits: kind.map(|kind| constraint_fits(kind, &a, a2.as_ref(), &b, b2.as_ref())),
            a,
            b,
        })
    }
}
