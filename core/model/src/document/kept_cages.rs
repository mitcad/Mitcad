// SPDX-License-Identifier: MIT
//! The control cage kept with an imported body (an `.f3d` Form body,
//! mitcad#1365) opened as a Form's cage (mitcad#1757).
//!
//! The import brings a Form body in as the faces the file stores and keeps
//! its cage beside it as data ([`crate::features::ControlCage`]). Where
//! that cage maps onto a Form's ([`crate::features::ControlCage::form_cage`]),
//! `form.create` with the primitive `kept_cage` makes an editable Form of
//! it; the imported body stays as it is.

use super::{Document, ModelError, invalid};
use crate::features::FeatureDef;
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::notice::said;
use mitcad_subdiv::Cage;

impl<K: Kernel> Document<K> {
    /// The control cage kept with a body of a base feature, as a Form's
    /// cage in millimetres in the coordinates of the body's component; Err
    /// in words when the body keeps none or its cage does not map onto a
    /// Form's (which says why).
    pub fn kept_form_cage(&self, body: BodyUid) -> Result<Cage, ModelError> {
        let none = || {
            invalid(said(
                "the body has no control cage kept with it",
                format!("body {body} keeps no control cage"),
            ))
        };
        let entry = self.feature(body.feature).ok_or_else(none)?;
        let FeatureDef::Base(base) = &entry.def else {
            return Err(none());
        };
        let kept = base
            .body_numbered(body.index)
            .and_then(|b| b.cage.as_deref())
            .ok_or_else(none)?;
        kept.form_cage().map_err(invalid)
    }
}

#[cfg(test)]
mod tests {
    use crate::exchange::{BaseInput, ImportBody};
    use crate::features::{ControlCage, FeatureDef, FormDef, Operation};
    use crate::testing::{MockKernel, MockShape};
    use crate::{Document, FeatureStatus, ValueInput};

    /// An imported sheet of 10 by 10 mm whose kept cage is one square face.
    fn sheet_with_cage(cage: Option<ControlCage>) -> (Document<MockKernel>, crate::BodyUid) {
        let mut doc = Document::new(MockKernel::default());
        let imported = doc
            .add_base_feature(BaseInput {
                cages: vec![cage.map(Box::new)],
                ..BaseInput::new(vec![ImportBody {
                    name: Some("Form1".to_owned()),
                    color: None,
                    shape: MockShape::imported("sheet", 1),
                    face_colors: Vec::new(),
                }])
            })
            .unwrap();
        (doc, imported.bodies[0].uid)
    }

    fn square() -> ControlCage {
        serde_json::from_value(serde_json::json!({
            "source": "TSpline.s.tsm",
            "degree": 3,
            "vertices": [
                {"position": [0.0, 0.0, 0.0]},
                {"position": [10.0, 0.0, 0.0]},
                {"position": [10.0, 10.0, 0.0]},
                {"position": [0.0, 10.0, 0.0]}
            ],
            "faces": [[0, 1, 2, 3]],
            "edges": [
                {"vertices": [0, 1], "boundary": true},
                {"vertices": [1, 2], "boundary": true},
                {"vertices": [2, 3], "boundary": true},
                {"vertices": [3, 0], "boundary": true}
            ]
        }))
        .unwrap()
    }

    #[test]
    fn a_kept_cage_opens_as_an_editable_form() {
        let (mut doc, body) = sheet_with_cage(Some(square()));
        let cage = doc.kept_form_cage(body).unwrap();
        assert_eq!(cage.vertices.len(), 4);
        assert_eq!(cage.faces[0].vertices, [1, 2, 3, 4]);
        assert!(!cage.full().closed());
        let def: FormDef<ValueInput> = FormDef::new(cage, Operation::NewBody, Vec::new());
        let added = doc.add_feature(&FeatureDef::Form(def), None).unwrap();
        assert!(
            !matches!(doc.status(added.uid), Some(FeatureStatus::Failed(_))),
            "{:?}",
            doc.status(added.uid)
        );
        // The Form's cage is the kept one, editable as any Form's.
        let moved = doc.edit_form(added.uid, "Drag", |cage| {
            mitcad_subdiv::edit::set_positions(cage, &[(3, [10.0, 10.0, 5.0])])
        });
        assert!(moved.is_ok(), "{moved:?}");
        let cage = &doc.form_def(added.uid).unwrap().cage;
        assert_eq!(cage.position(3), Some([10.0, 10.0, 5.0]));
    }

    #[test]
    fn a_body_without_a_kept_cage_says_so() {
        let (doc, body) = sheet_with_cage(None);
        let error = doc.kept_form_cage(body).unwrap_err().to_string();
        assert!(error.contains("no control cage"), "{error}");
        let mut rational = square();
        rational.vertices[0].weight = 2.0;
        let (doc, body) = sheet_with_cage(Some(rational));
        let error = doc.kept_form_cage(body).unwrap_err().to_string();
        assert!(error.contains("weight"), "{error}");
    }
}
