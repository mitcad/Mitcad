// SPDX-License-Identifier: MIT
//! Motion studies and contact sets of the document (mitcad#454; the data
//! in `crate::motion`, the contact checks in `crate::contact`): adding,
//! editing and deleting them (undo steps that recompute nothing), playing a
//! study, and driving joints so that they stop at the first contact.
//!
//! A motion of joints is a path of their values from where they are; each
//! sampled point of it is the document's state with those values as the
//! joints' positions, recomputed (the joints solved as Drive Joint solves
//! them) and checked against the contact sets. Without contact sets a
//! motion goes the whole way and costs one recompute, as before.

use std::collections::{BTreeMap, BTreeSet};

use super::{DocState, Document, ModelError, PreviewReport, invalid};
use crate::contact::{ContactHit, ContactPairs, sweep};
use crate::expr::{Dims, Expr, Unit};
use crate::features::{FeatureDef, OccurrencePath, ValueInput};
use crate::ids::{ComponentUid, FeatureUid};
use crate::joints::{Motion, limit_values, path_component};
use crate::kernel::Kernel;
use crate::motion::{ContactSet, MotionStudy, StudyKey};
use crate::parameters::unit_for;

/// Values of joints' free motions, by joint and motion (mm, radians).
pub type JointValues = BTreeMap<FeatureUid, BTreeMap<Motion, f64>>;

/// Where a motion of joints went.
#[derive(Debug, Clone, PartialEq)]
pub struct MotionOutcome {
    /// The driven motions' values where it ended.
    pub values: JointValues,
    /// How far it went, 0 to 1 (1: the whole way).
    pub fraction: f64,
    /// The contact that stopped it short.
    pub contact: Option<ContactHit>,
}

/// The largest step between two samples of a motion: of a turn (5°) and
/// of a slide (mm). A step of a study is sampled at least once.
const TURN_STEP: f64 = 5.0 * std::f64::consts::PI / 180.0;
const SLIDE_STEP: f64 = 1.0;
const MAX_SAMPLES: usize = 64;

/// A joint feature's definition with its parameters by name, to edit.
pub(crate) fn joint_def(
    state: &DocState,
    uid: FeatureUid,
) -> Result<FeatureDef<ValueInput>, ModelError> {
    let entry = state
        .entry(uid)
        .ok_or_else(|| invalid(format!("feature {uid} does not exist")))?;
    if !matches!(
        entry.def,
        FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_)
    ) {
        return Err(invalid(format!("{} is not a joint", entry.name)));
    }
    let params = &state.parameters;
    Ok(entry
        .def
        .map_params(&mut |_, id| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
        .expect("names never fail"))
}

/// A joint's free motions, None for another feature.
pub(crate) fn joint_motions(def: &FeatureDef) -> Option<Vec<Motion>> {
    match def {
        FeatureDef::Joint(j) => Some(j.kind.motions(j.slide_axis)),
        FeatureDef::AsBuiltJoint(j) => Some(j.kind.motions(j.slide_axis)),
        _ => None,
    }
}

/// `state` with the joints' motions driven to these values, each held
/// within its limits.
pub(crate) fn driven(state: &DocState, values: &JointValues) -> Result<DocState, ModelError> {
    let mut next = state.clone();
    for (uid, motions) in values {
        let limits = match next.entry(*uid).map(|e| &e.def) {
            Some(FeatureDef::Joint(j)) => limit_values(&next.parameters, &j.limits),
            Some(FeatureDef::AsBuiltJoint(j)) => limit_values(&next.parameters, &j.limits),
            _ => BTreeMap::new(),
        };
        let mut def = joint_def(&next, *uid)?;
        let position = match &mut def {
            FeatureDef::Joint(j) => &mut j.position,
            FeatureDef::AsBuiltJoint(j) => &mut j.position,
            _ => unreachable!("a joint"),
        };
        for (motion, value) in motions {
            let [min, max, _] = limits.get(motion).copied().unwrap_or_default();
            let mut v = *value;
            if let Some(min) = min {
                v = v.max(min);
            }
            if let Some(max) = max {
                v = v.min(max);
            }
            position.insert(*motion, ValueInput::Number(v));
        }
        next.edit_feature(*uid, &def)?;
    }
    // The motions linked to the driven ones follow them (mitcad#466).
    let keyed: Vec<(FeatureUid, Motion)> = values
        .iter()
        .flat_map(|(uid, motions)| motions.keys().map(|m| (*uid, *m)))
        .collect();
    next.release_linked(&keyed)?;
    Ok(next)
}

