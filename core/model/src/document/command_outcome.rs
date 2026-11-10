// SPDX-License-Identifier: MIT
//! The features in error after a command, and whether the command itself
//! added or edited them (mitcad#401). A failed feature does not reject a
//! command, so a client needs to tell "the feature I made fails" from "some
//! feature elsewhere in the timeline fails".

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use serde::Serialize;

use super::Document;
use crate::features::FeatureEntry;
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

/// The timeline before a command, to compare with the one after it.
pub struct TimelineBefore {
    /// Every feature's entry, by uid (shared with the state: no copies).
    features: BTreeMap<FeatureUid, Arc<FeatureEntry>>,
    /// The features in error.
    failed: BTreeSet<FeatureUid>,
}

/// A feature in error after a command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FailedFeature {
    pub uid: FeatureUid,
    pub name: String,
    /// The error for agents: with the names and coordinates (mitcad#1612).
    pub error: String,
    /// What a person reads of it, where that is not `error`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// The places it is about: faces, edges, vertices, points.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub places: Vec<crate::notice::NoticePlace>,
    /// The command added the feature or changed it: its definition, its
    /// suppression or its component, or the command names it as the one
    /// it edits (`edit_feature`, `suppress_feature`, `reorder_feature`, a
    /// sketch command's `sketch`).
    pub edited: bool,
    /// It did not fail before the command.
    pub new: bool,
}

impl<K: Kernel> Document<K> {
    /// The timeline as it is now, before a command.
    pub fn timeline_before(&self) -> TimelineBefore {
        TimelineBefore {
            features: self
                .state
                .features
                .iter()
                .map(|f| (f.uid, f.clone()))
                .collect(),
            failed: self.failed_uids().collect(),
        }
    }

    fn failed_uids(&self) -> impl Iterator<Item = FeatureUid> + '_ {
        self.result
            .results
            .iter()
            .filter(|r| r.status.error().is_some())
            .map(|r| r.uid)
    }

    /// The features in error now, in timeline order, compared with the
    /// timeline `before` a command; `target` is the feature the command
    /// names as the one it edits, if any.
    pub fn failed_features(
        &self,
        before: &TimelineBefore,
        target: Option<FeatureUid>,
    ) -> Vec<FailedFeature> {
        if self.failed_uids().next().is_none() {
            return Vec::new();
        }
        let current: BTreeMap<FeatureUid, &Arc<FeatureEntry>> =
            self.state.features.iter().map(|f| (f.uid, f)).collect();
        self.result
            .results
            .iter()
            .filter_map(|r| {
                let error = r.status.failure()?;
                let entry = *current.get(&r.uid)?;
                let edited = Some(r.uid) == target
                    || before.features.get(&r.uid).is_none_or(|old| {
                        // Entries the command did not touch are shared.
                        !Arc::ptr_eq(old, entry)
                            && (old.def != entry.def
                                || old.suppressed != entry.suppressed
                                || old.component != entry.component)
                    });
                Some(FailedFeature {
                    uid: r.uid,
                    name: entry.name.clone(),
                    error: error.agent_text().to_owned(),
                    message: (error.message != error.agent_text()).then(|| error.message.clone()),
                    places: error.places.clone(),
                    edited,
                    new: !before.failed.contains(&r.uid),
                })
            })
            .collect()
    }
}
