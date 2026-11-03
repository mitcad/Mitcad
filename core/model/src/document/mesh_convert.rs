// SPDX-License-Identifier: MIT
//! Convert Mesh's Faceted solid (mitcad#1082) added: the mesh body it was
//! made from is hidden in the same undo step, and a conversion that fails
//! is refused with its reason, so that nothing changes and the mesh body
//! stays shown. Editing, suppressing or deleting the feature later leaves
//! the body's light bulb as it is.
//!
//! A Show When condition on the mesh body (mitcad#809) is kept
//! (mitcad#1157): while a faceted conversion of the body is in the
//! timeline (suppressed or not) the condition does not show the body, and
//! the body's warnings say why; once that feature is deleted, or edited to
//! the analytic mode, the body follows the condition again.

use std::cell::Cell;

use super::{Added, DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry, ValueInput};
use crate::ids::{BodyUid, ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::recompute::FeatureStatus;

/// The mesh body a new feature hides: a faceted Convert Mesh's.
pub(super) fn hidden_mesh(def: &FeatureDef<ValueInput>) -> Option<BodyUid> {
    match def {
        FeatureDef::MeshConvert(convert) => convert.replaced_mesh(),
        _ => None,
    }
}

impl DocState {
    /// The faceted Convert Mesh in the timeline, suppressed or not, that
    /// replaces mesh body `uid`: while there is one, the body's Show When
    /// condition does not show it (see the module docs).
    pub(super) fn mesh_replaced_by(&self, uid: BodyUid) -> Option<&FeatureEntry> {
        self.features.iter().map(|f| &**f).find(|f| match &f.def {
            FeatureDef::MeshConvert(convert) => convert.replaced_mesh() == Some(uid),
            _ => false,
        })
    }
}

impl<K: Kernel> Document<K> {
    /// Adds a feature that hides `mesh` (see the module docs) to
    /// `component`, at the marker or at `position` (at most the marker,
    /// which stays after the feature it was after).
    pub(super) fn add_hiding_mesh(
        &mut self,
        def: &FeatureDef<ValueInput>,
        name: Option<&str>,
        component: ComponentUid,
        position: Option<usize>,
        mesh: BodyUid,
    ) -> Result<Added, ModelError> {
        let marker = self.state.marker;
        let added_uid: Cell<Option<FeatureUid>> = Cell::new(None);
        self.apply_checked(
            |state| {
                if let Some(at) = position {
                    state.marker = at;
                }
                let added = state.add_feature_in(def, name, component)?;
                if position.is_some() {
                    state.marker = marker + 1;
                }
                let mut attributes = state
                    .body_attributes
                    .get(&mesh)
                    .cloned()
                    .unwrap_or_default();
                // A Show When condition stays (mitcad#1157): the feature
                // keeps it from showing the body (`mesh_replaced_by`).
                attributes.visible = false;
                state.body_attributes.insert(mesh, attributes);
                added_uid.set(Some(added.uid));
                Ok((format!("Add {}", added.name), added))
            },
            |state, result| {
                let Some(uid) = added_uid.get() else {
                    return Ok(());
                };
                match result.result(uid).map(|r| &r.status) {
                    Some(FeatureStatus::Failed(message)) => {
                        let name = state
                            .entry(uid)
                            .map_or_else(|| uid.to_string(), |f| f.name.clone());
                        Err(invalid(format!("{name}: {message}")))
                    }
                    _ => Ok(()),
                }
            },
        )
    }
}
