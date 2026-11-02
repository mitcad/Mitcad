// SPDX-License-Identifier: MIT
//! Binder reporting follows the uses that survived the replay, not the
//! support's type or a candidate that was subsequently rolled back.

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
            let Some(carried) = self.replay.binder_uses.get(&report.object) else {
                continue;
            };
            let mut uses = 0;
            let complete = self.sources[0].file.document.objects.iter().all(|object| {
                object.references().all(|(property, link)| {
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
                        return true;
                    }
                    uses += 1;
                    carried
                        .get(&(object.name.clone(), property.to_owned()))
                        .is_some_and(|carried| carried.retained(self.doc))
                })
            });
            if !complete || uses == 0 {
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
