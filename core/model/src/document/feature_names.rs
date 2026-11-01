// SPDX-License-Identifier: MIT
//! Features by name (mitcad#1107). Feature names are not unique: a sketch
//! and the extrude made from it may both be named `Holder`. The model's own
//! references go by uid; a command, query or script step that takes a
//! feature "by uid or name" looks it up here, so every such place treats a
//! shared name the same way:
//!
//! - the uid of an existing feature is that feature;
//! - a name one feature has is that feature;
//! - a name several features have is refused with their uids and kinds
//!   ("Holder names several features: F1 (sketch), F2 (extrude); give the
//!   uid"), unless the place takes only one kind of feature and exactly one
//!   of them is of that kind (a `sketch` field and one sketch named Holder).
//!
//! An empty name is still refused, and default names (`Extrude3`) still
//! skip the names in use; other names (bodies, components, parameters,
//! annotations) stay unique.

use super::{DocState, Document, ModelError, invalid};
use crate::features::{FeatureDef, FeatureEntry};
use crate::ids::FeatureUid;
use crate::kernel::Kernel;

/// Which features a place takes, for a name several features share: the
/// one of this kind is taken when it is the only one.
pub type FeatureKind<'a> = &'a dyn Fn(&FeatureDef) -> bool;

impl DocState {
    /// A feature by its uid or its name (see the module): Ok(None) when no
    /// feature has that uid or name, an error for a name several features
    /// have (none of them, or more than one, of `kind`).
    pub fn feature_by_text(
        &self,
        text: &str,
        kind: Option<FeatureKind<'_>>,
    ) -> Result<Option<&FeatureEntry>, ModelError> {
        let text = text.trim();
        if let Ok(uid) = text.parse::<FeatureUid>()
            && let Some(entry) = self.entry(uid)
        {
            return Ok(Some(entry));
        }
        let named: Vec<&FeatureEntry> = self
            .features
            .iter()
            .map(|f| &**f)
            .filter(|f| f.name == text || f.name.trim() == text)
            .collect();
        match named.as_slice() {
            [] => return Ok(None),
            [one] => return Ok(Some(*one)),
            _ => {}
        }
        if let Some(kind) = kind {
            let mut of_kind = named.iter().filter(|f| kind(&f.def));
            if let (Some(one), None) = (of_kind.next(), of_kind.next()) {
                return Ok(Some(*one));
            }
        }
        Err(invalid(shared_name(text, &named)))
    }
}

impl<K: Kernel> Document<K> {
    /// A feature by its uid or its name ([`DocState::feature_by_text`]).
    pub fn feature_by_text(
        &self,
        text: &str,
        kind: Option<FeatureKind<'_>>,
    ) -> Result<Option<&FeatureEntry>, ModelError> {
        self.state.feature_by_text(text, kind)
    }
}

/// The refusal of a name several features have: each with its uid and
/// kind, in timeline order.
fn shared_name(name: &str, named: &[&FeatureEntry]) -> String {
    let listed: Vec<String> = named
        .iter()
        .map(|f| format!("{} ({})", f.uid, f.def.type_name()))
        .collect();
    format!(
        "{name} names several features: {}; give the uid",
        listed.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use crate::features::FeatureDef;
    use crate::features::SketchPlane;
    use crate::test_support::block;

    fn is_sketch(def: &FeatureDef) -> bool {
        matches!(def, FeatureDef::Sketch(_))
    }

    fn is_extrude(def: &FeatureDef) -> bool {
        matches!(def, FeatureDef::Extrude(_))
    }

    fn is_fillet(def: &FeatureDef) -> bool {
        matches!(def, FeatureDef::Fillet(_))
    }

    #[test]
    fn a_sketch_and_its_extrude_share_a_name_and_lookups_name_the_uids() {
        let mut b = block();
        b.doc.rename_feature(b.sketch, "Holder").unwrap();
        b.doc.rename_feature(b.extrude, "Holder").unwrap();
        assert_eq!(b.doc.feature(b.sketch).unwrap().name, "Holder");
        assert_eq!(b.doc.feature(b.extrude).unwrap().name, "Holder");
        // An empty name is still refused.
        assert!(b.doc.rename_feature(b.extrude, " ").is_err());

        let error = b.doc.feature_by_text("Holder", None).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Holder names several features: F1 (sketch), F2 (extrude); give the uid"
        );
        // A place that takes one kind finds the one of that kind.
        let sketch = |text: &str| {
            b.doc
                .feature_by_text(text, Some(&is_sketch))
                .map(|found| found.map(|entry| entry.uid))
        };
        assert_eq!(sketch("Holder").unwrap(), Some(b.sketch));
        assert_eq!(sketch(" Holder ").unwrap(), Some(b.sketch));
        let extrude = b.doc.feature_by_text("Holder", Some(&is_extrude)).unwrap();
        assert_eq!(extrude.map(|f| f.uid), Some(b.extrude));
        // None of that kind: refused with both uids.
        let fillet = b.doc.feature_by_text("Holder", Some(&is_fillet));
        assert!(
            fillet
                .unwrap_err()
                .to_string()
                .contains("F1 (sketch), F2 (extrude)")
        );
        // Uids are always one feature; an unknown name is none.
        let by_uid = b.doc.feature_by_text("F2", Some(&is_sketch)).unwrap();
        assert_eq!(by_uid.map(|f| f.uid), Some(b.extrude));
        assert!(b.doc.feature_by_text("Nothing", None).unwrap().is_none());
        assert!(b.doc.feature_by_text("F9", None).unwrap().is_none());

        // Two sketches of the name: a sketch field cannot tell them apart.
        let second = b.doc.add_sketch(SketchPlane::Xz).unwrap().uid;
        b.doc.rename_feature(second, "Holder").unwrap();
        let error = b
            .doc
            .feature_by_text("Holder", Some(&is_sketch))
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "Holder names several features: F1 (sketch), F2 (extrude), {second} (sketch); \
                 give the uid"
            )
        );
    }

    #[test]
    fn a_feature_named_once_is_found_by_its_name() {
        let b = block();
        let found = b.doc.feature_by_text("Extrude1", Some(&is_sketch)).unwrap();
        // The only feature of the name, whatever the place takes: the
        // place itself says it is not of its kind.
        assert_eq!(found.map(|f| f.uid), Some(b.extrude));
    }

    #[test]
    fn default_names_still_skip_the_names_in_use() {
        let mut b = block();
        b.doc.rename_feature(b.extrude, "Sketch2").unwrap();
        let sketch = b.doc.add_sketch(SketchPlane::Xy).unwrap();
        assert_eq!(sketch.name, "Sketch3");
    }
}
