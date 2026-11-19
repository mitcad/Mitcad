// SPDX-License-Identifier: MIT
//! The `face_run` query (mitcad#1850, see `commands.md`, "Press Pull
//! continued"): how a planar face's walls continue past it, for Press
//! Pull's handle along the run and its choices, and whether the face is
//! the moving end of a feature whose own edit Press Pull may open instead:
//! also a cut-to-length BUILD part's free end, whose length Derived
//! Component edits (mitcad#2030).

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::features::{Extent, FeatureDef, GeomRef, PathExtent, RevolveExtent};
use mitcad_model::ids::{BodyUid, FeatureUid};
use mitcad_model::kernel::{FaceAlong, Kernel};
use mitcad_model::notice::FeatureError;
use mitcad_model::topo::FaceName;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FaceRunQuery {
    body: BodyUid,
    face: FaceName,
}

/// The document's methods of this module used across the api, as an
/// extension trait (mitcad#777).
pub(super) trait PressPullOps<K: Kernel> {
    fn face_run_json(&self, query: &FaceRunQuery) -> Result<Value, ApiError>;
}

/// The input of a feature's panel that moves the end `role` ("start" or
/// "end") of its `def`, empty where the end moves by no value of its own
/// (up to an object); None where it is no moving end (a one-sided
/// extrusion's or revolution's start, a full turn's, a whole path's).
pub(crate) fn moving_end_input(def: &FeatureDef, role: &str) -> Option<&'static str> {
    match def {
        FeatureDef::Extrude(e) => match (&e.extent, role) {
            (Extent::Distance { .. }, "end") => Some("distance"),
            (Extent::Symmetric { .. }, "start" | "end") => Some("distance"),
            (Extent::TwoSides { .. }, "start") => Some("distance"),
            (Extent::TwoSides { .. }, "end") => Some("distance2"),
            (Extent::ToObject { .. } | Extent::ThroughAll { .. }, "end") => Some(""),
            _ => None,
        },
        FeatureDef::Revolve(r) => match (&r.extent, role) {
            (RevolveExtent::Angle { .. }, "end") => Some("angle"),
            (RevolveExtent::Symmetric { .. }, "start" | "end") => Some("angle"),
            (RevolveExtent::TwoSides { .. }, "start") => Some("angle"),
            (RevolveExtent::TwoSides { .. }, "end") => Some("angle2"),
            (RevolveExtent::ToObject { .. }, "end") => Some(""),
            _ => None,
        },
        FeatureDef::Sweep(s) => match (&s.extent, role) {
            (PathExtent::Partial { .. }, "end") => Some("fraction"),
            (
                PathExtent::Partial {
                    fraction2: Some(_), ..
                },
                "start",
            ) => Some("fraction2"),
            _ => None,
        },
        _ => None,
    }
}

/// The `feature_end` of a cut-to-length BUILD part's end face (mitcad#2030):
/// its derived part `part`'s face `role` (`end_a`, `end_b`) where that is
/// the end its length moves: not the end the part is placed from (its
/// joint's own frame, else end A), and none while its length is bound (to
/// a face, or an edge's frame). Named after the component (a BUILD part's
/// feature has no name of its own); its input is Derived Component's
/// Length.
pub(crate) fn build_part_end<K: Kernel>(
    doc: &Document<K>,
    part: FeatureUid,
    role: &str,
) -> Option<Value> {
    if role != "end_a" && role != "end_b" {
        return None;
    }
    let entry = doc.feature(part)?;
    let FeatureDef::DerivedPart(_) = &entry.def else {
        return None;
    };
    let info = doc.part_info(entry.component)?;
    if info.part != Some(part) || !info.is_cut_to_length() || info.length.is_none() {
        return None;
    }
    if info.record.bound.iter().any(|b| doc.feature(*b).is_some()) {
        return None;
    }
    let placed_from = info
        .record
        .joint
        .and_then(|joint| match &doc.feature(joint)?.def {
            FeatureDef::Joint(j) => match &j.a.geometry {
                GeomRef::Mount { feature, name } if *feature == part => Some(name.clone()),
                _ => None,
            },
            _ => None,
        });
    let placed_from = if placed_from.as_deref() == Some("end_b") {
        "end_b"
    } else {
        "end_a"
    };
    (role != placed_from).then(|| {
        json!({"uid": part, "name": info.name, "type": entry.def.type_name(), "input": "length"})
    })
}

impl<K: Kernel> PressPullOps<K> for Document<K> {
    fn face_run_json(&self, query: &FaceRunQuery) -> Result<Value, ApiError> {
        let shape = self.require_body(query.body)?;
        let run = self
            .kernel()
            .face_run(shape, &query.face)
            .map_err(|e| ApiError(e.to_string()))?;
        let along = match run.along {
            FaceAlong::Refused { error } => {
                json!({"kind": "none", "refusal": FeatureError::from_text(&error)})
            }
            FaceAlong::Straight { direction } => {
                json!({"kind": "straight", "direction": direction})
            }
            FaceAlong::Turn {
                origin,
                axis,
                radius,
            } => json!({"kind": "turn", "origin": origin, "axis": axis, "radius": radius}),
        };
        // The face's own feature, where the face is one of its moving ends;
        // a BUILD part's free end, its length (mitcad#2030).
        let feature_end = self
            .feature(query.face.feature)
            .and_then(|entry| {
                moving_end_input(&entry.def, &query.face.role).map(|input| {
                    json!({"uid": entry.uid, "name": entry.name, "type": entry.def.type_name(),
                           "input": input})
                })
            })
            .or_else(|| build_part_end(self, query.face.feature, &query.face.role))
            .unwrap_or(Value::Null);
        Ok(json!({
            "center": run.centre,
            "normal": run.normal,
            "reach": run.reach,
            "along": along,
            "feature_end": feature_end,
        }))
    }
}
