// SPDX-License-Identifier: MIT
//! Import notes (mitcad#170): what an importer said about a feature it
//! made, kept in the definition state and the design file so that a saved
//! import still shows it. A fallback's base feature says which items of
//! the file it stands in for and why, an item that came in partly says what
//! was left out, and a placeholder (`placeholder`, suppressed) stands where
//! the file had an item that was suppressed or failed there.
//!
//! A note is information (mitcad#1040): a feature with only a note keeps
//! the `ok` status and has the `info` level (a suppressed placeholder stays
//! `suppressed`); the note comes first in its message, before the warnings
//! of its evaluation, and is its last notice ([`feature_notices`]). A command that changes
//! the feature's definition or one of its dimensions clears the note when
//! the feature then builds (does not fail); undo brings it back with the
//! state.
//!
//! Also here: the join tolerance an importer sets (mitcad#298,
//! [`Document::set_join_tolerance`]).

use std::collections::HashMap;
use std::sync::Arc;

use super::{Added, DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry, PlaceholderDef, ValueInput};
use crate::ids::{ComponentUid, FeatureUid};
use crate::kernel::Kernel;
use crate::notice::{Notice, NoticeLevel};
use crate::recompute::{FeatureStatus, Recomputed};

impl DocState {
    /// After an edit that recomputed (`result`): the notes of features that
    /// are gone go, and so do those of features whose definition or
    /// dimensions (the parameters they own) the edit changed (from
    /// `before`) and that did not fail.
    pub(crate) fn settle_import_notes<S>(&mut self, before: &DocState, result: &Recomputed<S>) {
        if self.import_notes.is_empty() {
            return;
        }
        let noted = |features: &[Arc<FeatureEntry>]| -> HashMap<FeatureUid, Arc<FeatureEntry>> {
            features
                .iter()
                .filter(|f| self.import_notes.contains_key(&f.uid))
                .map(|f| (f.uid, f.clone()))
                .collect()
        };
        let now = noted(&self.features);
        let was = noted(&before.features);
        // The expressions of the parameters each noted feature owns (its
        // dimensions), where the edit changed parameters.
        let owned = |state: &DocState| -> HashMap<FeatureUid, Vec<String>> {
            let mut owned: HashMap<FeatureUid, Vec<String>> = HashMap::new();
            if self.parameters != before.parameters {
                for p in state.parameters.iter() {
                    if let Some(owner) = state.parameters.owner(p.id())
                        && self.import_notes.contains_key(&owner)
                    {
                        owned
                            .entry(owner)
                            .or_default()
                            .push(p.expression().to_owned());
                    }
                }
            }
            owned
        };
        let (dims_now, dims_was) = (owned(self), owned(before));
        self.import_notes.retain(|uid, _| {
            let Some(entry) = now.get(uid) else {
                return false;
            };
            let Some(old) = was.get(uid) else {
                return true;
            };
            if (Arc::ptr_eq(entry, old) || entry.def == old.def)
                && dims_now.get(uid) == dims_was.get(uid)
            {
                return true;
            }
            matches!(
                result.result(*uid).map(|r| &r.status),
                Some(FeatureStatus::Failed(_))
            )
        });
    }
}

impl<K: Kernel> Document<K> {
    /// What the importer that made the feature said about it (mitcad#170).
    pub fn import_note(&self, uid: FeatureUid) -> Option<&str> {
        self.state.import_notes.get(&uid).map(String::as_str)
    }

    /// Every feature's import note, by feature.
    pub fn import_notes(&self) -> impl Iterator<Item = (FeatureUid, &str)> {
        self.state
            .import_notes
            .iter()
            .map(|(uid, note)| (*uid, note.as_str()))
    }

    /// Sets features' import notes (an empty one removes the note); one
    /// undo step that recomputes nothing. An importer calls it once its
    /// report is complete.
    pub fn set_import_notes(&mut self, notes: &[(FeatureUid, String)]) -> Result<(), ModelError> {
        if notes.is_empty() {
            return Ok(());
        }
        self.apply_with(|state| {
            for (uid, note) in notes {
                state.require(*uid)?;
                let note = note.trim();
                if note.is_empty() {
                    state.import_notes.remove(uid);
                } else {
                    state.import_notes.insert(*uid, note.to_owned());
                }
            }
            Ok(("Import notes".to_owned(), (), false))
        })
    }

    /// The distance (mm) within which the features' joins still fuse where
    /// they fail exactly (mitcad#298, [`Document::set_join_tolerance`]);
    /// None for exact joins.
    pub fn join_tolerance(&self) -> Option<f64> {
        self.state.join_tolerance
    }

    /// Sets the join tolerance: with Some, a join (of an extrusion, a base
    /// feature, a pattern's copies …) whose tool touches no target still
    /// joins the targets within that distance, and one whose result is
    /// invalid is retried with a fuzzy value up to it
    /// ([`Kernel::join_within`]); a join that works exactly stays exact.
    /// An importer whose bodies match a file's only within its geometric
    /// tolerance sets it; the design file keeps it. One undo step; the
    /// joins recompute. A value that is not finite and positive is refused.
    pub fn set_join_tolerance(&mut self, tolerance: Option<f64>) -> Result<(), ModelError> {
        if tolerance.is_some_and(|t| !t.is_finite() || t <= 0.0) {
            return Err(invalid("the join tolerance must be a positive distance"));
        }
        if self.state.join_tolerance == tolerance {
            return Ok(());
        }
        self.apply(|state| {
            state.join_tolerance = tolerance;
            Ok(("Join tolerance".to_owned(), ()))
        })
    }

