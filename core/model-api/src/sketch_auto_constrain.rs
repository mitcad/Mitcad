// SPDX-License-Identifier: MIT
//! `sketch.auto_constrain` (mitcad#2019, `core/sketch/src/auto_constrain.rs`):
//! the relations a sketch's geometry has, then dimensions for the degrees
//! of freedom still free, as one undo step `Auto Constrain <sketch>`.

use serde_json::{Value, json};

use super::ApiError;
use super::sketch::report_json;
use mitcad_model::Document;
use mitcad_model::features::FeatureDef;
use mitcad_model::ids::FeatureUid;
use mitcad_model::kernel::Kernel;
use mitcad_model::sketch::Ref;
use mitcad_model::sketch::auto_constrain::{
    AutoConstrainOptions, DEFAULT_ANGLE_TOLERANCE, DEFAULT_LENGTH_TOLERANCE,
};

/// The command's fields as options: the tolerances in mm and degrees,
/// their defaults where left out.
pub(super) fn options(
    entities: Option<Vec<Ref>>,
    relations: bool,
    dimensions: bool,
    length_tolerance: Option<f64>,
    angle_tolerance: Option<f64>,
) -> Result<AutoConstrainOptions, ApiError> {
    if !relations && !dimensions {
        return Err(ApiError(
            "sketch.auto_constrain needs relations, dimensions or both".to_owned(),
        ));
    }
    let length = length_tolerance.unwrap_or(DEFAULT_LENGTH_TOLERANCE);
    let angle = angle_tolerance.unwrap_or(DEFAULT_ANGLE_TOLERANCE);
    if !(length.is_finite() && length > 0.0) {
        return Err(ApiError(
            "the length tolerance must be greater than zero".to_owned(),
        ));
    }
    if !(angle.is_finite() && angle > 0.0 && angle < 45.0) {
        return Err(ApiError(
            "the angle tolerance must be greater than zero and less than 45 degrees".to_owned(),
        ));
    }
    Ok(AutoConstrainOptions {
        length_tolerance: length,
        angle_tolerance: angle.to_radians(),
        relations,
        dimensions,
        scope: entities.filter(|e| !e.is_empty()),
    })
}

/// Runs Auto Constrain on `sketch`: the editing commands' result with what
/// it added (`added`, by type), the relations (`relations`: constraints and
/// joined ends), the joins (`joined`), the degrees of freedom left (`dof`)
/// and a `summary` in words.
pub(super) fn auto_constrain<K: Kernel>(
    doc: &mut Document<K>,
    sketch: FeatureUid,
    options: AutoConstrainOptions,
) -> Result<Value, ApiError> {
    let (made, report) = doc.edit_sketch(sketch, "Auto Constrain {sketch}", |edit| {
        edit.change = "Auto Constrain".to_owned();
        edit.auto_constrain(&options)
    })?;
    let def = match doc.feature(sketch).map(|f| &f.def) {
        Some(FeatureDef::Sketch(def)) => Some(def),
        _ => None,
    };
    let mut value = report_json(&report, def);
    // The sketch as solved when the edit was stored: its degrees of freedom.
    let dof = report.status.dof;
    value["added"] = json!(made.added);
    value["relations"] = json!(made.relations());
    value["joined"] = json!(made.joined);
    value["dof"] = json!(dof);
    value["summary"] =
        json!(mitcad_model::sketch::auto_constrain::AutoConstrained { dof, ..made }.summary());
    Ok(value)
}
