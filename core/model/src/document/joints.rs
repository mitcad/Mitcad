// SPDX-License-Identifier: MIT
//! Commands and views of joints (mitcad#55; the kinematics are in
//! `crate::joints`, the features in `features/joint.rs`): joints, as-built
//! joints that record where their occurrences are, rigid groups, the
//! joints' state and degrees of freedom at the marker, driving their
//! motions and dragging occurrences while they hold.

use std::collections::BTreeMap;

use super::{Added, Document, ModelError, invalid};
use crate::assembly::inverse;
use crate::datum::Vec3;
use crate::features::{AsBuiltJointDef, FeatureDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::{DofReport, Dragged, JointResult, Motion, dof_report, drag, path_component};
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// What a joint or as-built joint did in the last recompute; None when
    /// it failed, is suppressed or after the marker.
    pub fn joint_result(&self, uid: FeatureUid) -> Option<&JointResult> {
        self.result.joints.joints.get(&uid)
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
        let entry = self
            .state
            .entry(uid)
            .ok_or_else(|| invalid(format!("feature {uid} does not exist")))?;
        if !matches!(
            entry.def,
            FeatureDef::Joint(_) | FeatureDef::AsBuiltJoint(_)
        ) {
            return Err(invalid(format!("{} is not a joint", entry.name)));
        }
        let params = &self.state.parameters;
        Ok(entry
            .def
            .map_params(&mut |_, id| Ok::<_, ()>(ValueInput::Name(params.name(*id))))
            .expect("names never fail"))
    }

    /// Drives a joint's free motions: each value (mm, angles; parameters and
    /// expressions allowed) is stored as the joint's position and the
    /// motion is held there; None takes the position away (the rest value,
    /// else free). One undo step; returns the parameters created.
    pub fn drive_joint(
        &mut self,
        uid: FeatureUid,
        values: &BTreeMap<Motion, Option<ValueInput>>,
    ) -> Result<Vec<String>, ModelError> {
        let mut def = self.joint_input(uid)?;
        let position = match &mut def {
            FeatureDef::Joint(j) => &mut j.position,
            FeatureDef::AsBuiltJoint(j) => &mut j.position,
            _ => unreachable!("a joint"),
        };
        for (motion, value) in values {
            match value {
                Some(v) => position.insert(*motion, v.clone()),
                None => position.remove(motion),
            };
        }
        let entry = self.state.entry(uid).expect("found");
        self.state.editable(entry.component)?;
        let name = entry.name.clone();
        self.apply(|state| {
            let created = state.edit_feature(uid, &def)?;
            Ok((format!("Drive {name}"), created))
        })
    }

    /// Where dragging `occurrence` would put the occurrences of its parent
    /// component: `grab` (in the parent's coordinates, the occurrence's
    /// origin when None) pulled toward `target` while the joints in effect
    /// at the marker hold, the other occurrences moving as little as
    /// possible. Driven motions follow like free ones. Changes nothing.
    pub fn joint_drag(
        &self,
        occurrence: OccurrenceUid,
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        self.state.require_occurrence(occurrence)?;
        let grab = grab.unwrap_or_else(|| {
            self.placement(occurrence)
                .map_or([0.0; 3], |p| p.translation)
        });
        if !grab.iter().chain(&target).all(|v| v.is_finite()) {
            return Err(invalid("the points must be finite"));
        }
        drag(
            &self.state.assembly,
            &self.result.joints,
            &self.result.placements,
            occurrence,
            grab,
            target,
        )
        .map_err(invalid)
    }

    /// Drags `occurrence` (see [`Document::joint_drag`]) and keeps the
    /// result: the moved occurrences' own placements become where the drag
    /// put them, and the joints' driven motions are driven to their new
    /// values. One undo step.
    pub fn drag_occurrence(
        &mut self,
        occurrence: OccurrenceUid,
        grab: Option<Vec3>,
        target: Vec3,
    ) -> Result<Dragged, ModelError> {
        let dragged = self.joint_drag(occurrence, grab, target)?;
        let mut edits = Vec::new();
        for joint in &self.result.joints.active {
            let Some(values) = dragged.values.get(&joint.uid) else {
                continue;
            };
            let moved: BTreeMap<Motion, Option<ValueInput>> = joint
                .motions
                .iter()
                .filter_map(|spec| {
                    let target = spec.target?;
                    let v = *values.get(&spec.motion)?;
                    ((v - target).abs() > 1e-12 * (1.0 + v.abs()))
                        .then_some((spec.motion, Some(ValueInput::Number(v))))
                })
                .collect();
            if moved.is_empty() {
                continue;
            }
            let mut def = self.joint_input(joint.uid)?;
            let position = match &mut def {
                FeatureDef::Joint(j) => &mut j.position,
                FeatureDef::AsBuiltJoint(j) => &mut j.position,
                _ => unreachable!("a joint"),
            };
            for (m, v) in moved {
                position.insert(m, v.expect("a value"));
            }
            edits.push((joint.uid, def));
        }
        let name = self.state.assembly.occurrence_name(occurrence);
        let placements = dragged.placements.clone();
        self.apply(|state| {
            for (o, transform) in &placements {
                let by = state.positioning_features(*o);
                if !by.is_empty() {
                    return Err(invalid(format!(
                        "{} is placed by {} in the timeline; capture the position instead",
                        state.assembly.occurrence_name(*o),
                        by.join(", ")
                    )));
                }
                state.assembly.occurrence_mut(*o).expect("exists").transform = *transform;
            }
            for (uid, def) in &edits {
                state.edit_feature(*uid, def)?;
            }
            Ok((format!("Drag {name}"), ()))
        })?;
        Ok(dragged)
    }
}
