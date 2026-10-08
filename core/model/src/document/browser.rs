// SPDX-License-Identifier: MIT
//! What the application's browser and timeline need beyond the timeline
//! commands (U3): the visibility of sketches and construction geometry
//! (the light bulbs), the features that depend on one (the delete
//! dialog), and whether a feature can move to a timeline index (dragging).
//!
//! A sketch is hidden while a feature uses it and shown while none does;
//! construction geometry is shown. A light bulb set by hand wins until the
//! sketch gets its first consumer or loses its last one: the command that
//! does it forgets the setting, in the same undo step, so the sketch hides
//! or shows again (mitcad#7).

use std::collections::BTreeSet;
use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

/// Whether a feature has a light bulb of its own: sketches and
/// construction planes, axes, points and joint origins.
fn has_visibility(def: &FeatureDef) -> bool {
    crate::joints::placed_geometry(def)
}

impl DocState {
    /// The sketches some feature uses (a profile, curves or points).
    /// Suppressed features and those after the timeline marker count too,
    /// so suppressing or rolling back neither shows nor hides a sketch.
    pub(crate) fn used_sketches(&self) -> BTreeSet<FeatureUid> {
        let sketches: BTreeSet<FeatureUid> = self
            .features
            .iter()
            .filter(|f| matches!(f.def, FeatureDef::Sketch(_)))
            .map(|f| f.uid)
            .collect();
        let mut used = BTreeSet::new();
        if sketches.is_empty() {
            return used;
        }
        for entry in &self.features {
            let references = entry.def.info().references().features;
            used.extend(references.intersection(&sketches).copied());
        }
        used
    }

    /// Whether a feature is shown by default: a sketch while no feature
    /// uses it (`used`: [`DocState::used_sketches`]), construction geometry
    /// always.
    fn shown_by_default(entry: &FeatureEntry, used: &BTreeSet<FeatureUid>) -> bool {
        !(matches!(entry.def, FeatureDef::Sketch(_)) && used.contains(&entry.uid))
    }

    /// Whether a sketch or construction feature is shown: its light bulb
    /// when set, else the default; None for features without one.
    pub(crate) fn feature_shown(
        &self,
        entry: &FeatureEntry,
        used: &BTreeSet<FeatureUid>,
    ) -> Option<bool> {
        has_visibility(&entry.def).then(|| {
            self.feature_visibility
                .get(&entry.uid)
                .copied()
                .unwrap_or_else(|| Self::shown_by_default(entry, used))
        })
    }

    /// After a command, the state that follows `before`: forgets the light
    /// bulbs of the sketches that got their first consumer or lost their
    /// last one, so the default hides or shows them in the command's own
    /// undo step.
    pub(crate) fn follow_sketch_use(&mut self, before: &DocState) {
        let same = self.features.len() == before.features.len()
            && self
                .features
                .iter()
                .zip(&before.features)
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if same || self.feature_visibility.is_empty() {
            return;
        }
        let was = before.used_sketches();
        let now = self.used_sketches();
        for uid in was.symmetric_difference(&now) {
            self.feature_visibility.remove(uid);
        }
    }

    /// The state with a feature moved to a timeline index, or why it
    /// cannot move there: every feature must still come after the features
    /// it refers to.
    pub(crate) fn moved(&self, uid: FeatureUid, index: usize) -> Result<DocState, ModelError> {
        let mut state = self.clone();
        let position = state.require(uid)?;
        if index >= state.features.len() {
            return Err(invalid(format!(
                "timeline index {index} is out of range (0 to {})",
                state.features.len() - 1
            )));
        }
        let entry = state.features.remove(position);
        let name = entry.name.clone();
        state.features.insert(index, entry);
        // Out of its timeline group, or into the one it lands in (P9).
        state.leave_group_for_move(uid);
        state.normalize_groups();
        for (i, entry) in state.features.iter().enumerate() {
            state
                .check_feature(i, entry)
                .map_err(|e| invalid(format!("{name} cannot move there: {}: {e}", entry.name)))?;
        }
        Ok(state)
    }
}

impl<K: Kernel> Document<K> {
    /// The light bulb set for a sketch or construction feature, by
    /// [`Document::set_feature_visible`] or a project file; None when it
    /// follows the default (see [`Document::feature_shown`]). Only a
    /// setting that differs from the default is kept, and a sketch's is
    /// forgotten when it gets its first consumer or loses its last one.
    pub fn feature_visible(&self, uid: FeatureUid) -> Option<bool> {
        self.state.feature_visibility.get(&uid).copied()
    }

    /// Whether a sketch or construction feature is shown: its light bulb
    /// ([`Document::feature_visible`]) when set, else the default: a sketch
    /// is hidden while a feature uses it (suppressed and rolled back
    /// features too), construction geometry is shown. None for other
    /// features and unknown uids.
    pub fn feature_shown(&self, uid: FeatureUid) -> Option<bool> {
        let entry = self.state.entry(uid)?;
        self.state.feature_shown(entry, &self.state.used_sketches())
    }

    /// Shows or hides a sketch or a construction feature. An undo step
    /// (`Show Sketch1`, `Hide Plane1`) unless nothing changes; nothing is
    /// recomputed. The light bulb is kept only when it differs from the
    /// default ([`Document::feature_shown`]): showing a sketch no feature
    /// uses, or hiding a used one that was shown by hand, forgets it.
    pub fn set_feature_visible(
        &mut self,
        uid: FeatureUid,
        visible: bool,
    ) -> Result<(), ModelError> {
        let position = self.state.require(uid)?;
        let entry = &self.state.features[position];
        if !has_visibility(&entry.def) {
            return Err(invalid(format!(
                "{} has no visibility of its own; show or hide its bodies",
                entry.name
            )));
        }
        let used = self.state.used_sketches();
        if self.state.feature_shown(entry, &used) == Some(visible) {
            return Ok(());
        }
        let default = DocState::shown_by_default(entry, &used);
        let verb = if visible { "Show" } else { "Hide" };
        let label = format!("{verb} {}", entry.name);
        self.apply_with(|state| {
            if visible == default {
                state.feature_visibility.remove(&uid);
            } else {
                state.feature_visibility.insert(uid, visible);
            }
            Ok((label, (), false))
        })
    }

    /// The features that refer to a feature, directly or through others,
    /// in timeline order: what deleting it with its dependents deletes too.
    pub fn feature_dependents(&self, uid: FeatureUid) -> Result<Vec<FeatureUid>, ModelError> {
        self.state.require(uid)?;
        Ok(self.state.dependents(&BTreeSet::from([uid])))
    }

    /// Whether a feature can move to a timeline index; the reason when not.
    pub fn check_reorder(&self, uid: FeatureUid, index: usize) -> Result<(), ModelError> {
        self.state.moved(uid, index).map(|_| ())
    }
}
