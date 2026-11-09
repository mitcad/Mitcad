// SPDX-License-Identifier: MIT
//! Binder reporting follows the uses that survived the replay, not the
//! support's type or a candidate that was subsequently rolled back.

use std::collections::BTreeMap;

use mitcad_freecad::Object;
use mitcad_model::{Document, FeatureDef, FeatureUid, Kernel};
use serde_json::{Value, json};

use super::report::{FeatureReport, ObjectOutcome};
use super::sketches::is_binder;
use super::{Importer, is_component, is_group, is_sketch};

/// Only the definition's provenance is retained, not a second copy of
/// its sketch entities, constraints or dimensions.
pub(super) struct BinderUse {
    consumer: FeatureUid,
    kind: std::mem::Discriminant<FeatureDef>,
    provenance: Value,
    binder_sketch: Option<(FeatureUid, Value)>,
    how: String,
}

impl BinderUse {
    fn retained<K: Kernel>(&self, doc: &Document<K>) -> bool {
        doc.feature(self.consumer).is_some_and(|feature| {
            std::mem::discriminant(&feature.def) == self.kind
                && provenance(&feature.def) == self.provenance
                && self.binder_sketch.as_ref().is_none_or(|(uid, expected)| {
                    doc.feature(*uid).is_some_and(|bound| {
                        matches!(bound.def, FeatureDef::Sketch(_))
                            && provenance(&bound.def) == *expected
                    })
                })
        })
    }
}

/// Whether an extrusion's extent goes up to a face of a body (not up to a
/// fixed plane in its place).
fn names_face(def: &FeatureDef) -> bool {
    use mitcad_model::features::{Extent, ExtentObject, Side};
    let face = |o: &ExtentObject| matches!(o, ExtentObject::Face { .. });
    let side = |s: &Side| matches!(s, Side::ToObject { object, .. } if face(object));
    match def {
        FeatureDef::Extrude(d) => match &d.extent {
            Extent::ToObject { object, .. } => face(object),
            Extent::TwoSides { side1, side2, .. } => side(side1) || side(side2),
            _ => false,
        },
        _ => false,
    }
}

fn provenance(def: &FeatureDef) -> Value {
    match def {
        FeatureDef::Sketch(sketch) => json!({
            "plane": sketch.plane,
            "plane_link": sketch.plane_link,
            "projections": sketch.projections,
        }),
        _ => json!({"features": def.references().features}),
    }
}

impl<K: Kernel> Importer<'_, '_, K> {
    pub(super) fn carried_binder_use(
        &mut self,
        binder: &str,
        consumer: &str,
        property: &str,
        uid: FeatureUid,
        how: String,
    ) {
        let Some(feature) = self.doc.feature(uid) else {
            return;
        };
        let carried = BinderUse {
            consumer: uid,
            kind: std::mem::discriminant(&feature.def),
            provenance: provenance(&feature.def),
            binder_sketch: self.replay.sketches.get(binder).and_then(|sketch| {
                let feature = self.doc.feature(sketch.uid)?;
                matches!(feature.def, FeatureDef::Sketch(_))
                    .then(|| (sketch.uid, provenance(&feature.def)))
            }),
            how,
        };
        self.replay
            .binder_uses
            .entry(binder.to_owned())
            .or_default()
            .insert((consumer.to_owned(), property.to_owned()), carried);
    }

