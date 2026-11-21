// SPDX-License-Identifier: MIT
//! Drags leave BUILD's parts where they were placed (mitcad#2088;
//! `commands.md`, "BUILD: drags and travel limits"). The joint BUILD places
//! a part with keeps its driven values (its Offset, From and Rotate) under
//! a drag: they change only through Placement (`build_edit`). So a frame
//! stays put when something on it is dragged, and a drag of one of its
//! parts is refused. A part that moves on another (a carriage on its rail,
//! placed by its `linear.mate`) is the exception: its slide drags, within
//! its travel limits.

use std::collections::BTreeSet;

use super::BuildMeta;
use crate::document::{DocState, Document};
use crate::features::FeatureDef;
use crate::ids::{ComponentUid, FeatureUid, OccurrenceUid};
use crate::joints::JointKind;
use crate::kernel::Kernel;

/// Whether a BUILD part moves on another by its joint: placed by its
/// `linear.mate` (a carriage on its rail, a slider in a slot) with a joint
/// that slides.
pub(crate) fn slides_on_mate(state: &DocState, component: ComponentUid) -> bool {
    let Some(record) = state
        .assembly
        .component(component)
        .and_then(|c| c.build.as_ref())
    else {
        return false;
    };
    let Some(FeatureDef::Joint(joint)) = record.joint.and_then(|j| state.entry(j)).map(|e| &e.def)
    else {
        return false;
    };
    if !matches!(joint.kind, JointKind::Slider | JointKind::Cylindrical) {
        return false;
    }
    let row = record
        .part
        .and_then(|p| state.entry(p))
        .and_then(|e| match &e.def {
            FeatureDef::DerivedPart(d) => d.config.as_deref(),
            _ => None,
        });
    record
        .source
        .and_then(|id| state.library_sources.get(id))
        .and_then(|s| s.build.as_ref())
        .and_then(|b| BuildMeta::read_row(b, row).ok())
        .and_then(|meta| meta.linear)
        .is_some_and(|linear| linear.mate.is_some())
}

/// The BUILD part a component is and its joint, when that joint holds it
/// where it was placed (every part BUILD joined but a carriage).
fn held_joint(state: &DocState, component: ComponentUid) -> Option<FeatureUid> {
    let record = state.assembly.component(component)?.build.as_ref()?;
    let joint = record.joint?;
    let entry = state.entry(joint)?;
    let FeatureDef::Joint(def) = &entry.def else {
        return None;
    };
    // A rigid joint has nothing to hold; a carriage slides.
    if def.kind == JointKind::Rigid || slides_on_mate(state, component) {
        return None;
    }
    Some(joint)
}

impl<K: Kernel> Document<K> {
    /// The joints whose driven values a drag keeps (mitcad#2088): those
    /// BUILD placed its parts with, but a carriage's.
    pub(crate) fn build_held_joints(&self) -> BTreeSet<FeatureUid> {
        let state = self.state();
        state
            .assembly
            .components
            .iter()
            .filter(|c| c.build.is_some())
            .filter_map(|c| held_joint(state, c.uid))
            .collect()
    }

    /// Why a drag of `occurrence` is refused: it places a part BUILD joined
    /// where it is (any but a carriage) or grounded (a frame's first part),
    /// which Placement moves. A part without either (one freed of a removed
    /// part, mitcad#1811) drags.
    pub fn build_drag_refusal(&self, occurrence: OccurrenceUid) -> Option<String> {
        let state = self.state();
        let placed = state.assembly.occurrence(occurrence)?;
        let record = state.assembly.component(placed.component)?.build.as_ref()?;
        let joined = record.joint.is_some_and(|j| state.entry(j).is_some());
        if !joined && !placed.grounded {
            return None;
        }
        if joined && slides_on_mate(state, placed.component) {
            return None;
        }
        Some(format!(
            "{} stays where it was placed: move it with Placement",
            state.assembly.occurrence_name(occurrence)
        ))
    }

    /// Whether a BUILD part slides on another by its joint (a carriage on
    /// its rail, mitcad#2088): its slide drags within its travel limits.
    pub fn build_part_slides(&self, component: ComponentUid) -> bool {
        slides_on_mate(self.state(), component)
    }
}
