// SPDX-License-Identifier: MIT
//! The `threads` query: the threads on the bodies at the timeline marker,
//! cosmetic or modelled, from thread features and tapped holes, with what a
//! view needs to draw them (cosmetic threads are drawn on their faces).
//! And the `thread_sizes` query: the size table of the panels (P9).

use serde_json::{Value, json};

use crate::document::Document;
use crate::features::hole::hole_face;
use crate::features::thread_table;
use crate::features::{FeatureDef, FeatureEntry, ThreadEnd, ThreadSize, ThreadStandard};
use crate::ids::BodyUid;
use crate::kernel::Kernel;
use crate::parameters::ParamId;
use crate::recompute::FeatureStatus;
use crate::topo::FaceName;

/// (length, offset, from the high end) of a partial thread.
type Part = Option<(f64, f64, bool)>;

impl<K: Kernel> Document<K> {
    pub(super) fn threads_json(&self) -> Value {
        let params = self.parameters();
        let value = |id: Option<ParamId>| id.and_then(|id| params.value(id));
        let mut threads = Vec::new();
        for entry in self.features() {
            if self.status(entry.uid) != Some(&FeatureStatus::Ok) {
                continue;
            }
            match &entry.def {
                FeatureDef::Thread(def) => {
                    let part = value(def.length).map(|length| {
                        (
                            length,
                            value(def.offset).unwrap_or(0.0),
                            def.location == ThreadEnd::HighEnd,
                        )
                    });
                    for face in &def.faces {
                        threads.extend(self.thread_json(
                            entry,
                            face.body,
                            &face.face,
                            &def.thread,
                            def.modeled,
                            part,
                        ));
                    }
                }
                FeatureDef::Hole(def) => {
                    let Some(thread) = &def.thread else {
                        continue;
                    };
                    // Measured from the hole's start: the low end of the wall's
                    // axis, which points into the material.
                    let part = value(thread.length)
                        .map(|length| (length, value(thread.offset).unwrap_or(0.0), false));
                    let walls: Vec<FaceName> = (0..def.count())
                        .map(|i| hole_face(entry.uid, i, "wall"))
                        .collect();
                    for body in self.bodies() {
                        for wall in &walls {
                            threads.extend(self.thread_json(
                                entry,
                                body.uid,
                                wall,
                                &thread.size(),
                                thread.modeled,
                                part,
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Value::Array(threads)
    }

    /// A thread on a face of a body at the marker, if the face is there.
    fn thread_json(
        &self,
        entry: &FeatureEntry,
        body: BodyUid,
        face: &FaceName,
        size: &ThreadSize,
        modeled: bool,
        part: Part,
    ) -> Option<Value> {
        let shape = self.body_shape(body)?;
        let kernel = self.kernel();
        if kernel.count_faces(shape, face).ok()? == 0 {
            return None;
        }
        let data = size.data().ok()?;
        let mut thread = json!({
            "feature": entry.uid,
            "name": entry.name,
            "body": body,
            "face": face,
            "standard": size.standard,
            "designation": data.designation,
            "class": size.class,
            "right_handed": size.right_handed,
            "modeled": modeled,
            "major_diameter": data.major,
            "minor_diameter": data.minor,
            "pitch": data.pitch,
        });
        if let Ok(cylinder) = kernel.face_cylinder(shape, face) {
            let (from, to) = match part {
                None => (0.0, cylinder.length),
                Some((length, offset, true)) => {
                    (cylinder.length - offset - length, cylinder.length - offset)
                }
                Some((length, offset, false)) => (offset, offset + length),
            };
            let at = |t: f64| -> [f64; 3] {
                std::array::from_fn(|i| cylinder.axis.origin[i] + t * cylinder.axis.direction[i])
            };
            thread["internal"] = json!(cylinder.internal);
            thread["radius"] = json!(cylinder.radius);
            thread["start"] = json!(at(from));
            thread["end"] = json!(at(to));
        }
        Some(thread)
    }
}

/// The `thread_sizes` query (P9, P11): the size table of the Hole and
/// Thread panels, every standard (or the one asked for), ISO metric first
/// and the default.
pub(super) fn thread_sizes_json(standard: Option<ThreadStandard>) -> Value {
    let standards: Vec<Value> = ThreadStandard::ALL
        .into_iter()
        .filter(|s| standard.is_none_or(|wanted| wanted == *s))
        .map(|s| {
            let sizes: Vec<Value> = thread_table::sizes(s)
                .into_iter()
                .map(|size| {
                    json!({"size": size.size, "major_diameter": size.major,
                           "designations": size.designations})
                })
                .collect();
            let (external, internal) = thread_table::classes(s);
            json!({
                "standard": s,
                "title": s.title(),
                "default": s == ThreadStandard::default(),
                "sizes": sizes,
                "classes_external": external,
                "classes_internal": internal,
                "default_class_external": s.default_class(false),
                "default_class_internal": s.default_class(true),
            })
        })
        .collect();
    json!({"standards": standards})
}