    /// Adds a suppressed placeholder at the marker for an item an import
    /// did not bring in because it was suppressed or failed in the file:
    /// named `name` (features may share a name, mitcad#1107; else
    /// `Placeholder<n>`), in `component` (the active one when None), with
    /// `note` (the file's state) as its import note.
    pub fn add_import_placeholder(
        &mut self,
        name: Option<&str>,
        item_type: &str,
        note: &str,
        component: Option<ComponentUid>,
    ) -> Result<Added, ModelError> {
        let component = component.unwrap_or(self.state.assembly.active);
        self.state.editable(component)?;
        let note = note.trim();
        if note.is_empty() {
            return Err(invalid("a placeholder needs a note"));
        }
        self.apply(|state| {
            let name = name.map(str::trim).filter(|n| !n.is_empty());
            let def: FeatureDef<ValueInput> =
                FeatureDef::Placeholder(PlaceholderDef::new(item_type));
            let added = state.add_feature_in(&def, name, component)?;
            let position = state.require(added.uid)?;
            Arc::make_mut(&mut state.features[position]).suppressed = true;
            state.import_notes.insert(added.uid, note.to_owned());
            Ok((format!("Add {}", added.name), added))
        })
    }
}

/// A feature's status as the queries give it: `warning` for one that
/// succeeded with warnings. An import note alone is information
/// (mitcad#1040): the feature stays `ok`, its `level` is `info`.
pub fn status_text(
    status: Option<&FeatureStatus>,
    warnings: &[String],
    _note: Option<&str>,
) -> &'static str {
    super::status_text(status, warnings)
}

/// A feature's notices (mitcad#1040), the one place that decides its
/// level: the error of a failed feature, a warning per warning of its
/// evaluation (one for the references a change in one sketch renamed,
/// mitcad#1334), its evaluation's infos (mitcad#1106: a fillet built 0.1 %
/// smaller than asked) and the importer's note as information, in that
/// order. States (suppressed, rolled back, pending) are not severities: a
/// suppressed placeholder has only its note.
pub fn feature_notices(
    status: Option<&FeatureStatus>,
    warnings: &[String],
    infos: &[String],
    note: Option<&str>,
    uid: FeatureUid,
) -> Vec<Notice> {
    feature_notices_placed(status, warnings, infos, note, uid, &[])
}

/// [`feature_notices`] with the places of its warnings, by message
/// (mitcad#1327: the faces of a fillet's corner that folds back on itself,
/// its farthest point). Each notice's message is in plain words: the
/// topological names in it go into its places too
/// ([`Notice::new`](crate::notice::Notice::new)).
pub fn feature_notices_placed(
    status: Option<&FeatureStatus>,
    warnings: &[String],
    infos: &[String],
    note: Option<&str>,
    uid: FeatureUid,
    places: &[(String, Vec<crate::notice::NoticePlace>)],
) -> Vec<Notice> {
    let mut notices = Vec::new();
    // A failure as a person reads it, its places with it (mitcad#1612).
    if let Some(error) = status.and_then(FeatureStatus::failure) {
        notices.push(error.notice().with_target(uid));
    }
    if !matches!(status, Some(FeatureStatus::Failed(_))) {
        // References a change in one sketch renamed are one notice of
        // their kind (mitcad#1334: "2 edges found again after a change in
        // Sketch1"); the warnings keep one each.
        notices.extend(crate::notice::merge_found_again(
            warnings
                .iter()
                .map(|w| {
                    let notice = Notice::warning(w.as_str()).with_target(uid);
                    match places.iter().find(|(message, _)| message == w) {
                        Some((_, places)) => notice.with_places(places.iter().cloned()),
                        None => notice,
                    }
                })
                .collect(),
        ));
        notices.extend(
            infos
                .iter()
                .map(|i| Notice::info(i.as_str()).with_target(uid)),
        );
    }
    if let Some(note) = note {
        notices.push(Notice::info(note).with_target(uid));
    }
    notices
}

/// A feature's level (mitcad#1040): `error` when it failed, `warning` when
/// it built with warnings, `info` when it has only infos (mitcad#1106) or
/// an import note; None for a feature with nothing to say.
pub fn feature_level(
    status: Option<&FeatureStatus>,
    warnings: &[String],
    infos: &[String],
    note: Option<&str>,
) -> Option<NoticeLevel> {
    match status {
        Some(FeatureStatus::Failed(_)) => Some(NoticeLevel::Error),
        Some(FeatureStatus::Ok) if !warnings.is_empty() => Some(NoticeLevel::Warning),
        Some(FeatureStatus::Ok) if !infos.is_empty() => Some(NoticeLevel::Info),
        _ if note.is_some() => Some(NoticeLevel::Info),
        _ => None,
    }
}

