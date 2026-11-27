// SPDX-License-Identifier: MIT
//! The `edge_continuity` query (mitcad#2521, see `commands.md`, "Continuity
//! analysis"): how smoothly a body's faces meet across its edges, for
//! Inspect › Continuity Analysis. Each edge is sampled along its length
//! between its two faces on the exact geometry (`Kernel::edge_continuity`):
//! the gap between them (G0), the angle between their normals (G1) and the
//! difference of their normal curvatures across it (G2), the worst values
//! with where they are, and the class each edge reaches within the
//! tolerances. Bodies are taken where they are placed in the design (an
//! occurrence's body as that occurrence places it), so points are in design
//! coordinates. Nothing in the design changes.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::{Value, json};

use super::ApiError;
use super::analysis::SelectionArg;
use super::prelude::*;
use mitcad_model::Document;
use mitcad_model::ids::BodyUid;
use mitcad_model::kernel::{ContinuityClass, ContinuitySpec, EdgeSides, Kernel};
use mitcad_model::topo::{EdgeName, FaceName};

/// The samples along an edge when the query gives none, and the most.
pub(crate) const DEFAULT_SAMPLES: usize = 16;
pub(crate) const MOST_SAMPLES: usize = 1000;
/// The tolerances when the query gives none: the gap (mm), the angle
/// (degrees) and the curvature difference (percent).
pub(crate) const DEFAULT_GAP: f64 = 0.01;
pub(crate) const DEFAULT_ANGLE: f64 = 0.1;
pub(crate) const DEFAULT_CURVATURE: f64 = 2.0;
/// The classes in the order they are counted and reported.
const CLASSES: [ContinuityClass; 5] = [
    ContinuityClass::None,
    ContinuityClass::G0,
    ContinuityClass::G1,
    ContinuityClass::G2,
    ContinuityClass::Unmeasured,
];

/// The `edge_continuity` query.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContinuityQuery {
    /// What to measure: per body its edges, the edges of its faces, or
    /// every edge of it when it names neither.
    bodies: Vec<ContinuityBody>,
    #[serde(default = "default_samples")]
    samples: usize,
    #[serde(default = "default_gap")]
    gap: f64,
    #[serde(default = "default_angle")]
    angle: f64,
    #[serde(default = "default_curvature")]
    curvature: f64,
}

/// One body of the query: its uid, the occurrence that places it (a path
/// from the root by ids or names; left out, its only placement) and the
/// edges and faces asked about.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ContinuityBody {
    body: BodyUid,
    #[serde(default)]
    occurrence: Option<String>,
    #[serde(default)]
    edges: Vec<EdgeName>,
    #[serde(default)]
    faces: Vec<FaceName>,
}

fn default_samples() -> usize {
    DEFAULT_SAMPLES
}

fn default_gap() -> f64 {
    DEFAULT_GAP
}

fn default_angle() -> f64 {
    DEFAULT_ANGLE
}

fn default_curvature() -> f64 {
    DEFAULT_CURVATURE
}

/// The query's settings as the kernel takes them, or why they cannot be
/// used.
pub(crate) fn spec_of(
    samples: usize,
    gap: f64,
    angle: f64,
    curvature: f64,
) -> Result<ContinuitySpec, ApiError> {
    if !(1..=MOST_SAMPLES).contains(&samples) {
        return Err(ApiError(format!(
            "samples: from 1 to {MOST_SAMPLES}, got {samples}"
        )));
    }
    if !(gap.is_finite() && gap >= 0.0) {
        return Err(ApiError(format!("gap: 0 mm or more, got {gap}")));
    }
    if !(angle.is_finite() && (0.0..90.0).contains(&angle)) {
        return Err(ApiError(format!(
            "angle: from 0° to less than 90°, got {angle}"
        )));
    }
    if !(curvature.is_finite() && curvature >= 0.0) {
        return Err(ApiError(format!("curvature: 0 % or more, got {curvature}")));
    }
    Ok(ContinuitySpec {
        samples,
        gap,
        angle: angle.to_radians(),
        curvature: curvature / 100.0,
    })
}

fn sides_word(sides: EdgeSides) -> &'static str {
    match sides {
        EdgeSides::Shared => "shared",
        EdgeSides::Seam => "seam",
        EdgeSides::Open => "open",
    }
}

