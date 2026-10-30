// SPDX-License-Identifier: MIT
//! Drawings: views of bodies with hidden lines removed, and their round
//! holes (`geometry/include/mitcad/geometry/drawing.hpp`, mitcad#471).

#[cxx::bridge(namespace = "mitcad::bridge")]
pub mod ffi {
    /// [`mitcad_model::drawing::ViewSpec`].
    struct DrawingView {
        toward_eye: [f64; 3],
        x_axis: [f64; 3],
        hidden: bool,
        smooth: bool,
        exact: bool,
        quick: bool,
        has_section: bool,
        section_origin: [f64; 3],
        section_normal: [f64; 3],
        deflection: f64,
    }

    /// One projected edge. `kind`: 0 sharp, 1 smooth, 2 outline; `shape`:
    /// 0 a line (two points), 1 an arc (centre, radius, angles), 2 a
    /// polyline, 3 a B-spline (`degree`, `knots`, `poles` as x, y pairs,
    /// `weights`; its polyline in `points`, mitcad#853). `points` holds x,
    /// y pairs.
    struct DrawingCurve {
        visible: bool,
        kind: u8,
        shape: u8,
        points: Vec<f64>,
        center: [f64; 2],
        radius: f64,
        start: f64,
        end: f64,
        degree: u32,
        knots: Vec<f64>,
        poles: Vec<f64>,
        weights: Vec<f64>,
    }

    /// A closed loop of a cut face: x, y pairs.
    struct DrawingLoop {
        face: usize,
        points: Vec<f64>,
    }

    struct DrawingProjection {
        curves: Vec<DrawingCurve>,
        cuts: Vec<DrawingLoop>,
        polygonal: bool,
    }

    /// [`mitcad_model::drawing::RoundHole`].
    struct DrawingHole {
        face: String,
        opening: [f64; 3],
        direction: [f64; 3],
        diameter: f64,
        through: bool,
        depth: f64,
        cbore_diameter: f64,
        cbore_depth: f64,
        csink_diameter: f64,
        csink_angle: f64,
    }

    unsafe extern "C++" {
        include!("bridge/drawing.hpp");
        include!("bridge/trycatch.hpp");

        #[namespace = "mitcad::geometry"]
        type Shape = crate::kernel::shape::ffi::Shape;
        type ShapeList = crate::kernel::shape::ffi::ShapeList;

        /// geometry::project_view.
        fn project_view(shapes: &ShapeList, view: &DrawingView) -> Result<DrawingProjection>;
        /// geometry::round_holes.
        fn round_holes(shape: &Shape) -> Result<Vec<DrawingHole>>;
    }
}

use mitcad_model::drawing::{
    CutFace, RoundHole, ViewCurve, ViewEdge, ViewEdgeKind, ViewProjection, ViewSpec, ViewSpline,
};

pub fn view(spec: &ViewSpec) -> ffi::DrawingView {
    let (has_section, section_origin, section_normal) = match &spec.section {
        Some(plane) => (true, plane.origin, plane.normal),
        None => (false, [0.0; 3], [0.0, 0.0, 1.0]),
    };
    ffi::DrawingView {
        toward_eye: spec.toward_eye,
        x_axis: spec.x_axis,
        hidden: spec.hidden,
        smooth: spec.smooth,
        exact: spec.exact,
        quick: spec.quick,
        has_section,
        section_origin,
        section_normal,
        deflection: spec.deflection,
    }
}

fn pairs(points: &[f64]) -> Vec<[f64; 2]> {
    points.as_chunks::<2>().0.to_vec()
}

pub fn projection(drawn: &ffi::DrawingProjection) -> ViewProjection {
    let edges = drawn
        .curves
        .iter()
        .map(|c| ViewEdge {
            visible: c.visible,
            kind: match c.kind {
                1 => ViewEdgeKind::Smooth,
                2 => ViewEdgeKind::Outline,
                _ => ViewEdgeKind::Sharp,
            },
            curve: match c.shape {
                0 => {
                    let p = pairs(&c.points);
                    ViewCurve::Line(p[0], p[1])
                }
                1 => ViewCurve::Arc {
                    center: c.center,
                    radius: c.radius,
                    start: c.start,
                    end: c.end,
                },
                3 => ViewCurve::Spline(ViewSpline {
                    degree: c.degree,
                    knots: c.knots.clone(),
                    poles: pairs(&c.poles),
                    weights: c.weights.clone(),
                    points: pairs(&c.points),
                }),
                _ => ViewCurve::Polyline(pairs(&c.points)),
            },
        })
        .collect();
    let mut cuts: Vec<CutFace> = Vec::new();
    for lp in &drawn.cuts {
        while cuts.len() <= lp.face {
            cuts.push(Vec::new());
        }
        cuts[lp.face].push(pairs(&lp.points));
    }
    ViewProjection {
        edges,
        cuts,
        polygonal: drawn.polygonal,
    }
}

pub fn hole(h: &ffi::DrawingHole) -> RoundHole {
    RoundHole {
        face: (!h.face.is_empty()).then(|| h.face.parse().ok()).flatten(),
        opening: h.opening,
        direction: h.direction,
        diameter: h.diameter,
        through: h.through,
        depth: h.depth,
        counterbore: (h.cbore_diameter > 0.0).then_some((h.cbore_diameter, h.cbore_depth)),
        countersink: (h.csink_diameter > 0.0).then_some((h.csink_diameter, h.csink_angle)),
    }
}
