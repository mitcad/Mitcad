// SPDX-License-Identifier: MIT
//! The `feature_curves` query (mitcad#1405, see `commands.md`,
//! "intersection_curve"): the curves of the Intersection Curves before the
//! timeline marker as polylines, for the view to draw them. It reads the
//! results only: no geometry is computed.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::intersection_curve::curve_polyline;
use mitcad_model::kernel::Kernel;

/// `[{"uid", "name", "points"}]` in timeline order: each Intersection
/// Curve that evaluated, its curve as one polyline in its component's
/// coordinates.
pub(crate) fn feature_curves_json<K: Kernel>(doc: &Document<K>) -> Value {
    Value::Array(
        doc.feature_curves()
            .into_iter()
            .map(|view| {
                json!({"uid": view.uid, "name": view.name,
                       "points": curve_polyline(&view.curve)})
            })
            .collect(),
    )
}