    /// A feature must actually keep a reference to the sketch a binder
    /// supplied. Merely preparing that sketch while trying a feature does
    /// not carry the binder's use when the feature falls back.
    pub(super) fn feature_binder_uses(&mut self, object: &Object, report: &FeatureReport) {
        if !matches!(
            report.outcome,
            ObjectOutcome::Parametric | ObjectOutcome::Partial
        ) {
            return;
        }
        // An extent up to a binder's face (mitcad#952): the face of the
        // feature the binder binds, found by its geometry, which the
        // replayed feature's extent names (mitcad#1557).
        for property in ["UpToFace", "UpToFace2"] {
            let Some(link) = object.link(property).filter(|l| l.file.is_none()) else {
                continue;
            };
            let Some(binder) = self.sources[0].object(&link.object) else {
                continue;
            };
            if !is_binder(&binder.type_name) {
                continue;
            }
            let Some(&kept) = self.replay.main.get(&object.name) else {
                continue;
            };
            // Up to a face of a body, not up to a fixed plane in its place.
            let to_face = self.doc.feature(kept).is_some_and(|f| names_face(&f.def));
            if !to_face {
                continue;
            }
            let how = format!(
                "its face carried as the face of what it binds, in the extent of {} ({})",
                object.label(),
                report.features.join(", ")
            );
            let binder = binder.name.clone();
            self.carried_binder_use(&binder, &object.name, property, kept, how);
        }
        let Some(link) = object.link("Profile") else {
            return;
        };
        let Some(binder) = self.sources[0].object(&link.object) else {
            return;
        };
        if !is_binder(&binder.type_name) || link.file.is_some() {
            return;
        }
        let Some(made) = self.replay.sketches.get(&binder.name) else {
            return;
        };
        let kept = self.replay.features.get(&object.name).and_then(|uids| {
            uids.iter().copied().find(|uid| {
                self.doc
                    .feature(*uid)
                    .is_some_and(|f| f.def.references().features.contains(&made.uid))
            })
        });
        let Some(kept) = kept.filter(|_| self.doc.feature(made.uid).is_some()) else {
            return;
        };
        let Some(bound) = self.sketches_of(binder).and_then(|s| s.into_iter().next()) else {
            return;
        };
        let Some(source) = self.replay.sketches.get(&bound.name) else {
            return;
        };
        let Some(FeatureDef::Sketch(projected)) = self.doc.feature(made.uid).map(|f| &f.def) else {
            return;
        };
        let direct = source.uid == made.uid;
        if !direct
            && (projected.projections.is_empty()
                || projected.projections.iter().any(|projection| {
                    projection
                        .source
                        .sketch()
                        .is_none_or(|s| s.sketch != source.uid)
                }))
        {
            return;
        }
        let how = format!(
            "bound sketch {} carried {} ({}) into {} ({})",
            bound.label(),
            if direct {
                "directly"
            } else {
                "through linked projections"
            },
            made.uid,
            object.label(),
            report.features.join(", ")
        );
        let binder = binder.name.clone();
        self.carried_binder_use(&binder, &object.name, "Profile", kept, how);
    }

    /// Keep the original report row/order. Only replace its skipped
    /// outcome after every meaningful use has been carried; an unused
    /// binder or one with any lost use keeps its warning.
    pub(super) fn finish_binders(&mut self) {
        for index in 0..self.report.features.len() {
            let report = &self.report.features[index];
            if !is_binder(&report.object_type) || report.outcome != ObjectOutcome::Skipped {
                continue;
            }
            let empty = BTreeMap::new();
            let carried = self
                .replay
                .binder_uses
                .get(&report.object)
                .unwrap_or(&empty);
            // Every geometric use, and those not carried (mitcad#1557: the
            // reason names them).
            let mut uses = 0;
            let mut lost: Vec<String> = Vec::new();
            for object in &self.sources[0].file.document.objects {
                for (property, link) in object.references() {
                    if link.file.is_some()
                        || link.object != report.object
                        || matches!(property, "Tip" | "Context" | "OriginFeatures")
                        || (property == "Group"
                            && (object.type_name == "PartDesign::Body"
                                || is_component(&object.type_name)
                                || is_group(object)))
                        // A sketch's support locates its plane, not its
                        // geometry. Copying an origin plane alone does not
                        // prove a binder's geometry has been carried.
                        || (is_sketch(&object.type_name)
                            && matches!(property, "AttachmentSupport" | "Support"))
                    {
                        continue;
                    }
                    uses += 1;
                    let kept = carried
                        .get(&(object.name.clone(), property.to_owned()))
                        .is_some_and(|carried| carried.retained(self.doc));
                    if !kept {
                        lost.push(format!("{}.{property}", object.label()));
                    }
                }
            }
            if uses == 0 || !lost.is_empty() {
                let why = if uses == 0 {
                    "a binder nothing uses for its geometry (a sketch attached to it takes \
                     only its plane; no link to its edges or faces is left in the file): \
                     reference geometry, nothing to carry"
                        .to_owned()
                } else {
                    format!(
                        "a binder: MitCad has none; its use by {} is not carried (sketches \
                         project its geometry linked to what it binds, features take the face \
                         or sketch it binds, mitcad#955)",
                        lost.join(", ")
                    )
                };
                let name = report.object.clone();
                self.roles
                    .insert(name, (ObjectOutcome::Skipped, Some(why.clone())));
                self.report.features[index].notes = vec![why];
                continue;
            }
            let notes: Vec<String> = carried.values().map(|use_| use_.how.clone()).collect();
            let note = notes.join("; ");
            self.roles
                .insert(report.object.clone(), (ObjectOutcome::Included, Some(note)));
            let report = &mut self.report.features[index];
            report.outcome = ObjectOutcome::Included;
            report.notes = notes;
        }
    }
}
