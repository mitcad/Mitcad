// SPDX-License-Identifier: MIT
//! Motion links of the document (mitcad#466; the feature in
//! `features/motion_link.rs`, recompute in `crate::motion_links`): adding
//! one with the offset that keeps the joints where they are, its result,
//! and letting go of the positions of motions a driven motion is linked to
//! (the last driven one drives).

use std::collections::BTreeSet;

use super::{Added, DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, MotionLinkDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid};
use crate::joints::Motion;
use crate::kernel::Kernel;
use crate::motion_links::{LinkGeo, LinkResult, coupled, relation};

impl DocState {
    /// Takes away the positions of the motions linked to `driven` (through
    /// links, not themselves driven): the driven ones then drive them.
    pub(crate) fn release_linked(
        &mut self,
        driven: &[(FeatureUid, Motion)],
    ) -> Result<(), ModelError> {
        let mut seen: BTreeSet<(FeatureUid, Motion)> = driven.iter().copied().collect();
        let mut queue: Vec<(FeatureUid, Motion)> = driven.to_vec();
        let mut release: Vec<(FeatureUid, Motion)> = Vec::new();
        while let Some((joint, motion)) = queue.pop() {
            for (_, other, m) in coupled(self, joint, motion) {
                if seen.insert((other, m)) {
                    release.push((other, m));
                    queue.push((other, m));
                }
            }
        }
        // A joint inside a flexible occurrence that a driven one is linked
        // to (mitcad#1793): its position for that occurrence goes.
        for f in self.features.iter().filter(|f| !f.suppressed) {
            let FeatureDef::MotionLink(l) = &f.def else {
                continue;
            };
            for (this, other) in [(&l.a, &l.b), (&l.b, &l.a)] {
                let Some(o) = other.occurrence else {
                    continue;
                };
                if this.occurrence.is_none() && seen.contains(&(this.joint, this.motion)) {
                    let flex = self
                        .assembly
                        .occurrence_mut(o)
                        .and_then(|o| o.flexible.as_mut());
                    if let Some(position) = flex.and_then(|f| f.position.get_mut(&other.joint)) {
                        position.remove(&other.motion);
                    }
                }
            }
        }
        for (joint, motion) in release {
            let Ok(mut def) = super::motion::joint_def(self, joint) else {
                continue;
            };
            let position = match &mut def {
                FeatureDef::Joint(j) => &mut j.position,
                FeatureDef::AsBuiltJoint(j) => &mut j.position,
                _ => continue,
            };
            if position.remove(&motion).is_some() {
                self.edit_feature(joint, &def)?;
            }
        }
        Ok(())
    }
}

impl<K: Kernel> Document<K> {
    /// What a motion link did in the last recompute; None when it failed,
    /// is suppressed or after the marker.
    pub fn link_result(&self, uid: FeatureUid) -> Option<&LinkResult> {
        self.result.joints.link_results.get(&uid)
    }

    /// The offset of a link (without one) that keeps its joints where they
    /// are at the marker: b's value less what the relation gives for a's.
    pub fn link_offset_as_built(
        &self,
        def: &MotionLinkDef<ValueInput>,
        component: ComponentUid,
    ) -> Result<f64, ModelError> {
        // The definition with its values as parameters, in a copy.
        let mut trial = self.state.clone();
        let def = MotionLinkDef {
            offset: None,
            ..def.clone()
        };
        let added = trial.add_feature_in(&FeatureDef::MotionLink(def), None, component)?;
        let Some(FeatureDef::MotionLink(link)) = trial.entry(added.uid).map(|e| e.def.clone())
        else {
            unreachable!("the link just added");
        };
        let active = &self.result.joints.active;
        let joint = |m: &crate::features::LinkedMotion, key: &str| {
            active
                .iter()
                .find(|j| {
                    j.uid == m.joint
                        && j.row.is_none()
                        && j.under.as_slice() == m.occurrence.as_slice()
                })
                .ok_or_else(|| {
                    invalid(format!(
                        "{key}: {} is not in effect at the marker",
                        self.feature(m.joint)
                            .map_or_else(|| m.joint.to_string(), |f| f.name.clone())
                    ))
                })
        };
        let (ja, jb) = (joint(&link.a, "a")?, joint(&link.b, "b")?);
        let mut geo = LinkGeo {
            assembly: &self.state.assembly,
            component,
            placements: &self.result.placements,
            flexed: &self.result.joints.flexed,
            resolver: |_| self.resolver(),
        };
        let (relation, _) =
            relation(&mut geo, &trial.parameters, &link, (ja, jb)).map_err(invalid)?;
        // Inside a flexible occurrence, its value there (mitcad#1793).
        let value = |m: &crate::features::LinkedMotion| match m.occurrence {
            Some(o) => self
                .flexible_joint_values(o)
                .get(&m.joint)
                .and_then(|v| v.get(&m.motion).copied())
                .unwrap_or(0.0),
            None => self.joint_value(m.joint, m.motion),
        };
        let va = value(&link.a);
        let vb = value(&link.b);
        Ok(vb - relation.follow(va))
    }

    /// Adds a motion link to `component` (the active one when None). With
    /// `as_built` (the default for every coupling but a cam) and no offset
    /// given, its offset keeps the joints where they are at the marker; a
    /// cam's follower otherwise moves to touch the cam.
    pub fn add_motion_link(
        &mut self,
        def: &MotionLinkDef<ValueInput>,
        as_built: Option<bool>,
        name: Option<&str>,
        component: Option<ComponentUid>,
    ) -> Result<Added, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        let mut def = def.clone();
        let cam = matches!(def.coupling, crate::features::Coupling::Cam { .. });
        if def.offset.is_none() && as_built.unwrap_or(!cam) {
            let offset = self.link_offset_as_built(&def, component)?;
            def.offset = Some(ValueInput::Number(if offset.abs() < 1e-12 {
                0.0
            } else {
                offset
            }));
        }
        self.add_feature_to(&FeatureDef::MotionLink(def), name, Some(component))
    }
}