/// Answers the query.
pub(crate) fn query<K: Kernel>(
    doc: &Document<K>,
    query: &ContinuityQuery,
) -> Result<Value, ApiError> {
    let spec = spec_of(query.samples, query.gap, query.angle, query.curvature)?;
    if query.bodies.is_empty() {
        return Err(ApiError(
            "bodies names the bodies whose edges are measured; it is empty".to_owned(),
        ));
    }
    let mut edges = Vec::new();
    let mut counts: BTreeMap<&'static str, usize> =
        CLASSES.iter().map(|class| (class.as_str(), 0)).collect();
    let mut worst_gap: f64 = 0.0;
    let mut worst_angle: f64 = 0.0;
    let mut worst_curvature: f64 = 0.0;
    for (index, item) in query.bodies.iter().enumerate() {
        let mut fields = json!({"body": item.body.to_string()});
        if let Some(occurrence) = &item.occurrence {
            fields["occurrence"] = json!(occurrence);
        }
        let arg: SelectionArg = serde_json::from_value(fields)
            .map_err(|e| ApiError(format!("bodies[{index}]: {e}")))?;
        let placed = doc.placed_body(&arg, &format!("bodies[{index}]"))?;
        let measured = doc
            .kernel()
            .edge_continuity(&placed.shape, &item.edges, &item.faces, &spec)
            .map_err(|e| ApiError(format!("{}: {e}", placed.name)))?;
        let occurrence = placed.occurrence.as_ref().map(ToString::to_string);
        for edge in measured {
            *counts.entry(edge.class.as_str()).or_default() += 1;
            let measured_here = edge.samples > 0 && edge.sides != EdgeSides::Open;
            if measured_here {
                worst_gap = worst_gap.max(edge.gap);
                worst_angle = worst_angle.max(edge.angle);
                worst_curvature = worst_curvature.max(edge.curvature_share);
            }
            let at = |p: [f64; 3]| placed.placed.apply_point(p);
            let mut value = json!({
                "body": placed.uid,
                "name": placed.name,
                "edge": edge.name,
                "sides": sides_word(edge.sides),
                "class": edge.class.as_str(),
                "samples": edge.samples,
                "points": edge.points.iter().map(|p| at(*p)).collect::<Vec<_>>(),
            });
            if measured_here {
                value["gap"] = json!(edge.gap);
                value["angle"] = json!(edge.angle.to_degrees());
                value["curvature"] = json!(edge.curvature);
                value["curvature_percent"] = json!(edge.curvature_share * 100.0);
                value["gap_at"] = json!(at(edge.gap_at));
                value["angle_at"] = json!(at(edge.angle_at));
                value["curvature_at"] = json!(at(edge.curvature_at));
            }
            if let Some(path) = &occurrence {
                value["occurrence"] = json!(path);
            }
            edges.push(value);
        }
    }
    Ok(json!({
        "samples": spec.samples,
        "gap": query.gap,
        "angle": query.angle,
        "curvature": query.curvature,
        "edges": edges,
        "counts": counts,
        "worst": {
            "gap": worst_gap,
            "angle": worst_angle.to_degrees(),
            "curvature_percent": worst_curvature * 100.0,
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_are_checked_and_converted() {
        let spec = spec_of(16, 0.01, 0.1, 2.0).unwrap();
        assert_eq!(spec.samples, 16);
        assert!((spec.angle - 0.1_f64.to_radians()).abs() < 1e-15);
        assert!((spec.curvature - 0.02).abs() < 1e-15);
        assert!(spec_of(0, 0.01, 0.1, 2.0).is_err());
        assert!(spec_of(MOST_SAMPLES + 1, 0.01, 0.1, 2.0).is_err());
        assert!(spec_of(16, -0.1, 0.1, 2.0).is_err());
        assert!(spec_of(16, 0.01, 90.0, 2.0).is_err());
        assert!(spec_of(16, 0.01, 0.1, f64::NAN).is_err());
        assert!(spec_of(16, 0.0, 0.0, 0.0).is_ok());
    }

    #[test]
    fn classes_are_ordered_and_named() {
        assert!(ContinuityClass::None < ContinuityClass::G0);
        assert!(ContinuityClass::G0 < ContinuityClass::G1);
        assert!(ContinuityClass::G1 < ContinuityClass::G2);
        let words: Vec<&str> = CLASSES.iter().map(|class| class.as_str()).collect();
        assert_eq!(words, ["none", "g0", "g1", "g2", "unmeasured"]);
    }
}
