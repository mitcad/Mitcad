// SPDX-License-Identifier: MIT
//! A feature added at a timeline position before the marker (mitcad#403):
//! one undo step and one recompute of the features after it, instead of
//! moving the marker there, adding the feature and moving it back. The
//! marker stays after the feature it was after.

use super::{Added, Document, ModelError, invalid};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::ComponentUid;
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// Adds a feature to a component (the active one when None) at
    /// `position`: the number of features before it, at most the marker.
    /// At the marker it is [`Document::add_feature_to`]. A refused feature
    /// leaves the document as it was.
    pub fn add_feature_at(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        component: Option<ComponentUid>,
        position: usize,
    ) -> Result<Added, ModelError> {
        let marker = self.state.marker;
        if position > marker {
            return Err(invalid(format!(
                "timeline position {position} is after the timeline marker ({marker}); a feature \
                 is added at or before the marker"
            )));
        }
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        // Faceted Convert Mesh hides its mesh body (mitcad#1082).
        if let Some(mesh) = super::mesh_convert::hidden_mesh(def) {
            return self.add_hiding_mesh(def, name, component, Some(position), mesh);
        }
        self.check_profiles(def)?;
        // A new component of a component that takes none goes beside it
        // (mitcad#292).
        let beside = self.component_parent_beside(component);
        // An Envelope's body takes its source's appearance (mitcad#1271).
        let appearance = self.made_appearance(def, component);
        self.apply(|state| {
            state.marker = position;
            let added = state.add_feature_in(def, name, component)?;
            state.marker = marker + 1;
            state.place_made_beside(added.uid, None, beside);
            state.take_appearance(added.uid, appearance.as_deref());
            Ok((format!("Add {}", added.name), added))
        })
    }
}