/// A failed feature's error, or its import note and warnings joined.
pub fn status_message(
    status: Option<&FeatureStatus>,
    warnings: &[String],
    note: Option<&str>,
) -> Option<String> {
    match (note, super::status_message(status, warnings)) {
        (_, Some(error)) if status.and_then(FeatureStatus::error).is_some() => Some(error),
        (Some(note), Some(warnings)) => Some(format!("{note}; {warnings}")),
        (Some(note), None) => Some(note.to_owned()),
        (None, message) => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const F: FeatureUid = FeatureUid(4);

    #[test]
    fn a_feature_s_level_is_its_worst_notice() {
        let failed = FeatureStatus::failed("no result");
        let warned = ["lost 2 of 5 edges".to_owned()];
        let note = Some("Stands in for Fillet3");
        // Failed: an error, also with warnings or a note.
        assert_eq!(
            feature_level(Some(&failed), &warned, &[], note),
            Some(NoticeLevel::Error)
        );
        assert_eq!(
            feature_notices(Some(&failed), &warned, &[], note, F),
            vec![
                Notice::error("no result").with_target("F4"),
                Notice::info("Stands in for Fillet3").with_target("F4"),
            ]
        );
        // Built with warnings: a warning; a note stays information.
        let ok = FeatureStatus::Ok;
        assert_eq!(
            feature_level(Some(&ok), &warned, &[], note),
            Some(NoticeLevel::Warning)
        );
        assert_eq!(
            feature_notices(Some(&ok), &warned, &[], None, F),
            vec![Notice::warning("lost 2 of 5 edges").with_target("F4")]
        );
        assert_eq!(status_text(Some(&ok), &warned, None), "warning");
        // Only a note: information, and the status stays ok.
        assert_eq!(
            feature_level(Some(&ok), &[], &[], note),
            Some(NoticeLevel::Info)
        );
        assert_eq!(status_text(Some(&ok), &[], note), "ok");
        // Nothing to say.
        assert_eq!(feature_level(Some(&ok), &[], &[], None), None);
        assert!(feature_notices(Some(&ok), &[], &[], None, F).is_empty());
        // States are not severities: a suppressed placeholder has only its
        // note, a rolled back or pending feature nothing.
        assert_eq!(
            feature_level(Some(&FeatureStatus::Suppressed), &[], &[], note),
            Some(NoticeLevel::Info)
        );
        assert_eq!(
            status_text(Some(&FeatureStatus::Suppressed), &[], note),
            "suppressed"
        );
        assert_eq!(
            feature_level(Some(&FeatureStatus::RolledBack), &[], &[], None),
            None
        );
        assert_eq!(feature_level(None, &[], &[], None), None);
    }

    #[test]
    fn infos_are_information() {
        // A feature that built with infos only (mitcad#1106): information,
        // its status stays ok; with warnings too, a warning.
        let ok = FeatureStatus::Ok;
        let infos = ["built 0.1 % smaller".to_owned()];
        let warned = ["built as rings".to_owned()];
        assert_eq!(
            feature_level(Some(&ok), &[], &infos, None),
            Some(NoticeLevel::Info)
        );
        assert_eq!(status_text(Some(&ok), &[], None), "ok");
        assert_eq!(
            feature_notices(Some(&ok), &[], &infos, None, F),
            vec![Notice::info("built 0.1 % smaller").with_target("F4")]
        );
        assert_eq!(
            feature_level(Some(&ok), &warned, &infos, None),
            Some(NoticeLevel::Warning)
        );
        assert_eq!(
            feature_notices(Some(&ok), &warned, &infos, Some("note"), F),
            vec![
                Notice::warning("built as rings").with_target("F4"),
                Notice::info("built 0.1 % smaller").with_target("F4"),
                Notice::info("note").with_target("F4"),
            ]
        );
        // A failed feature has no infos to tell.
        let failed = FeatureStatus::failed("no result");
        assert_eq!(
            feature_notices(Some(&failed), &[], &infos, None, F),
            vec![Notice::error("no result").with_target("F4")]
        );
    }

    #[test]
    fn renamed_references_are_one_notice_of_their_kind() {
        // Two edges a sketch change renamed (mitcad#1334): one notice with
        // all four names as its places; the warnings keep one each.
        let ok = FeatureStatus::Ok;
        let warned = [
            "edge E{F2:a|F2:b} found as E{F2:a|F2:c} after a change in Sketch1".to_owned(),
            "edge E{F2:d|F2:e} found as E{F2:d|F2:f} after a change in Sketch1".to_owned(),
        ];
        let notices = feature_notices(Some(&ok), &warned, &[], None, F);
        assert_eq!(notices.len(), 1);
        assert_eq!(
            notices[0].message,
            "2 edges found again after a change in Sketch1"
        );
        assert_eq!(notices[0].target.as_deref(), Some("F4"));
        assert_eq!(notices[0].places.len(), 4);
        assert_eq!(
            feature_level(Some(&ok), &warned, &[], None),
            Some(NoticeLevel::Warning)
        );
    }
}