/// Values from `from` toward `to` at `t` (0 to 1); motions only `to` has
/// stay at its values.
fn between(from: &JointValues, to: &JointValues, t: f64) -> JointValues {
    to.iter()
        .map(|(uid, motions)| {
            let start = from.get(uid);
            let values = motions
                .iter()
                .map(|(m, v)| {
                    let a = start.and_then(|s| s.get(m)).copied().unwrap_or(*v);
                    (*m, a + (v - a) * t)
                })
                .collect();
            (*uid, values)
        })
        .collect()
}

/// Samples enough for the motions' changes from `from` to `to`.
fn samples_for(from: &JointValues, to: &JointValues) -> usize {
    let mut most = 1.0_f64;
    for (uid, motions) in to {
        for (m, v) in motions {
            let a = from.get(uid).and_then(|s| s.get(m)).copied().unwrap_or(*v);
            let step = if m.is_rotation() {
                TURN_STEP
            } else {
                SLIDE_STEP
            };
            most = most.max(((v - a).abs() / step).ceil());
        }
    }
    (most as usize).clamp(1, MAX_SAMPLES)
}

impl DocState {
    /// Drops what deleted joints and occurrences leave behind: keys of
    /// joints (or motions) that are gone, occurrences of contact sets
    /// that no longer exist where their paths say.
    pub(crate) fn prune_motion(&mut self) {
        if self.motion_studies.is_empty() && self.contact_sets.is_empty() {
            return;
        }
        let joints: BTreeMap<FeatureUid, Vec<Motion>> = self
            .features
            .iter()
            .filter_map(|f| Some((f.uid, joint_motions(&f.def)?)))
            .collect();
        for study in &mut self.motion_studies {
            study
                .keys
                .retain(|k| joints.get(&k.joint).is_some_and(|m| m.contains(&k.motion)));
        }
        let assembly = &self.assembly;
        for set in &mut self.contact_sets {
            set.occurrences
                .retain(|p| path_component(assembly, ComponentUid::ROOT, &p.0).is_ok());
        }
    }
}

impl<K: Kernel> Document<K> {
    /// The motion studies in the order they were added.
    pub fn motion_studies(&self) -> &[MotionStudy] {
        &self.state.motion_studies
    }

    /// The contact sets in the order they were added.
    pub fn contact_sets(&self) -> &[ContactSet] {
        &self.state.contact_sets
    }

