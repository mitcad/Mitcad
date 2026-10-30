// SPDX-License-Identifier: MIT
//! The `feature_inputs` query (mitcad#957, see `commands.md`, "Feature
//! inputs"): what a feature picked, where it was before the feature (a
//! Fillet's or a Chamfer's edges as polylines), as its last build kept it.
//! It reads the stored result only: no geometry is computed, so the
//! application can ask it whenever a feature is selected or hovered on the
//! timeline.

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use mitcad_model::Document;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FeatureInputsQuery {
    uid: FeatureUid,
}

/// `{"uid", "name", "stored", "edges"}`: `stored` false (and `edges`
/// empty) when the feature's last build kept nothing.
pub(crate) fn feature_inputs_json<K: Kernel>(
    doc: &Document<K>,
    query: &FeatureInputsQuery,
) -> Result<Value, ApiError> {
    let entry = doc
        .feature(query.uid)
        .ok_or_else(|| ApiError(format!("feature {} does not exist", query.uid)))?;
    let inputs = doc.feature_inputs(query.uid);
    let edges: &[Vec<[f64; 3]>] = inputs.map_or(&[], |i| i.edges.as_slice());
    Ok(json!({
        "uid": query.uid,
        "name": entry.name,
        "stored": inputs.is_some(),
        "edges": edges,
    }))
}
