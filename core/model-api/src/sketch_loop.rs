// SPDX-License-Identifier: MIT
//! The `sketch_loop` query (mitcad#1541, see `commands.md`, "The closed
//! loop of a sketch curve"): the curves of the smallest closed loop a
//! sketch curve bounds, which a double-click on the curve selects.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::Ref;
use mitcad_model::sketch::loops::loop_of;

/// `{"query": "sketch_loop", "sketch": "F1", "curve": "c3"}`.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SketchLoopQuery {
    sketch: FeatureUid,
    curve: Ref,
}

/// `{"curves": ["c1", …]}`: the loop's curves in its order, the picked one
/// among them; empty when the curve bounds no closed loop (or is a point,
/// an axis or no curve of the sketch).
pub(super) fn sketch_loop_json<K: Kernel>(
    doc: &Document<K>,
    query: &SketchLoopQuery,
) -> Result<Value, ApiError> {
    let uid = query.sketch;
    let entry = doc
        .feature(uid)
        .ok_or_else(|| ApiError(format!("feature {uid} does not exist")))?;
    let FeatureDef::Sketch(def) = &entry.def else {
        return Err(ApiError(format!("{} ({uid}) is not a sketch", entry.name)));
    };
    // Linked projections where their sources are now, and the geometry
    // the `sketch` query shows.
    let followed = doc.followed_sketch(uid, def);
    let def = followed.as_ref().unwrap_or(def);
    let params = doc.parameters();
    let solved = match doc.evaluated_solve(uid) {
        Some(solved) => solved,
        None => std::sync::Arc::new(
            def.solve_with(
                &mut |id| {
                    params
                        .value(id)
                        .ok_or_else(|| format!("parameter {} does not exist", params.name(id)))
                },
                &|id| params.name(id),
            )
            .map_err(|e| ApiError(format!("the sketch does not solve: {e}")))?,
        ),
    };
    let Ref::Curve(curve) = query.curve else {
        return Ok(json!({"curves": []}));
    };
    let curves: Vec<String> = loop_of(def, &solved, curve)
        .unwrap_or_default()
        .into_iter()
        .map(|id| Ref::Curve(id).to_string())
        .collect();
    Ok(json!({ "curves": curves }))
}
