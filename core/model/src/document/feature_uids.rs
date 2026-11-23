// SPDX-License-Identifier: MIT
//! A feature added under a uid of the caller's (mitcad#2245): a script
//! exported from a design (`export_script`) adds each feature with the uid
//! it has in the design, so that the topological names of later features
//! (`F4:side(c1[c2,c3])`), body uids (`F4.b0`) and references to it
//! (`"F4"`) hold in the replay as they are, also where the design's uids
//! have gaps (deleted features) or its timeline order is not the order the
//! features were made in (features inserted or moved).
//!
//! A feature's uid is otherwise the next number ([`DocState::next_uid`]);
//! here the next number is the uid asked for while the feature is added and
//! afterwards the larger of the old next number and one past that uid, so
//! counting on never gives a uid twice. The state the add's undo step keeps
//! has the next number it had before.
//!
//! Such a feature may also come with what it remembers of the pieces of
//! its splits (mitcad#408, `features/pieces.rs`), as the design file keeps
//! it (mitcad#2268): a split's first evaluation numbers the pieces in
//! geometric order, and an edit after it keeps their ids, so a replay that
//! builds the edited state at once would number the pieces otherwise than
//! the design does. With the memory its first evaluation matches the
//! pieces with the remembered ones and gives each the design's id.

use std::sync::Arc;

use super::{DocState, Document, ModelError, invalid};
use crate::features::{PieceGroup, SplitPieces};
use crate::ids::{FeatureUid, leaves_reserve};
use crate::kernel::Kernel;

impl DocState {
    /// What a feature remembers of the pieces of its splits (mitcad#408):
    /// None when it remembers nothing.
    pub fn split_pieces_of(&self, feature: FeatureUid) -> Option<&[PieceGroup]> {
        self.split_pieces.get(&feature).map(|groups| &groups[..])
    }
}

/// Puts back what `feature` remembered before (`before`) into `pieces`.
fn put_back(pieces: &mut SplitPieces, feature: FeatureUid, before: Option<Arc<Vec<PieceGroup>>>) {
    match before {
        Some(groups) => {
            pieces.insert(feature, groups);
        }
        None => {
            pieces.remove(&feature);
        }
    }
}

impl<K: Kernel> Document<K> {
    /// Runs `add` (which adds one feature, as `add_feature_to` or
    /// `add_feature_at` do) so that the feature it adds gets `uid`. Refused
    /// when a feature has that uid or it is in the reserve of id numbers
    /// (mitcad#944); when `add` fails nothing changes.
    pub fn with_feature_uid<T>(
        &mut self,
        uid: FeatureUid,
        add: impl FnOnce(&mut Self) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        if self.state.entry(uid).is_some() {
            return Err(invalid(format!(
                "a feature has the uid {uid} already; give a free uid or none"
            )));
        }
        if uid.0 == 0 || !leaves_reserve(uid.0) {
            return Err(invalid(format!(
                "{uid} is not a feature uid a design can use"
            )));
        }
        let before = self.state.next_uid;
        let depth = self.undo.len();
        self.state.next_uid = uid.0;
        let result = add(self);
        match &result {
            Ok(_) => {
                self.state.next_uid = before.max(uid.0 + 1);
                // The state before the add, which undo brings back, counts
                // on from where it did.
                if self.undo.len() > depth
                    && let Some(step) = self.undo.last_mut()
                {
                    step.state.next_uid = before;
                }
            }
            Err(_) => self.state.next_uid = before,
        }
        result
    }

    /// Runs `add` (which adds the feature `uid` and nothing else, inside
    /// [`Document::with_feature_uid`]) so that the feature's first
    /// evaluation finds `pieces` as what it remembers of its splits'
    /// pieces (mitcad#2268), as a design file read gives it. Each piece is
    /// the body split or one of the feature's own bodies (`<uid>.b<n>`), or
    /// the memory is refused. The state the add's undo step keeps has the
    /// memory it had before; when `add` fails nothing changes.
    pub fn with_feature_pieces<T>(
        &mut self,
        uid: FeatureUid,
        pieces: Vec<PieceGroup>,
        add: impl FnOnce(&mut Self) -> Result<T, ModelError>,
    ) -> Result<T, ModelError> {
        if pieces.is_empty() {
            return add(self);
        }
        for group in &pieces {
            if let Some(other) = group
                .pieces
                .iter()
                .find(|p| p.body != group.body && p.body.feature != uid)
            {
                return Err(invalid(format!(
                    "pieces: {} is neither the body split ({}) nor a body of {uid}",
                    other.body, group.body
                )));
            }
        }
        let before = self.state.split_pieces.get(&uid).cloned();
        let depth = self.undo.len();
        self.state.split_pieces.insert(uid, Arc::new(pieces));
        let result = add(self);
        match &result {
            Ok(_) => {
                if self.undo.len() > depth
                    && let Some(step) = self.undo.last_mut()
                {
                    put_back(&mut step.state.split_pieces, uid, before);
                }
            }
            Err(_) => put_back(&mut self.state.split_pieces, uid, before),
        }
        result
    }
}
