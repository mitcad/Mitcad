// SPDX-License-Identifier: MIT
//! Feature inputs drawn where they were (mitcad#957): what a feature picked
//! and its result no longer shows (a Fillet's or a Chamfer's edges, which
//! the rounding or the bevel took away), kept with the feature's result as
//! polylines. The feature's build works them out from the body before it
//! (one kernel call, [`Kernel::edge_polylines`]); the `feature_inputs`
//! query only reads them, so selecting the feature runs no geometry
//! operation. A feature built without them (a kernel that cannot give
//! them, a document opened without building) has none.
//!
//! The kinds are fields of their own, so that other inputs (the faces of a
//! Shell or a Draft, a Hole's points) can follow without changing the
//! edges'.

use serde::{Deserialize, Serialize};

use crate::kernel::Kernel;
use crate::topo::{EdgeName, FaceName};

/// How finely an edge's curve is cut into straight pieces: the view's
/// chordal error and angle (`app/view/ViewTessellation.hpp`,
/// `kViewDeviation` and `kViewAngle`), so that the lines lie on the edges
/// the view drew before the feature.
pub const INPUT_LINE_DEFLECTION: f64 = 0.01;
pub const INPUT_LINE_ANGLE: f64 = 5.0 * std::f64::consts::PI / 180.0;

/// What a feature picked, where it was before the feature, in its
/// component's coordinates (millimetres).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FeatureInputs {
    /// Each picked edge as a polyline: the edges named and those bounding
    /// the faces named, each once.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub edges: Vec<Vec<[f64; 3]>>,
}

impl FeatureInputs {
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

/// The picked edges of a fillet's or a chamfer's sets on `shape`, the body
/// before the feature, as polylines; None when the kernel gives none (the
/// feature is built all the same and shows no lines).
pub(crate) fn picked_edges<'a, K: Kernel>(
    kernel: &K,
    shape: &K::Shape,
    sets: impl Iterator<Item = (&'a [EdgeName], &'a [FaceName])>,
) -> Option<FeatureInputs> {
    let mut edges = Vec::new();
    let mut faces = Vec::new();
    for (named, bounded) in sets {
        edges.extend_from_slice(named);
        faces.extend_from_slice(bounded);
    }
    if edges.is_empty() && faces.is_empty() {
        return None;
    }
    let lines = kernel
        .edge_polylines(
            shape,
            &edges,
            &faces,
            INPUT_LINE_DEFLECTION,
            INPUT_LINE_ANGLE,
        )
        .ok()?;
    let inputs = FeatureInputs { edges: lines };
    (!inputs.is_empty()).then_some(inputs)
}