    fn study_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .motion_studies
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| invalid(format!("there is no motion study {name}")))
    }

    fn contact_index(&self, name: &str) -> Result<usize, ModelError> {
        self.state
            .contact_sets
            .iter()
            .position(|s| s.name == name)
            .ok_or_else(|| invalid(format!("there is no contact set {name}")))
    }

    /// The study by name.
    pub fn motion_study(&self, name: &str) -> Result<&MotionStudy, ModelError> {
        Ok(&self.state.motion_studies[self.study_index(name)?])
    }

    /// Why a study cannot be kept: its own checks, and each key's joint
    /// must be a joint (or as-built joint) with that free motion.
    fn check_study(&self, study: &MotionStudy) -> Result<(), ModelError> {
        study.check().map_err(invalid)?;
        for key in &study.keys {
            let entry = self.state.entry(key.joint).ok_or_else(|| {
                invalid(format!(
                    "{}: feature {} does not exist",
                    study.name, key.joint
                ))
            })?;
            let motions = joint_motions(&entry.def)
                .ok_or_else(|| invalid(format!("{}: {} is not a joint", study.name, entry.name)))?;
            if !motions.contains(&key.motion) {
                return Err(invalid(format!(
                    "{}: {} has no free motion {} (it has {})",
                    study.name,
                    entry.name,
                    key.motion,
                    if motions.is_empty() {
                        "none".to_owned()
                    } else {
                        motions
                            .iter()
                            .map(|m| m.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                )));
            }
        }
        Ok(())
    }

    /// Why a contact set cannot be kept: its own checks, and each path
    /// must lead from the root to an occurrence.
    fn check_contact_set(&self, set: &ContactSet) -> Result<(), ModelError> {
        set.check().map_err(invalid)?;
        for path in &set.occurrences {
            path_component(&self.state.assembly, ComponentUid::ROOT, &path.0)
                .map_err(|e| invalid(format!("{}: {e}", set.name)))?;
        }
        if set.occurrences.len() < 2 {
            return Err(invalid(format!(
                "{}: a contact set needs two occurrences or more",
                set.name
            )));
        }
        Ok(())
    }

    /// A free name: the stem and the next free number.
    fn free_name(&self, stem: &str, taken: impl Fn(&str) -> bool) -> String {
        (1..)
            .map(|n| format!("{stem}{n}"))
            .find(|n| !taken(n))
            .expect("a free name")
    }

    /// Adds a study; without a name it is `Study1`, `Study2`, ... Returns
    /// the name. One undo step that recomputes nothing.
    pub fn add_motion_study(&mut self, mut study: MotionStudy) -> Result<String, ModelError> {
        if study.name.trim().is_empty() {
            study.name = self.free_name("Study", |n| self.study_index(n).is_ok());
        }
        study.name = study.name.trim().to_owned();
        if self.study_index(&study.name).is_ok() {
            return Err(invalid(format!("a motion study {} exists", study.name)));
        }
        self.check_study(&study)?;
        let name = study.name.clone();
        let label = format!("Add {name}");
        self.apply_with(|state| {
            state.motion_studies.push(study);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a study: its name, steps and keys (all of them), each when
    /// given. A change that changes nothing is no undo step.
    pub fn edit_motion_study(
        &mut self,
        name: &str,
        new_name: Option<&str>,
        steps: Option<u32>,
        keys: Option<Vec<StudyKey>>,
    ) -> Result<(), ModelError> {
        let i = self.study_index(name)?;
        let mut study = self.state.motion_studies[i].clone();
        if let Some(new_name) = new_name.map(str::trim).filter(|n| *n != name) {
            if new_name.is_empty() {
                return Err(invalid("the motion study needs a name"));
            }
            if self.study_index(new_name).is_ok() {
                return Err(invalid(format!("a motion study {new_name} exists")));
            }
            study.name = new_name.to_owned();
        }
        if let Some(steps) = steps {
            study.steps = steps;
        }
        if let Some(keys) = keys {
            study.keys = keys;
        }
        if study == self.state.motion_studies[i] {
            return Ok(());
        }
        self.check_study(&study)?;
        let label = format!("Edit {name}");
        self.apply_with(|state| {
            state.motion_studies[i] = study;
            Ok((label, (), false))
        })
    }

    pub fn delete_motion_study(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.study_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.motion_studies.remove(i);
            Ok((label, (), false))
        })
    }

    /// Keys a joint's motion at a step of a study (a new point, or the
    /// point at that step changed): Add Key.
    pub fn set_study_key(
        &mut self,
        name: &str,
        joint: FeatureUid,
        motion: Motion,
        step: f64,
        value: f64,
    ) -> Result<(), ModelError> {
        let i = self.study_index(name)?;
        if !step.is_finite() || !value.is_finite() {
            return Err(invalid("a key's step and value must be finite"));
        }
        let mut study = self.state.motion_studies[i].clone();
        match study.key_mut(joint, motion) {
            Some(key) => key.set_point(step, value),
            None => study.keys.push(StudyKey {
                joint,
                motion,
                points: vec![(step, value)],
            }),
        }
        self.check_study(&study)?;
        let joint_name = self
            .state
            .entry(joint)
            .map_or_else(|| joint.to_string(), |e| e.name.clone());
        let label = format!("Key {joint_name} in {name}");
        self.apply_with(|state| {
            state.motion_studies[i] = study;
            Ok((label, (), false))
        })
    }

    /// Takes away a joint motion's point at a step of a study, or (without
    /// a step) its whole key.
    pub fn remove_study_key(
        &mut self,
        name: &str,
        joint: FeatureUid,
        motion: Motion,
        step: Option<f64>,
    ) -> Result<(), ModelError> {
        let i = self.study_index(name)?;
        let mut study = self.state.motion_studies[i].clone();
        let k = study
            .keys
            .iter()
            .position(|k| k.joint == joint && k.motion == motion)
            .ok_or_else(|| invalid(format!("{name} has no key of {joint} {motion}")))?;
        match step {
            None => {
                study.keys.remove(k);
            }
            Some(step) => {
                let key = &mut study.keys[k];
                let before = key.points.len();
                key.points.retain(|(s, _)| (s - step).abs() >= 1e-9);
                if key.points.len() == before {
                    return Err(invalid(format!(
                        "{name} has no point of {joint} {motion} at step {step}"
                    )));
                }
                if key.points.is_empty() {
                    study.keys.remove(k);
                }
            }
        }
        let label = format!("Remove Key in {name}");
        self.apply_with(|state| {
            state.motion_studies[i] = study;
            Ok((label, (), false))
        })
    }

    /// Adds a contact set; without a name it is `Contact1`, ... Returns the
    /// name.
    pub fn add_contact_set(&mut self, mut set: ContactSet) -> Result<String, ModelError> {
        if set.name.trim().is_empty() {
            set.name = self.free_name("Contact", |n| self.contact_index(n).is_ok());
        }
        set.name = set.name.trim().to_owned();
        if self.contact_index(&set.name).is_ok() {
            return Err(invalid(format!("a contact set {} exists", set.name)));
        }
        self.check_contact_set(&set)?;
        let name = set.name.clone();
        let label = format!("Add {name}");
        self.apply_with(|state| {
            state.contact_sets.push(set);
            Ok((label, (), false))
        })?;
        Ok(name)
    }

    /// Changes a contact set: its name, occurrences and whether it is
    /// suppressed, each when given.
    pub fn edit_contact_set(
        &mut self,
        name: &str,
        new_name: Option<&str>,
        occurrences: Option<Vec<OccurrencePath>>,
        suppressed: Option<bool>,
    ) -> Result<(), ModelError> {
        let i = self.contact_index(name)?;
        let mut set = self.state.contact_sets[i].clone();
        if let Some(new_name) = new_name.map(str::trim).filter(|n| *n != name) {
            if new_name.is_empty() {
                return Err(invalid("the contact set needs a name"));
            }
            if self.contact_index(new_name).is_ok() {
                return Err(invalid(format!("a contact set {new_name} exists")));
            }
            set.name = new_name.to_owned();
        }
        if let Some(occurrences) = occurrences {
            set.occurrences = occurrences;
        }
        if let Some(suppressed) = suppressed {
            set.suppressed = suppressed;
        }
        if set == self.state.contact_sets[i] {
            return Ok(());
        }
        self.check_contact_set(&set)?;
        let label = match suppressed {
            Some(true) if !self.state.contact_sets[i].suppressed => format!("Suppress {name}"),
            Some(false) if self.state.contact_sets[i].suppressed => format!("Unsuppress {name}"),
            _ => format!("Edit {name}"),
        };
        self.apply_with(|state| {
            state.contact_sets[i] = set;
            Ok((label, (), false))
        })
    }

    pub fn delete_contact_set(&mut self, name: &str) -> Result<(), ModelError> {
        let i = self.contact_index(name)?;
        let label = format!("Delete {name}");
        self.apply_with(|state| {
            state.contact_sets.remove(i);
            Ok((label, (), false))
        })
    }

    /// A value of a joint's motion in a command: a number (mm, radians) or
    /// an expression (`30 deg`, `crank_angle`), read as a turn's angle or a
    /// slide's length.
    pub fn motion_value(&self, input: &ValueInput, motion: Motion) -> Result<f64, ModelError> {
        match input {
            ValueInput::Number(v) if v.is_finite() => Ok(*v),
            ValueInput::Number(_) => Err(invalid(format!("{motion}: the value is not finite"))),
            ValueInput::Name(text) => {
                let params = &self.state.parameters;
                let context = params.context();
                let dims = if motion.is_rotation() {
                    Dims::ANGLE
                } else {
                    Dims::LENGTH
                };
                let unit = unit_for(dims, &context).unwrap_or(Unit::NONE);
                let parsed = Expr::parse(text).map_err(|e| invalid(format!("{motion}: {e}")))?;
                let value = parsed
                    .eval_as(params.table(), &context, unit)
                    .map_err(|e| invalid(format!("{motion}: {e}")))?;
                if value.value.is_finite() {
                    Ok(value.value)
                } else {
                    Err(invalid(format!("{motion}: {text} is not a finite value")))
                }
            }
        }
    }

    /// Where a joint's motion is at the marker: its value there, else the
    /// value it is driven to, else 0.
    pub fn joint_value(&self, joint: FeatureUid, motion: Motion) -> f64 {
        if let Some(v) = self
            .result
            .joints
            .joints
            .get(&joint)
            .and_then(|r| r.values.as_ref())
            .and_then(|v| v.get(&motion))
        {
            return *v;
        }
        let position = match self.state.entry(joint).map(|e| &e.def) {
            Some(FeatureDef::Joint(j)) => j.position.get(&motion).copied(),
            Some(FeatureDef::AsBuiltJoint(j)) => j.position.get(&motion).copied(),
            _ => None,
        };
        position
            .and_then(|id| self.state.parameters.value(id))
            .unwrap_or(0.0)
    }

    /// The values the given motions have at the marker.
    fn current_values(&self, of: &JointValues) -> JointValues {
        of.iter()
            .map(|(uid, motions)| {
                let values = motions
                    .keys()
                    .map(|m| (*m, self.joint_value(*uid, *m)))
                    .collect();
                (*uid, values)
            })
            .collect()
    }

    /// Whether any contact set has pairs to check at the marker.
    pub(crate) fn has_contact_pairs(&self) -> bool {
        self.state
            .contact_sets
            .iter()
            .any(|s| !s.suppressed && s.occurrences.len() > 1)
            && !ContactPairs::new(&self.kernel, &self.state, &self.result).is_empty()
    }

    /// Drives joints along `path` (their values at t from 0 to 1) in
    /// `base`, stopping at the first contact of the contact sets; the
    /// pairs that pass through each other at the start are left out.
    /// Without contact sets it goes the whole way without a computation.
    pub(crate) fn sweep_joints(
        &mut self,
        base: &DocState,
        path: &dyn Fn(f64) -> JointValues,
        samples: usize,
    ) -> Result<MotionOutcome, ModelError> {
        let pairs = ContactPairs::new(&self.kernel, base, &self.result);
        if pairs.is_empty() {
            return Ok(MotionOutcome {
                values: path(1.0),
                fraction: 1.0,
                contact: None,
            });
        }
        let failed = |e: crate::kernel::KernelError| invalid(format!("contact check: {e}"));
        let start = driven(base, &path(0.0))?;
        let at_start = self.compute(&start)?;
        let skip: BTreeSet<usize> = pairs
            .overlapping(&self.kernel, &at_start.placements, &BTreeSet::new())
            .map_err(failed)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let stop = sweep(
            samples,
            |t| -> Result<((), Option<ContactHit>), ModelError> {
                let state = driven(base, &path(t))?;
                let result = self.compute(&state)?;
                let deepest = pairs
                    .overlapping(&self.kernel, &result.placements, &skip)
                    .map_err(failed)?
                    .into_iter()
                    .max_by(|a, b| a.1.total_cmp(&b.1));
                Ok(((), deepest.map(|(p, v)| pairs.hit(p, v))))
            },
        )?;
        Ok(MotionOutcome {
            values: path(stop.fraction),
            fraction: stop.fraction,
            contact: stop.hit,
        })
    }

    /// Drives joints from where they are straight to `to`, stopping at the
    /// first contact.
    pub(crate) fn drive_to(&mut self, to: &JointValues) -> Result<MotionOutcome, ModelError> {
        let from = self.current_values(to);
        let samples = samples_for(&from, to);
        let base = self.state.clone();
        self.sweep_joints(&base, &|t| between(&from, to, t), samples)
    }

    /// A study's keyed values at a step, from where the joints are (`from`
    /// None) or along the study from another step.
    fn study_motion(
        &mut self,
        name: &str,
        step: f64,
        from: Option<f64>,
    ) -> Result<MotionOutcome, ModelError> {
        if !step.is_finite() || from.is_some_and(|f| !f.is_finite()) {
            return Err(invalid("the step must be finite"));
        }
        let study = self.motion_study(name)?.clone();
        let to = study.values_at(step);
        if to.is_empty() {
            return Err(invalid(format!("{name} has no keys")));
        }
        match from {
            None => self.drive_to(&to),
            Some(from) => {
                let start = study.values_at(from);
                let samples = samples_for(&start, &to)
                    .max(((step - from).abs().ceil() as usize).clamp(1, MAX_SAMPLES));
                let base = self.state.clone();
                self.sweep_joints(
                    &base,
                    &|t| study.values_at(from + (step - from) * t),
                    samples,
                )
            }
        }
    }

    /// Plays a study to a step: the keyed joints are driven from where they
    /// are to their values there, stopping at the first contact, and kept
    /// there. One undo step `Play Study1`.
    pub fn play_study(&mut self, name: &str, step: f64) -> Result<MotionOutcome, ModelError> {
        let outcome = self.study_motion(name, step, None)?;
        let values = outcome.values.clone();
        for uid in values.keys() {
            let entry = self.state.entry(*uid).expect("a joint of the study");
            self.state.editable(entry.component)?;
        }
        let label = format!("Play {name}");
        self.apply(|state| {
            *state = driven(state, &values)?;
            Ok((label, ()))
        })?;
        Ok(outcome)
    }

    /// The occurrences' placements in their parents at steps of a study
    /// (mitcad#1597, an animated USDZ export): at each step the keyed
    /// joints driven to the study's values there and solved as Play Study
    /// solves them, from the document's state; contact sets do not stop
    /// it. Nothing of the document changes but its cache: not its joints,
    /// its pose or its undo steps. A keyed joint that fails at a step is
    /// refused.
    pub fn study_placements(
        &mut self,
        name: &str,
        steps: &[f64],
    ) -> Result<Vec<BTreeMap<crate::ids::OccurrenceUid, crate::transform::Transform>>, ModelError>
    {
        let study = self.motion_study(name)?.clone();
        if study.keys.is_empty() {
            return Err(invalid(format!("{name} has no keys")));
        }
        let joints: BTreeSet<FeatureUid> = study.keys.iter().map(|k| k.joint).collect();
        let mut out = Vec::with_capacity(steps.len());
        for &step in steps {
            if !step.is_finite() {
                return Err(invalid("the step must be finite"));
            }
            let state = driven(&self.state, &study.values_at(step))?;
            let result = self.compute(&state)?;
            if let Some((uid, why)) = result.results.iter().find_map(|r| match &r.status {
                crate::recompute::FeatureStatus::Failed(why) if joints.contains(&r.uid) => {
                    Some((r.uid, why.clone()))
                }
                _ => None,
            }) {
                let joint = self
                    .state
                    .entry(uid)
                    .map_or_else(|| uid.to_string(), |e| e.name.clone());
                return Err(invalid(format!(
                    "{name} at step {step}: {joint} fails ({why})"
                )));
            }
            out.push(result.placements);
        }
        Ok(out)
    }

    /// A preview of a study at a step (the application's playback): the
    /// joints driven to the study's values there, from where they are
    /// (`from` None) or along the study from step `from`, stopping at the
    /// first contact. Changes nothing but the preview.
    pub fn preview_study(
        &mut self,
        name: &str,
        step: f64,
        from: Option<f64>,
    ) -> Result<(PreviewReport, MotionOutcome), ModelError> {
        let outcome = self.study_motion(name, step, from)?;
        let uid = *outcome.values.keys().next().expect("a keyed joint");
        let state = driven(&self.state, &outcome.values)?;
        let report = self.preview_state(state, uid)?;
        Ok((report, outcome))
    }

    /// A preview of a joint driven to values (Animate Joint), from where it
    /// is or from `from`, stopping at the first contact. Changes nothing but
    /// the preview.
    pub fn preview_drive(
        &mut self,
        joint: FeatureUid,
        to: &BTreeMap<Motion, f64>,
        from: Option<&BTreeMap<Motion, f64>>,
    ) -> Result<(PreviewReport, MotionOutcome), ModelError> {
        joint_def(&self.state, joint)?;
        let to: JointValues = BTreeMap::from([(joint, to.clone())]);
        let outcome = match from {
            None => self.drive_to(&to)?,
            Some(from) => {
                let from: JointValues = BTreeMap::from([(joint, from.clone())]);
                let samples = samples_for(&from, &to);
                let base = self.state.clone();
                self.sweep_joints(&base, &|t| between(&from, &to, t), samples)?
            }
        };
        let state = driven(&self.state, &outcome.values)?;
        let report = self.preview_state(state, joint)?;
        Ok((report, outcome))
    }
}
