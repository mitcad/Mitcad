// SPDX-License-Identifier: MIT
//! Acknowledged warnings (mitcad#1935, GitHub mitcad/Mitcad#380): a
//! feature's warning the user has seen. The definition state keeps the
//! warning's text as it was acknowledged ([`DocState::acknowledged_warnings`],
//! saved with the feature as `"acknowledged_warning"`); while the feature
//! builds with that very text, the warning is acknowledged: the `timeline`
//! query says so (`"acknowledged": true`) and the application neither
//! paints it yellow nor counts it in the warnings badge. Its notices and
//! level stay as they are, so agents still read the warning.
//!
//! An acknowledgement lapses by itself: another text (a changed or added
//! warning) or a failure no longer matches it, and an edit that recomputes
//! forgets those that no longer match, in its own undo step, so that the
//! same warning coming back later is shown again. Errors cannot be
//! acknowledged. Acknowledging and showing again are undo steps of their
//! own; nothing is recomputed.

use super::{DocState, Document, ModelError, invalid};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;
use crate::recompute::{FeatureStatus, Recomputed};

/// The text a feature's warnings are acknowledged by: its warnings joined
/// as the queries' messages join them; None without warnings.
pub(crate) fn warning_text(warnings: &[String]) -> Option<String> {
    (!warnings.is_empty()).then(|| warnings.join("; "))
}

impl DocState {
    /// After an edit that recomputed (`result`): acknowledgements of
    /// features that now fail, or build with another warning or none, go.
    /// Features the recompute did not evaluate (suppressed, rolled back)
    /// keep theirs.
    pub(crate) fn settle_acknowledged_warnings<S>(&mut self, result: &Recomputed<S>) {
        if self.acknowledged_warnings.is_empty() {
            return;
        }
        for feature in &result.results {
            let Some(text) = self.acknowledged_warnings.get(&feature.uid) else {
                continue;
            };
            let lapsed = match &feature.status {
                FeatureStatus::Failed(_) => true,
                FeatureStatus::Ok => {
                    warning_text(feature.warnings()).as_deref() != Some(text.as_str())
                }
                FeatureStatus::Suppressed | FeatureStatus::RolledBack => false,
            };
            if lapsed {
                self.acknowledged_warnings.remove(&feature.uid);
            }
        }
    }
}

impl<K: Kernel> Document<K> {
    /// Whether the feature's warning is acknowledged: it built with
    /// warnings, and they read as they did when they were acknowledged.
    pub fn warning_acknowledged(&self, uid: FeatureUid) -> bool {
        let Some(text) = self.state.acknowledged_warnings.get(&uid) else {
            return false;
        };
        matches!(self.status(uid), Some(FeatureStatus::Ok))
            && warning_text(self.warnings(uid)).as_deref() == Some(text.as_str())
    }

    /// The text of the warning acknowledged for a feature, as kept and
    /// saved (also when it no longer matches); None when none is.
    pub fn acknowledged_warning(&self, uid: FeatureUid) -> Option<&str> {
        self.state
            .acknowledged_warnings
            .get(&uid)
            .map(String::as_str)
    }

    /// Acknowledges the warnings of features (`acknowledged`), or shows
    /// them again. One undo step (`Acknowledge Fillet1`, `Acknowledge 3
    /// Warnings`, `Unacknowledge Fillet1`) unless nothing changes; nothing
    /// is recomputed. Acknowledging a feature that failed, or one without a
    /// warning, is refused; one already acknowledged, or shown again without
    /// an acknowledgement, is left as it is.
    pub fn acknowledge_warnings(
        &mut self,
        uids: &[FeatureUid],
        acknowledged: bool,
    ) -> Result<(), ModelError> {
        let mut changes: Vec<(FeatureUid, Option<String>)> = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for &uid in uids {
            let position = self.state.require(uid)?;
            let name = self.state.features[position].name.clone();
            if changes.iter().any(|(done, _)| *done == uid) {
                continue;
            }
            let kept = self.state.acknowledged_warnings.get(&uid);
            if acknowledged {
                if matches!(self.status(uid), Some(FeatureStatus::Failed(_))) {
                    return Err(invalid(format!(
                        "{name} failed: an error cannot be acknowledged"
                    )));
                }
                let Some(text) = warning_text(self.warnings(uid))
                    .filter(|_| matches!(self.status(uid), Some(FeatureStatus::Ok)))
                else {
                    return Err(invalid(format!("{name} has no warning to acknowledge")));
                };
                if kept != Some(&text) {
                    changes.push((uid, Some(text)));
                    names.push(name);
                }
            } else if kept.is_some() {
                changes.push((uid, None));
                names.push(name);
            }
        }
        if changes.is_empty() {
            return Ok(());
        }
        let verb = if acknowledged {
            "Acknowledge"
        } else {
            "Unacknowledge"
        };
        let label = match names.as_slice() {
            [name] => format!("{verb} {name}"),
            _ => {
                let count = names.len();
                let noun = "Warnings";
                format!("{verb} {count} {noun}")
            }
        };
        self.apply_with(|state| {
            for (uid, text) in changes {
                match text {
                    Some(text) => state.acknowledged_warnings.insert(uid, text),
                    None => state.acknowledged_warnings.remove(&uid),
                };
            }
            Ok((label, (), false))
        })
    }
}
