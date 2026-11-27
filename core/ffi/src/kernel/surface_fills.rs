// SPDX-License-Identifier: MIT
//! Surface modelling: Blend Surface, Boundary Surface, Match Surface and
//! Fair Surface (`geometry/include/mitcad/geometry/surface_fills.hpp`,
//! mitcad#455).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// Free edges of an opening: its index among the body's open
    /// boundaries, the faces its wanted edges bound (none: all) and points
    /// x0, y0, z0, x1, ... that pick the nearest of those (none: all).
    struct ChainOpening {
        index: i32,
        faces: Vec<String>,
        at: Vec<f64>,
    }

    /// A chain of edges of `bodies[body]`: named and of openings; `face`
    /// empty for none.
    struct ChainData {
        body: usize,
        edges: Vec<String>,
        openings: Vec<ChainOpening>,
        face: String,
    }

    /// One end of a blend; `continuity` 0 G0, 1 G1, 2 G2.
    struct BlendEndData {
        chain: ChainData,
        continuity: u8,
        bulge: f64,
    }

    /// A curve of a boundary surface: `kind` 0 sketch curves (`count`
    /// curves of the shared list from `first`, on the plane `plane`:
    /// origin, normal), 1 a chain of edges, 2 the point `point`.
    struct BoundaryData {
        kind: u8,
        first: usize,
        count: usize,
        plane: [f64; 6],
        chain: ChainData,
        point: [f64; 3],
        continuity: u8,
    }

    unsafe extern "C++" {
        include!("bridge/surface_fills.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;
        type SweepCurve = crate::kernel::sweeps::ffi::SweepCurve;

        /// A surface bridging two chains of edges.
        fn blend_surface(
            feature: &str,
            bodies: &ShapeList,
            from: &BlendEndData,
            to: &BlendEndData,
        ) -> Result<SharedPtr<Shape>>;
        /// A surface filling a loop of curves (`fill`, the loop in `u`,
        /// running through the sketch curves of `guides`, mitcad#2526) or
        /// through a network (`guides` empty).
        fn boundary_surface(
            feature: &str,
            bodies: &ShapeList,
            curves: &[SweepCurve],
            fill: bool,
            u: &[BoundaryData],
            v: &[BoundaryData],
            guides: &[BoundaryData],
        ) -> Result<SharedPtr<Shape>>;
        /// The body's face (its only one when `face` is empty) rebuilt
        /// against the target's chain.
        fn match_surface(
            body: &Shape,
            face: &str,
            bodies: &ShapeList,
            target: &ChainData,
            continuity: u8,
        ) -> Result<SharedPtr<Shape>>;
        /// The B-spline faces (all when none) faired.
        fn fair_surface(
            body: &Shape,
            faces: &[String],
            strength: f64,
            iterations: u32,
            keep_edges: bool,
            tolerance: f64,
        ) -> Result<SharedPtr<Shape>>;
    }
}

use mitcad_model::{BodyEdges, BoundaryCurve, PatchContinuity};

/// The bridge's form of boundary curves: their sketch curves appended to
/// `pieces`, the bodies of their edges to `bodies` (a chain's `body` is
/// its index there).
pub(crate) fn boundary_data<'a, S>(
    curves: &[BoundaryCurve<'a, S>],
    bodies: &mut Vec<&'a S>,
    pieces: &mut Vec<crate::kernel::sweeps::ffi::SweepCurve>,
) -> Vec<ffi::BoundaryData> {
    curves
        .iter()
        .map(|curve| match curve {
            BoundaryCurve::Curves {
                chain,
                origin,
                normal,
                continuity: c,
            } => {
                let first = pieces.len();
                pieces.extend(chain.iter().map(crate::kernel::sweeps::curve));
                ffi::BoundaryData {
                    kind: 0,
                    first,
                    count: chain.len(),
                    plane: [
                        origin[0], origin[1], origin[2], normal[0], normal[1], normal[2],
                    ],
                    chain: no_chain(),
                    point: [0.0; 3],
                    continuity: continuity(*c),
                }
            }
            BoundaryCurve::Edges {
                edges,
                continuity: c,
            } => {
                bodies.push(edges.body);
                ffi::BoundaryData {
                    kind: 1,
                    first: 0,
                    count: 0,
                    plane: [0.0; 6],
                    chain: chain(edges, bodies.len() - 1),
                    point: [0.0; 3],
                    continuity: continuity(*c),
                }
            }
            BoundaryCurve::Point(point) => ffi::BoundaryData {
                kind: 2,
                first: 0,
                count: 0,
                plane: [0.0; 6],
                chain: no_chain(),
                point: *point,
                continuity: 0,
            },
        })
        .collect()
}

/// The bridge's form of a chain of edges of `bodies[body]`.
pub(crate) fn chain<S>(edges: &BodyEdges<'_, S>, body: usize) -> ffi::ChainData {
    ffi::ChainData {
        body,
        edges: edges.edges.iter().map(ToString::to_string).collect(),
        openings: edges
            .openings
            .iter()
            .map(|o| ffi::ChainOpening {
                index: i32::try_from(o.index).unwrap_or(i32::MAX),
                faces: o.faces.iter().map(ToString::to_string).collect(),
                at: o.at.iter().flatten().copied().collect(),
            })
            .collect(),
        face: edges.face.map(ToString::to_string).unwrap_or_default(),
    }
}

pub(crate) fn continuity(continuity: PatchContinuity) -> u8 {
    match continuity {
        PatchContinuity::G0 => 0,
        PatchContinuity::G1 => 1,
        PatchContinuity::G2 => 2,
    }
}

/// No chain (a boundary curve of another kind).
pub(crate) fn no_chain() -> ffi::ChainData {
    ffi::ChainData {
        body: 0,
        edges: Vec::new(),
        openings: Vec::new(),
        face: String::new(),
    }
}
