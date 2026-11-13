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

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

/// Whether a feature has a light bulb of its own: sketches and
/// construction planes, axes, points and joint origins, intersection
/// curves (mitcad#1405) and 3D sketches (mitcad#1524).
fn has_visibility(def: &FeatureDef) -> bool {
    crate::joints::placed_geometry(def)
        || matches!(
            def,
            FeatureDef::IntersectionCurve(_) | FeatureDef::SpatialSketch(_)
        )
}

/// Whether a feature keeps a display size (mitcad#1748): construction
/// planes and axes, whose square and line the view draws at that size.
pub(crate) fn has_display_size(def: &FeatureDef) -> bool {
    matches!(
        def,
        FeatureDef::ConstructionPlane(_) | FeatureDef::ConstructionAxis(_)
    )
}

/// The largest display size kept, mm (a kilometre): larger values are
/// refused, so that a drag far out of the view stores no absurd size.
const MAX_DATUM_DISPLAY_SIZE: f64 = 1.0e6;

impl DocState {
    /// The sketches some feature uses (a profile, curves or points).
    /// Suppressed features and those after the timeline marker count too,
    /// so suppressing or rolling back neither shows nor hides a sketch.
    pub fn used_sketches(&self) -> BTreeSet<FeatureUid> {
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

    /// The sketches each feature uses, planar ones as
    /// [`DocState::used_sketches`] counts them and 3D sketches too (the
    /// route of a Pipe or a Sweep, mitcad#1527), in timeline order;
    /// features that use none are left out. The browser lists a feature
    /// under each of its sketches (mitcad#177), and editing the feature
    /// shows them for its time. A 3D sketch is not hidden when a feature
    /// uses it ([`DocState::used_sketches`] counts planar ones only): its
    /// route stays in view under the pipe it makes.
    pub fn sketches_used_by(&self) -> BTreeMap<FeatureUid, Vec<FeatureUid>> {
        let order: HashMap<FeatureUid, usize> = self
            .features
            .iter()
            .enumerate()
            .filter(|(_, f)| matches!(f.def, FeatureDef::Sketch(_) | FeatureDef::SpatialSketch(_)))
            .map(|(index, f)| (f.uid, index))
            .collect();
        let mut used = BTreeMap::new();
        if order.is_empty() {
            return used;
        }
        for entry in &self.features {
            let mut sketches: Vec<(usize, FeatureUid)> = entry
                .def
                .info()
                .references()
                .features
                .iter()
                .filter(|uid| **uid != entry.uid)
                .filter_map(|uid| order.get(uid).map(|index| (*index, *uid)))
                .collect();
            if !sketches.is_empty() {
                sketches.sort_unstable();
                used.insert(
                    entry.uid,
                    sketches.into_iter().map(|(_, uid)| uid).collect(),
                );
            }
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
    pub fn feature_shown(&self, entry: &FeatureEntry, used: &BTreeSet<FeatureUid>) -> Option<bool> {
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
        // Driven dimensions' references stay after their sketches
        // (mitcad#231).
        state
            .check_reference_order()
            .map_err(|e| invalid(format!("{name} cannot move there: {e}")))?;
        Ok(state)
    }
}

impl<K: Kernel> Document<K> {
    /// The light bulb set for a sketch or construction feature, by
    /// [`Document::set_feature_visible`] or a design file; None when it
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

    /// How large a construction plane or axis is drawn, mm, where its resize
    /// handles set it ([`Document::set_datum_display_size`]); None: the
    /// application's default size (mitcad#1748).
    pub fn datum_display_size(&self, uid: FeatureUid) -> Option<f64> {
        self.state.datum_display_sizes.get(&uid).copied()
    }

    /// Sets how large a construction plane (its square's side) or axis (its
    /// line's length) is drawn, mm; None goes back to the default size. An
    /// undo step (`Resize Plane1`) unless nothing changes; nothing is
    /// recomputed and the datum's geometry stays as it is: only the view's
    /// drawing follows (mitcad#1748). Kept and saved with the feature
    /// (`"display_size"`), forgotten when the feature is deleted.
    pub fn set_datum_display_size(
        &mut self,
        uid: FeatureUid,
        size: Option<f64>,
    ) -> Result<(), ModelError> {
        let position = self.state.require(uid)?;
        let entry = &self.state.features[position];
        if !has_display_size(&entry.def) {
            return Err(invalid(format!(
                "{} has no display size: only construction planes and axes are resized",
                entry.name
            )));
        }
        if let Some(size) = size
            && !(size.is_finite() && size > 0.0 && size <= MAX_DATUM_DISPLAY_SIZE)
        {
            return Err(invalid(format!(
                "the display size of {} must be greater than 0 and at most {MAX_DATUM_DISPLAY_SIZE} mm, got {size}",
                entry.name
            )));
        }
        if self.state.datum_display_sizes.get(&uid).copied() == size {
            return Ok(());
        }
        let label = format!("Resize {}", entry.name);
        self.apply_with(|state| {
            match size {
                Some(size) => state.datum_display_sizes.insert(uid, size),
                None => state.datum_display_sizes.remove(&uid),
            };
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
