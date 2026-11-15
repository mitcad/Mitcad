// SPDX-License-Identifier: MIT
//! The `face_run` query (mitcad#1850, see `commands.md`, "Press Pull
//! continued"): how a planar face's walls continue past it, for Press
//! Pull's handle along the run and its choices, and whether the face is
//! the moving end of a feature whose own edit Press Pull may open instead.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::features::{Extent, FeatureDef, PathExtent, RevolveExtent};
use mitcad_model::ids::BodyUid;
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
        // The face's own feature, where the face is one of its moving ends.
        let feature_end = self
            .feature(query.face.feature)
            .and_then(|entry| {
                moving_end_input(&entry.def, &query.face.role).map(|input| {
                    json!({"uid": entry.uid, "name": entry.name, "type": entry.def.type_name(),
                           "input": input})
                })
            })
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
