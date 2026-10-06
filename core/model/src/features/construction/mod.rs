// SPDX-License-Identifier: MIT
//! Construction geometry (the Construct menu): planes, axes and points
//! that sketches and features refer to. A construction feature produces one
//! datum ([`crate::datum`]) and no bodies; they are named `Plane1`, `Axis1`
//! and `Point1`. Its datum is kept in the recompute state by the
//! feature's uid, so it follows parameter edits like a body does.
//!
//! Features take geometric inputs as [`GeomRef`]s (see
//! [`super::geom_ref`]): origin datums, construction features (`"F5"`),
//! faces, edges and vertices of bodies, sketch curves and points, and
//! fixed geometry.

pub mod axis;
pub mod plane;
pub mod point;

#[cfg(test)]
mod tests;

pub use axis::{AxisDefinition, ConstructionAxisDef};
pub use plane::{ConstructionPlaneDef, PlaneDefinition};
pub use point::{ConstructionPointDef, PointDefinition};

pub use super::geom_ref::{GeomRef, PathRef, Want};

use super::FeatureDef;
use crate::datum::{Datum, DatumKind};

/// The datum kind a feature definition produces, if it is construction
/// geometry.
pub fn datum_kind(def: &FeatureDef) -> Option<DatumKind> {
    match def {
        FeatureDef::ConstructionPlane(_) => Some(DatumKind::Plane),
        FeatureDef::ConstructionAxis(_) => Some(DatumKind::Axis),
        FeatureDef::ConstructionPoint(_) => Some(DatumKind::Point),
        _ => None,
    }
}

/// Checks that no reference is listed twice, e.g. a midplane between a
/// plane and itself.
pub(crate) fn distinct(references: &[&GeomRef]) -> Result<(), String> {
    for (i, reference) in references.iter().enumerate() {
        if references[..i].contains(reference) {
            return Err(format!("{reference} is selected twice"));
        }
    }
    Ok(())
}

/// Rejects datums with non-finite numbers (degenerate input that slipped
/// through), which could not be saved or displayed.
pub(crate) fn finite(datum: Datum) -> Result<Datum, String> {
    let numbers: Vec<f64> = match datum {
        Datum::Plane(p) => [p.origin, p.x_axis, p.y_axis].concat(),
        Datum::Axis(a) => [a.origin, a.direction].concat(),
        Datum::Point(p) => p.point.to_vec(),
    };
    if numbers.iter().all(|v| v.is_finite()) {
        Ok(datum)
    } else {
        Err(format!("the {} could not be computed", datum.kind()))
    }
}
