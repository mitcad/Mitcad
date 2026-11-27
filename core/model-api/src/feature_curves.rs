// SPDX-License-Identifier: MIT
//! The `feature_curves` query (mitcad#1405, see `commands.md`,
//! "intersection_curve"): the curves of the Intersection Curves before the
//! timeline marker as polylines, for the view to draw them. It reads the
//! results only: no geometry is computed.

use serde_json::{Value, json};

use mitcad_model::Document;
use mitcad_model::features::intersection_curve::{curve_polyline, curve_polylines};
use mitcad_model::kernel::Kernel;
use mitcad_model::sweeps::PathCurve;

/// `[{"uid", "name", "points"}]` in timeline order: each Intersection
/// Curve that evaluated, its curve as one polyline in its component's
/// coordinates. A curve whose pieces do not all join (an Isoparametric
/// Curve's several curves, mitcad#2522) also has `polylines`, one for
/// each run of pieces that join, which the view draws instead.
pub(crate) fn feature_curves_json<K: Kernel>(doc: &Document<K>) -> Value {
    Value::Array(
        doc.feature_curves()
            .into_iter()
            .map(|view| {
                let mut curve = json!({"uid": view.uid, "name": view.name});
                add_polylines(&mut curve, &view.curve);
                curve
            })
            .collect(),
    )
}

/// `points` of the curve, and `polylines` when its pieces make more than
/// one run (also the preview's `curve`).
pub(crate) fn add_polylines(target: &mut Value, curve: &[PathCurve]) {
    target["points"] = json!(curve_polyline(curve));
    let runs = curve_polylines(curve);
    if runs.len() > 1 {
        target["polylines"] = json!(runs);
    }
}
