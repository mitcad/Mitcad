// SPDX-License-Identifier: MIT
//! Several features' definitions replaced in one command (mitcad#2495): a
//! command that changes a feature and the features after it that follow it
//! (a standard box's rules, its outline and creases sketches and its Folds)
//! recomputes the timeline once, not once for each edit. Each edit is the
//! one [`Document::edit_feature`] makes, checked against the features as
//! the earlier edits of the same command left them.

use super::{Document, ModelError, invalid};
use crate::features::{FeatureDef, ValueInput};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

impl<K: Kernel> Document<K> {
    /// Replaces the definitions of `edits`' features, in their order, in
    /// one command named `label` (one undo step, one recompute). Returns
    /// the parameters the edits created. An edit that changes a feature's
    /// operation so that the later features using it would move with it
    /// (mitcad#2185) is refused: edit that feature on its own. An error
    /// from any edit leaves the document as it was.
    pub fn edit_features(
        &mut self,
        label: &str,
        edits: &[(FeatureUid, FeatureDef<ValueInput>)],
    ) -> Result<Vec<String>, ModelError> {
        let mut prepared = Vec::with_capacity(edits.len());
        for (uid, def) in edits {
            let Some(entry) = self.state.entry(*uid) else {
                return Err(invalid(format!("no feature {uid}")));
            };
            self.state.editable(entry.component)?;
            self.check_profiles(def)?;
            // Where an assembly cut lies against what it machines
            // (mitcad#2131), as a single edit has it.
            let def = self.with_machined_frames(def, Some(*uid)).into_owned();
            if self.operation_follow(*uid, &def).is_some() {
                return Err(invalid(format!(
                    "the edit of {uid} changes its operation: edit it on its own"
                )));
            }
            let (made, beside) = self.made_and_beside(*uid);
            prepared.push((*uid, def, made, beside));
        }
        self.apply(|state| {
            let mut created = Vec::new();
            for (uid, def, made, beside) in &prepared {
                let (names, _) = state.edit_feature_following(*uid, def, None)?;
                state.place_made_beside(*uid, *made, *beside);
                created.extend(names);
            }
            Ok((label.to_owned(), created))
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;
    use crate::ids::FeatureUid;
    use crate::test_support::def;
    use crate::testing::MockKernel;
    use serde_json::json;

    fn sketch(radius: f64) -> serde_json::Value {
        json!({"type": "sketch", "plane": "xy",
               "entities": [{"id": "p1", "type": "point", "at": [0, 0]},
                            {"id": "c2", "type": "circle", "center": "p1", "radius": radius}]})
    }

    fn extrude(sketch: FeatureUid, distance: f64) -> serde_json::Value {
        json!({"type": "extrude",
               "profiles": [{"sketch": sketch.to_string(), "region": "r{c2}"}],
               "extent": {"type": "distance", "distance": distance},
               "operation": "new_body"})
    }

    fn has_value(doc: &Document<MockKernel>, value: f64) -> bool {
        doc.parameters()
            .iter()
            .any(|p| (p.value() - value).abs() < 1e-9)
    }

    /// A sketch and its extrusion edited together: one recompute, one undo
    /// step under the command's name, both edits made; undo takes both
    /// back. A refused edit in the batch leaves the document as it was.
    #[test]
    fn edits_in_one_command_recompute_once() {
        let mut doc = Document::new(MockKernel::default());
        let s = doc.add_feature(&def(sketch(5.0)), None).unwrap().uid;
        let e = doc.add_feature(&def(extrude(s, 10.0)), None).unwrap().uid;
        let (recomputes, depth) = (doc.recompute_count(), doc.undo_depth());
        doc.edit_features(
            "Edit Both",
            &[(s, def(sketch(6.0))), (e, def(extrude(s, 20.0)))],
        )
        .expect("both edited");
        assert_eq!(doc.recompute_count(), recomputes + 1, "one recompute");
        assert_eq!(doc.undo_depth(), depth + 1, "one undo step");
        assert_eq!(doc.undo_label(), Some("Edit Both"));
        assert!(has_value(&doc, 20.0) && !has_value(&doc, 10.0));
        doc.undo();
        assert!(has_value(&doc, 10.0) && !has_value(&doc, 20.0));
        // The second edit makes the extrusion a sketch: refused, nothing
        // of the first kept.
        let (recomputes, depth) = (doc.recompute_count(), doc.undo_depth());
        let refused = doc.edit_features(
            "Edit Both",
            &[(e, def(extrude(s, 30.0))), (e, def(sketch(7.0)))],
        );
        assert!(refused.is_err(), "{refused:?}");
        assert_eq!(doc.recompute_count(), recomputes);
        assert_eq!(doc.undo_depth(), depth);
        assert!(has_value(&doc, 10.0) && !has_value(&doc, 30.0));
    }
}
